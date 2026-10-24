//! Each sequential totals pair is an independently addressed node in the shared result tail.

use super::super::{
    Action, ClauseKind, Error, Expression, Model, Selection, Statement, TotalSection, editing,
    visual,
};
use crate::project::metadata_edit::{EditPlan, Replacement};

/// Section intentions are planned against one immutable model and published atomically by the constructor.
pub(in super::super) fn plan(
    input: &str,
    model: &Model,
    action: &Action,
) -> Result<Option<EditPlan>, Error> {
    let edits = match action {
        Action::AddTotals { statement, after } => add(input, owner(model, *statement)?, *after)?,
        Action::RemoveTotals {
            statement,
            section,
            comments,
        } => {
            let owner = owner(model, *statement)?;
            let selected = section_at(owner, *section)?;
            editing::remove_totals(input, owner, selected.groups, *comments)?
        }
        Action::MoveTotals {
            statement,
            section,
            to,
        } => move_section(input, owner(model, *statement)?, *section, *to)?,
        Action::ComposeTotal {
            statement,
            section,
            selection,
            expression,
        } => {
            let owner = owner(model, *statement)?;
            let selected = section_at(owner, *section)?;
            compose(input, model, *statement, &selected, *selection, expression)?
        }
        Action::AppendTotal {
            statement,
            section,
            clause,
            text,
        } => {
            if text.len() > 16_384 {
                return Err(Error::TooLarge);
            }
            let owner = owner(model, *statement)?;
            append(input, owner, &section_at(owner, *section)?, *clause, text)?
        }
        _ => return Ok(None),
    };
    Ok(Some(
        EditPlan::from_replacements(input, edits).map_err(|_| Error::InvalidExpression)?,
    ))
}

/// The last branch alone owns result totals; nested or opaque syntax never grants edit authority.
fn owner(model: &Model, index: usize) -> Result<&Statement, Error> {
    let owner = model.statements.get(index).ok_or(Error::InvalidSelection)?;
    if let Some(error) = owner.append_unavailable(ClauseKind::Totals) {
        return Err(error);
    }
    Ok(owner)
}

/// Section ordinals and clause indices are valid only for this supplied model revision.
fn section_at(owner: &Statement, index: usize) -> Result<TotalSection, Error> {
    owner
        .total_sections()
        .into_iter()
        .nth(index)
        .ok_or(Error::InvalidSelection)
}

/// An empty expression list requests automatic totals; OVERALL supplies a complete, editable grouping list.
fn add(input: &str, owner: &Statement, after: Option<usize>) -> Result<Vec<Replacement>, Error> {
    let sections = owner.total_sections();
    if sections.len() >= 32 {
        return Err(Error::TooLarge);
    }
    let position = if let Some(after) = after {
        sections
            .get(after)
            .ok_or(Error::InvalidSelection)?
            .range
            .end
    } else {
        sections.last().map_or_else(
            || {
                owner
                    .clauses
                    .last()
                    .and_then(|clause| clause.items.last())
                    .map_or(owner.range.end, |item| item.range.end)
            },
            |section| section.range.end,
        )
    };
    let newline = editing::newline(input);
    Ok(vec![Replacement {
        range: position..position,
        text: format!(
            "{newline}{}{newline}{} {}{newline}",
            ClauseKind::Totals.keyword(owner.russian),
            ClauseKind::TotalGroups.keyword(owner.russian),
            if owner.russian {
                "ОБЩИЕ"
            } else {
                "OVERALL"
            }
        ),
    }])
}

/// Only total expressions can enter a section; selections must point into this exact pair.
fn compose(
    input: &str,
    model: &Model,
    statement: usize,
    section: &TotalSection,
    selection: Option<Selection>,
    expression: &Expression,
) -> Result<Vec<Replacement>, Error> {
    let kind = expression.clause();
    let clause = match kind {
        ClauseKind::Totals => section.fields,
        ClauseKind::TotalGroups => section.groups,
        _ => return Err(Error::InvalidExpression),
    };
    if let Some(selection) = selection {
        if selection.statement != statement || selection.clause != clause {
            return Err(Error::InvalidSelection);
        }
        let lowered = visual::compose(input, model, statement, Some(selection), expression)?;
        return Ok(editing::plan(input, model, &lowered)?
            .replacements()
            .to_vec());
    }
    let owner = &model.statements[statement];
    if matches!(expression, Expression::Overall) {
        let groups = &owner.clauses[section.groups];
        if groups
            .items
            .iter()
            .any(|item| matches!(item.visual(input, groups), Some(Expression::Overall)))
        {
            return Ok(Vec::new());
        }
        let at = groups
            .items
            .first()
            .ok_or(Error::InvalidExpression)?
            .range
            .start;
        return Ok(vec![Replacement {
            range: at..at,
            text: format!(
                "{},{}",
                if owner.russian {
                    "ОБЩИЕ"
                } else {
                    "OVERALL"
                },
                editing::newline(input)
            ),
        }]);
    }
    let text = expression.text(owner.russian)?;
    if text.len() > 16_384 {
        return Err(Error::TooLarge);
    }
    append(input, owner, section, kind, &text)
}

