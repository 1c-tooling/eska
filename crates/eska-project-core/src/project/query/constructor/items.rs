//! Group intentions run against a private staging model and publish one checked revision.

use std::collections::BTreeSet;

use super::{
    Action, Clause, ClauseKind, CommentDecision, Constructor, Error, Expression, ItemSetOperation,
    JoinKind, MAX_ITEM_SET_ITEMS, Selection, lex, visual,
};
use crate::project::metadata_edit::{EditPlan, Replacement};

/// Staging retains no source authority and cannot partially replace the live constructor.
pub(super) struct Staged {
    pub value: Constructor,
    pub focus: Vec<Selection>,
}

/// All indices belong to the supplied snapshot; intermediate revisions are private implementation details.
pub(super) fn stage(value: &Constructor, action: &Action) -> Result<Option<Staged>, Error> {
    match action {
        Action::MovePredicate { selection, target } => {
            super::predicates::move_stage(value, *selection, *target).map(Some)
        }
        Action::EditItems {
            statement,
            clause,
            items,
            operation,
            comments,
        } => edit(value, *statement, *clause, items, *operation, *comments).map(Some),
        Action::AddSourceFields {
            statement,
            source,
            fields,
        } => add_source(value, *statement, source, fields).map(Some),
        _ => Ok(None),
    }
}

/// A private counter avoids overflowing the public counter once per intermediate item.
fn staging(value: &Constructor) -> Constructor {
    let mut staged = value.clone();
    staged.revision = 1;
    staged
}

/// A set cannot cross a clause boundary or silently normalize duplicate/stale item identities.
fn edit(
    value: &Constructor,
    statement: usize,
    clause: usize,
    selected: &[usize],
    operation: ItemSetOperation,
    comments: Option<CommentDecision>,
) -> Result<Staged, Error> {
    let owner = value
        .model
        .statements
        .get(statement)
        .ok_or(Error::InvalidSelection)?;
    if owner.unavailable.is_some() {
        return Err(Error::Unsupported);
    }
    let section = owner.clauses.get(clause).ok_or(Error::InvalidSelection)?;
    let items = selected_items(section, selected)?;
    let count = section.items.len();
    if matches!(operation, ItemSetOperation::Remove)
        && items.len() == count
        && matches!(section.kind, ClauseKind::Fields | ClauseKind::Sources)
    {
        return Err(Error::RequiredItem);
    }
    let mut staged = staging(value);
    let mut order: Vec<_> = (0..count).collect();
    let focus = match operation {
        ItemSetOperation::Remove => {
            for &item in items.iter().rev() {
                staged.apply(
                    staged.revision,
                    &Action::Remove {
                        selection: Selection {
                            statement,
                            clause,
                            item,
                        },
                        comments,
                    },
                )?;
            }
            Vec::new()
        }
        ItemSetOperation::Move { to } => {
            if to > count {
                return Err(Error::InvalidSelection);
            }
            let to = (0..to).filter(|item| !items.contains(item)).count();
            let mut desired: Vec<_> = order
                .iter()
                .copied()
                .filter(|item| !items.contains(item))
                .collect();
            desired.splice(to..to, items.iter().copied());
            reorder(&mut staged, statement, clause, &mut order, &desired)?;
            (to..to + items.len()).collect()
        }
        ItemSetOperation::Copy { to } => {
            if to > count || section.kind == ClauseKind::Index {
                return Err(Error::InvalidSelection);
            }
            for &item in &items {
                let text = copied_fragment(value, &staged, statement, clause, item)?;
                copy_total_projection(value, &mut staged, statement, clause, item)?;
                staged.apply(
                    staged.revision,
                    &Action::Append {
                        statement,
                        clause: section.kind,
                        text,
                        join: JoinKind::Inner,
                    },
                )?;
            }
            order.extend(count..count + items.len());
            let mut desired: Vec<_> = (0..count).collect();
            desired.splice(to..to, count..count + items.len());
            reorder(&mut staged, statement, clause, &mut order, &desired)?;
            (to..to + items.len()).collect()
        }
    };
    Ok(Staged {
        value: staged,
        focus: focus
            .into_iter()
            .map(|item| Selection {
                statement,
                clause,
                item,
            })
            .collect(),
    })
}

/// A copied total writes into a matching output column; both additions share the outer transaction.
fn copy_total_projection(
    original: &Constructor,
    staged: &mut Constructor,
    statement: usize,
    clause: usize,
    item: usize,
) -> Result<(), Error> {
    let owner = &original.model.statements[statement];
    let section = &owner.clauses[clause];
    if !matches!(section.kind, ClauseKind::Totals | ClauseKind::TotalGroups) {
        return Ok(());
    }
    let alias = match section.items[item].visual(original.text(), section) {
        Some(
            Expression::TotalField {
                alias, expression, ..
            }
            | Expression::TotalGroup {
                alias, expression, ..
            },
        ) => total_name(&alias, &expression)?,
        _ => return Err(Error::Unsupported),
    };
    let first = original.model.scopes[owner.scope].branches[0];
    let fields = original.model.statements[first]
        .clauses
        .iter()
        .position(|section| section.kind == ClauseKind::Fields)
        .ok_or(Error::Unsupported)?;
    let column = original.model.statements[first].clauses[fields]
        .items
        .iter()
        .position(|item| {
            item.output_name(
                original.text(),
                &original.model.statements[first].clauses[fields],
            )
            .is_some_and(|name| name.to_lowercase() == alias.to_lowercase())
        })
        .ok_or(Error::Unsupported)?;
    let text = copied_fragment(original, staged, first, fields, column)?;
    staged.apply(
        staged.revision,
        &Action::Append {
            statement: first,
            clause: ClauseKind::Fields,
            text,
            join: JoinKind::Inner,
        },
    )?;
    Ok(())
}

/// Validate every member before an operation can enter its private staging model.
fn selected_items(section: &Clause, selected: &[usize]) -> Result<BTreeSet<usize>, Error> {
    if matches!(
        section.kind,
        ClauseKind::Into | ClauseKind::Destroy | ClauseKind::Joins
    ) {
        return Err(Error::Unsupported);
    }
    if selected.is_empty() {
        return Err(Error::InvalidSelection);
    }
    if selected.len() > MAX_ITEM_SET_ITEMS {
        return Err(Error::TooLarge);
    }
    let items: BTreeSet<_> = selected.iter().copied().collect();
    if items.len() != selected.len()
        || items
            .last()
            .is_some_and(|index| *index >= section.items.len())
    {
        return Err(Error::InvalidSelection);
    }
    Ok(items)
}

/// Stable set order is enforced by the core, including UNION synchronization and OVERALL constraints.
fn reorder(
    staged: &mut Constructor,
    statement: usize,
    clause: usize,
    order: &mut Vec<usize>,
    desired: &[usize],
) -> Result<(), Error> {
    for (to, &item) in desired.iter().enumerate() {
        let from = order
            .iter()
            .position(|value| *value == item)
            .ok_or(Error::InvalidSelection)?;
        if from == to {
            continue;
        }
        staged.apply(
            staged.revision,
            &Action::Move {
                selection: Selection {
                    statement,
                    clause,
                    item: from,
                },
                to,
            },
        )?;
        let item = order.remove(from);
        order.insert(to, item);
    }
    Ok(())
}

/// Copies retain original comment bytes; only an independently allocated alias changes.
fn copied_fragment(
    original: &Constructor,
    staged: &Constructor,
    statement: usize,
    clause: usize,
    item: usize,
) -> Result<String, Error> {
    let owner = &original.model.statements[statement];
    let section = &owner.clauses[clause];
    let selected = &section.items[item];
    let expression = selected
        .visual(original.text(), section)
        .ok_or(Error::Unsupported)?;
    if matches!(expression, Expression::Overall)
        || selected.wildcard_source(original.text(), section).is_some()
    {
        return Err(Error::Unsupported);
    }
    let raw = original.text()[selected.range.clone()].to_owned();
    let (alias, base) = match &expression {
        Expression::Field { alias, .. } => (
            alias.as_str(),
            if alias.is_empty() {
                selected
                    .output_name(original.text(), section)
                    .unwrap_or_else(|| "Field".into())
            } else {
                alias.clone()
            },
        ),
        Expression::Source { alias, table } | Expression::VirtualSource { alias, table, .. } => (
            alias.as_str(),
            if alias.is_empty() {
                table.rsplit('.').next().unwrap_or("Source").to_owned()
            } else {
                alias.clone()
            },
        ),
        Expression::SubquerySource { alias } => (
            alias.as_str(),
            if alias.is_empty() {
                "NestedQuery".into()
            } else {
                alias.clone()
            },
        ),
        Expression::TotalField {
            alias, expression, ..
        }
        | Expression::TotalGroup {
            alias, expression, ..
        } => (alias.as_str(), total_name(alias, expression)?),
        _ => return Ok(raw),
    };
    let new_alias = fresh_alias(staged, statement, section.kind, &base)?;
    let tokens: Vec<_> = lex::tokens(&raw)?
        .into_iter()
        .filter(|token| token.kind != lex::Kind::Comment)
        .collect();
    let range = if alias.is_empty() {
        let at = tokens.last().ok_or(Error::InvalidExpression)?.range.end;
        at..at
    } else {
        tokens
            .windows(2)
            .rev()
            .find(|pair| {
                pair[0].depth == 0
                    && pair[0].keyword(&raw, &["AS", "КАК"])
                    && raw[pair[1].range.clone()] == *alias
            })
            .map(|pair| pair[1].range.clone())
            .or_else(|| {
                tokens
                    .last()
                    .filter(|token| {
                        token.depth == 0
                            && token.kind == lex::Kind::Word
                            && raw[token.range.clone()] == *alias
                    })
                    .map(|token| token.range.clone())
            })
            .ok_or(Error::InvalidExpression)?
    };
    let text = if alias.is_empty() {
        format!(" {} {new_alias}", if owner.russian { "КАК" } else { "AS" })
    } else {
        new_alias
    };
    let plan = EditPlan::from_replacements(&raw, vec![Replacement { range, text }])
        .map_err(|_| Error::InvalidExpression)?;
    Ok(plan.output().to_owned())
}