/// Reuse ordinary lossless insertion on a view containing only this pair, retaining original byte addresses.
fn append(
    input: &str,
    owner: &Statement,
    section: &TotalSection,
    kind: ClauseKind,
    text: &str,
) -> Result<Vec<Replacement>, Error> {
    if !matches!(kind, ClauseKind::Totals | ClauseKind::TotalGroups) {
        return Err(Error::InvalidExpression);
    }
    let mut selected = owner.clone();
    selected.clauses = owner
        .clauses
        .iter()
        .enumerate()
        .filter(|(index, clause)| {
            !matches!(clause.kind, ClauseKind::Totals | ClauseKind::TotalGroups)
                || [section.fields, section.groups].contains(index)
        })
        .map(|(_, clause)| clause.clone())
        .collect();
    editing::append(input, &selected, kind, text, super::super::JoinKind::Inner)
}

/// Move complete pairs together; new separating newlines prevent a moved line comment from hiding the next header.
fn move_section(
    input: &str,
    owner: &Statement,
    from: usize,
    to: usize,
) -> Result<Vec<Replacement>, Error> {
    let sections = owner.total_sections();
    if from >= sections.len() || to >= sections.len() {
        return Err(Error::InvalidSelection);
    }
    if from == to {
        return Ok(Vec::new());
    }
    let mut order: Vec<_> = (0..sections.len()).collect();
    let selected = order.remove(from);
    order.insert(to, selected);
    let newline = editing::newline(input);
    Ok((from.min(to)..=from.max(to))
        .map(|index| Replacement {
            range: sections[index].range.clone(),
            text: format!(
                "{}{newline}",
                input[sections[order[index]].range.clone()].trim_end()
            ),
        })
        .collect())
}

/// A section edit cannot alter branches, nested scopes, non-total clauses or the number of unrelated sections.
pub(in super::super) fn check_sections(
    before: &Model,
    after: &Model,
    action: &Action,
) -> Result<(), Error> {
    let index = action.statement();
    let old = before
        .statements
        .get(index)
        .ok_or(Error::InvalidSelection)?;
    let new = after
        .statements
        .get(index)
        .ok_or(Error::InvalidExpression)?;
    let delta = match action {
        Action::AddTotals { .. } => 1,
        Action::RemoveTotals { .. } => -1,
        _ => 0,
    };
    let ordinary = |owner: &Statement| {
        owner
            .clauses
            .iter()
            .filter(|c| !matches!(c.kind, ClauseKind::Totals | ClauseKind::TotalGroups))
            .map(|c| c.kind)
            .collect::<Vec<_>>()
    };
    if before.statements.len() != after.statements.len()
        || before.scopes.len() != after.scopes.len()
        || new.unavailable.is_some()
        || new.scope != old.scope
        || new.branch != old.branch
        || new.total_sections().len().checked_add_signed(-delta) != Some(old.total_sections().len())
        || ordinary(old) != ordinary(new)
    {
        return Err(Error::InvalidExpression);
    }
    Ok(())
}

/// Return the surviving section ordinal rather than asking an adapter to remap shifted clause indices.
pub(in super::super) fn section_focus(
    before: &Model,
    after: &Model,
    action: &Action,
) -> Option<usize> {
    let old = before.statements.get(action.statement())?;
    let count = after
        .statements
        .get(action.statement())?
        .total_sections()
        .len();
    if count == 0 {
        return None;
    }
    let index = match action {
        Action::AddTotals { after, .. } => after.map_or(count - 1, |index| index + 1),
        Action::RemoveTotals { section, .. }
        | Action::ComposeTotal { section, .. }
        | Action::AppendTotal { section, .. } => *section,
        Action::MoveTotals { to, .. } => *to,
        Action::Remove { selection, .. } => old
            .total_sections()
            .iter()
            .position(|section| [section.fields, section.groups].contains(&selection.clause))?,
        _ => return None,
    };
    Some(index.min(count - 1))
}