/// Unaliased totals name a simple output column; complex unbound expressions stay unsupported.
fn total_name(alias: &str, expression: &str) -> Result<String, Error> {
    let name = if alias.is_empty() {
        expression.trim()
    } else {
        alias
    };
    visual::identifier(name).map_err(|_| Error::Unsupported)?;
    Ok(name.to_owned())
}

/// Alias allocation checks the complete UNION result namespace or all current source bindings.
fn fresh_alias(
    value: &Constructor,
    statement: usize,
    kind: ClauseKind,
    base: &str,
) -> Result<String, Error> {
    visual::identifier(base)?;
    let owner = value
        .model
        .statements
        .get(statement)
        .ok_or(Error::InvalidSelection)?;
    let mut names = BTreeSet::new();
    if kind == ClauseKind::Sources {
        for binding in owner.source_bindings(value.text()) {
            names.insert(binding.alias.to_lowercase());
            if let Some(name) = binding.alias.rsplit('.').next() {
                names.insert(name.to_lowercase());
            }
        }
    } else {
        for &branch in &value.model.scopes[owner.scope].branches {
            for section in &value.model.statements[branch].clauses {
                for item in &section.items {
                    if let Some(name) = item.output_name(value.text(), section) {
                        names.insert(name.to_lowercase());
                    }
                    if let Some(
                        Expression::TotalField { alias, .. } | Expression::TotalGroup { alias, .. },
                    ) = item.visual(value.text(), section)
                        && !alias.is_empty()
                    {
                        names.insert(alias.to_lowercase());
                    }
                }
            }
        }
    }
    let mut alias = base.to_owned();
    let mut suffix = 1usize;
    while names.contains(&alias.to_lowercase()) {
        suffix = suffix.checked_add(1).ok_or(Error::TooLarge)?;
        alias = format!("{base}{suffix}");
    }
    Ok(alias)
}

/// A metadata drop adds its source and selected fields in one transaction, using the allocated alias.
fn add_source(
    value: &Constructor,
    statement: usize,
    source: &Expression,
    fields: &[String],
) -> Result<Staged, Error> {
    if fields.is_empty() {
        return Err(Error::InvalidSelection);
    }
    if fields.len() > MAX_ITEM_SET_ITEMS {
        return Err(Error::TooLarge);
    }
    let (Expression::Source { alias, .. } | Expression::VirtualSource { alias, .. }) = source
    else {
        return Err(Error::Unsupported);
    };
    if !alias.is_empty() && fresh_alias(value, statement, ClauseKind::Sources, alias)? != *alias {
        return Err(Error::InvalidExpression);
    }
    for field in fields {
        for part in field.split('.') {
            visual::identifier(part)?;
        }
    }
    let mut staged = staging(value);
    staged.apply(
        staged.revision,
        &Action::Compose {
            statement,
            selection: None,
            expression: source.clone(),
        },
    )?;
    let owner = &staged.model.statements[statement];
    let binding = owner
        .source_bindings(staged.text())
        .into_iter()
        .rfind(|source| owner.clauses[source.clause].kind == ClauseKind::Sources)
        .ok_or(Error::SourceRequired)?;
    let clause = owner
        .clauses
        .iter()
        .position(|clause| clause.kind == ClauseKind::Fields)
        .ok_or(Error::Unsupported)?;
    let before = owner.clauses[clause].items.len();
    staged.apply(
        staged.revision,
        &Action::AppendFields {
            statement,
            fields: fields
                .iter()
                .map(|field| format!("{}.{field}", binding.alias))
                .collect(),
        },
    )?;
    let after = staged.model.statements[statement].clauses[clause]
        .items
        .len();
    Ok(Staged {
        value: staged,
        focus: (before..after)
            .map(|item| Selection {
                statement,
                clause,
                item,
            })
            .collect(),
    })
}

/// Copy permissions describe the recognized subset without allocating another query model.
pub(super) fn copy_unavailable(value: &Constructor, selection: Selection) -> Option<Error> {
    let Some(owner) = value.model.statements.get(selection.statement) else {
        return Some(Error::InvalidSelection);
    };
    if owner.unavailable.is_some() {
        return Some(Error::Unsupported);
    }
    let Some(clause) = owner.clauses.get(selection.clause) else {
        return Some(Error::InvalidSelection);
    };
    let Some(item) = clause.items.get(selection.item) else {
        return Some(Error::InvalidSelection);
    };
    if matches!(
        clause.kind,
        ClauseKind::Into | ClauseKind::Destroy | ClauseKind::Joins | ClauseKind::Index
    ) || matches!(
        item.visual(value.text(), clause),
        None | Some(Expression::Overall)
    ) || item.wildcard_source(value.text(), clause).is_some()
    {
        Some(Error::Unsupported)
    } else {
        None
    }
}
