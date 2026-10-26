//! Minimal edits preserve separators, comments and all unsupported package text.

use crate::project::metadata_edit::{EditPlan, Replacement};

use super::{
    Action, Clause, ClauseKind, CommentDecision, Error, Item, JoinKind, Model, Selection,
    Statement,
    lex::{self, Kind},
};

/// Resolve indices only against the supplied immutable model revision.
pub(super) fn plan(input: &str, model: &Model, action: &Action) -> Result<EditPlan, Error> {
    let edits = match action {
        Action::SetJoinKind { .. }
        | Action::MovePredicate { .. }
        | Action::SetPredicate { .. }
        | Action::EditItems { .. }
        | Action::AddSourceFields { .. }
        | Action::Compose { .. }
        | Action::SetDcs { .. }
        | Action::AddTotals { .. }
        | Action::RemoveTotals { .. }
        | Action::MoveTotals { .. }
        | Action::ComposeTotal { .. }
        | Action::AppendTotal { .. }
        | Action::SetInto { .. }
        | Action::AddStatement { .. }
        | Action::RemoveStatement { .. }
        | Action::MoveStatement { .. }
        | Action::AppendFields { .. }
        | Action::AddUnion { .. }
        | Action::SetUnion { .. }
        | Action::RemoveUnion { .. } => {
            return Err(Error::InvalidExpression);
        }
        Action::Append {
            statement,
            clause,
            text,
            join,
        } => append(
            input,
            statement_at(model, *statement)?,
            *clause,
            text,
            *join,
        )?,
        Action::Replace { selection, text } => replace(input, item_at(model, *selection)?.1, text)?,
        Action::Remove {
            selection,
            comments,
        } => {
            let (clause, _) = item_at(model, *selection)?;
            if clause.kind == ClauseKind::TotalGroups && clause.items.len() == 1 {
                remove_totals(
                    input,
                    statement_at(model, selection.statement)?,
                    selection.clause,
                    *comments,
                )?
            } else {
                remove(input, clause, selection.item, *comments)?
            }
        }
        Action::Move { selection, to } => {
            let (clause, _) = item_at(model, *selection)?;
            move_item(input, clause, selection.item, *to)?
        }
    };
    let added: usize = edits.iter().map(|edit| edit.text.len()).sum();
    if input.len().saturating_add(added) > super::super::MAX_SNAPSHOT_BYTES * 2 {
        return Err(Error::TooLarge);
    }
    EditPlan::from_replacements(input, edits).map_err(|_| Error::InvalidExpression)
}

/// Unsupported nodes are never partially rewritten by a visual operation.
fn statement_at(model: &Model, index: usize) -> Result<&Statement, Error> {
    let statement = model.statements.get(index).ok_or(Error::InvalidSelection)?;
    if statement.unavailable.is_some() {
        return Err(Error::Unsupported);
    }
    Ok(statement)
}

/// A selection cannot address another statement, clause or revision's row.
fn item_at(model: &Model, selection: Selection) -> Result<(&Clause, &Item), Error> {
    let clause = statement_at(model, selection.statement)?
        .clauses
        .get(selection.clause)
        .ok_or(Error::InvalidSelection)?;
    let item = clause
        .items
        .get(selection.item)
        .ok_or(Error::InvalidSelection)?;
    Ok((clause, item))
}

/// Fragments cannot escape their selected item into another clause, statement or SCD extension.
pub(super) fn fragment(text: &str) -> Result<(), Error> {
    if text.trim().is_empty() || text.len() > super::super::MAX_SNAPSHOT_BYTES {
        return Err(Error::InvalidExpression);
    }
    let tokens = lex::tokens(text)?;
    if !tokens.iter().any(|token| token.kind != Kind::Comment)
        || tokens.iter().any(|token| {
            matches!(token.kind, Kind::Symbol(';' | '{' | '}' | '\'' | '`'))
                || (token.depth == 0
                    && (token.kind == Kind::Symbol(',')
                        || token.keyword(
                            text,
                            &[
                                "SELECT",
                                "ВЫБРАТЬ",
                                "FROM",
                                "ИЗ",
                                "WHERE",
                                "ГДЕ",
                                "JOIN",
                                "СОЕДИНЕНИЕ",
                                "GROUP",
                                "СГРУППИРОВАТЬ",
                                "ORDER",
                                "УПОРЯДОЧИТЬ",
                                "UNION",
                                "ОБЪЕДИНИТЬ",
                                "INTO",
                                "ПОМЕСТИТЬ",
                                "HAVING",
                                "ИМЕЮЩИЕ",
                                "TOTALS",
                                "ИТОГИ",
                            ],
                        )))
        })
    {
        return Err(Error::InvalidExpression);
    }
    Ok(())
}

/// Add only a requested node; language and line endings come from its existing statement.
pub(super) fn append(
    input: &str,
    statement: &Statement,
    kind: ClauseKind,
    text: &str,
    join: JoinKind,
) -> Result<Vec<Replacement>, Error> {
    if let Some(error) = statement.append_unavailable(kind) {
        return Err(error);
    }
    fragment(text)?;
    let newline = newline(input);
    if matches!(kind, ClauseKind::Totals | ClauseKind::TotalGroups)
        && !statement
            .clauses
            .iter()
            .any(|clause| clause.kind == ClauseKind::Totals)
    {
        let position = statement
            .clauses
            .last()
            .and_then(|clause| clause.items.last())
            .map_or(statement.range.end, |item| item.range.end);
        let totals = ClauseKind::Totals.keyword(statement.russian);
        let groups = ClauseKind::TotalGroups.keyword(statement.russian);
        let body = if kind == ClauseKind::Totals {
            format!(
                "{text}{newline}{groups} {}",
                if statement.russian {
                    "ОБЩИЕ"
                } else {
                    "OVERALL"
                }
            )
        } else {
            format!("{newline}{groups} {text}")
        };
        return Ok(vec![Replacement {
            range: position..position,
            text: format!("{newline}{totals} {body}{newline}"),
        }]);
    }
    if kind != ClauseKind::Joins
        && let Some(clause) = statement.clauses.iter().find(|clause| clause.kind == kind)
    {
        let Some(item) = clause.items.last() else {
            return Ok(vec![Replacement {
                range: clause.body.start..clause.body.start,
                text: format!(" {text}{newline}"),
            }]);
        };
        let separator = if matches!(kind, ClauseKind::Conditions | ClauseKind::Having) {
            if statement.russian { "И" } else { "AND" }
        } else {
            ","
        };
        if matches!(kind, ClauseKind::Conditions | ClauseKind::Having)
            && super::predicates::complex(input, statement)
        {
            let raw = &input[clause.body.clone()];
            return Ok(vec![Replacement {
                range: clause.body.clone(),
                text: format!(" ({raw}{newline}){separator} ({text}){newline}"),
            }]);
        }
        let text = if matches!(kind, ClauseKind::Conditions | ClauseKind::Having) {
            format!("{newline}{separator} {text}")
        } else {
            format!(
                "{},{newline}{text}",
                if trailing_comment(input, item) {
                    newline
                } else {
                    ""
                }
            )
        };
        return Ok(vec![Replacement {
            range: item.range.end..item.range.end,
            text: safe_boundary(input, item.range.end, text)?,
        }]);
    }
    let position = statement
        .clauses
        .iter()
        .find(|clause| clause.kind.rank() > kind.rank())
        .map_or_else(
            || {
                statement
                    .clauses
                    .last()
                    .and_then(|clause| clause.items.last())
                    .map_or(statement.range.end, |item| item.range.end)
            },
            |clause| clause.range.start,
        );
    let keyword = if kind == ClauseKind::Joins {
        join_keyword(join, statement.russian)
    } else {
        kind.keyword(statement.russian)
    };
    Ok(vec![Replacement {
        range: position..position,
        text: format!("{newline}{keyword} {text}{newline}"),
    }])
}

/// Existing comment bytes remain attached when an expression changes; unchanged expressions are exact no-ops.
pub(super) fn replace(input: &str, item: &Item, text: &str) -> Result<Vec<Replacement>, Error> {
    if item.expression(input) == text.trim() {
        return Ok(Vec::new());
    }
    fragment(text)?;
    let comments = retained_comments(input, &item.comments);
    Ok(vec![Replacement {
        range: item.range.clone(),
        text: safe_boundary(input, item.range.end, format!("{comments}{text}"))?,
    }])
}

/// An empty last grouping removes the whole totals section; no orphan BY or implicit totals are left behind.
pub(super) fn remove_totals(
    input: &str,
    statement: &Statement,
    group: usize,
    decision: Option<CommentDecision>,
) -> Result<Vec<Replacement>, Error> {
    let totals = statement
        .clauses
        .get(group.checked_sub(1).ok_or(Error::InvalidSelection)?)
        .filter(|clause| clause.kind == ClauseKind::Totals)
        .ok_or(Error::InvalidSelection)?;
    let groups = statement
        .clauses
        .get(group)
        .filter(|clause| clause.kind == ClauseKind::TotalGroups)
        .ok_or(Error::InvalidSelection)?;
    let comments: Vec<_> = totals
        .comments
        .iter()
        .chain(&groups.comments)
        .cloned()
        .collect();
    if !comments.is_empty() && decision.is_none() {
        return Err(Error::CommentsRequireDecision);
    }
    let text = if matches!(decision, Some(CommentDecision::Keep)) {
        retained_comments(input, &comments)
    } else {
        String::new()
    };
    Ok(vec![Replacement {
        range: totals.range.start..groups.range.end,
        text,
    }])
}

/// Required lists cannot become empty, and deleting a commented node always requires an explicit disposition.
fn remove(
    input: &str,
    clause: &Clause,
    index: usize,
    decision: Option<CommentDecision>,
) -> Result<Vec<Replacement>, Error> {
    if clause.items.len() == 1 && matches!(clause.kind, ClauseKind::Fields | ClauseKind::Sources) {
        return Err(Error::RequiredItem);
    }
    let item = &clause.items[index];
    let comments = if clause.items.len() == 1 && clause.kind != ClauseKind::Totals {
        &clause.comments
    } else {
        &item.comments
    };
    if !comments.is_empty() && decision.is_none() {
        return Err(Error::CommentsRequireDecision);
    }
    let retained = if matches!(decision, Some(CommentDecision::Keep)) {
        retained_comments(input, comments)
    } else {
        String::new()
    };
    let range = if clause.items.len() == 1 && clause.kind == ClauseKind::Totals {
        item.range.clone()
    } else if clause.items.len() == 1 {
        clause.range.clone()
    } else if let Some(next) = clause.items.get(index + 1) {
        item.range.start..next.range.start
    } else {
        clause.items[index - 1].range.end..item.range.end
    };
    let text = if !retained.is_empty() && index + 1 == clause.items.len() {
        format!("{}{retained}", newline(input))
    } else {
        retained
    };
    Ok(vec![Replacement { range, text }])
}

/// Reorder raw nodes while retaining each original separator; JOIN dependency order cannot be inferred safely.
fn move_item(
    input: &str,
    clause: &Clause,
    from: usize,
    to: usize,
) -> Result<Vec<Replacement>, Error> {
    if to >= clause.items.len() {
        return Err(Error::InvalidSelection);
    }
    if from == to {
        return Ok(Vec::new());
    }
    if let Some(error) = clause.move_unavailable(input, from, to) {
        return Err(error);
    }
    let mut order: Vec<_> = (0..clause.items.len()).collect();
    let moved = order.remove(from);
    order.insert(to, moved);
    clause
        .items
        .iter()
        .enumerate()
        .filter(|(index, _)| order[*index] != *index)
        .map(|(index, item)| {
            Ok(Replacement {
                range: item.range.clone(),
                text: safe_boundary(
                    input,
                    item.range.end,
                    input[clause.items[order[index]].range.clone()].to_owned(),
                )?,
            })
        })
        .collect()
}

/// Retained comments have a safe terminating newline even at a removed node's last statement boundary.
fn retained_comments(input: &str, comments: &[std::ops::Range<usize>]) -> String {
    comments.iter().fold(String::new(), |mut output, comment| {
        output.push_str(&input[comment.clone()]);
        output.push_str(newline(input));
        output
    })
}

/// Only newly inserted syntax follows this physical line ending; unchanged ranges stay byte-exact.
pub(super) fn newline(input: &str) -> &'static str {
    if input.contains("\r\n") { "\r\n" } else { "\n" }
}

/// JOIN type is a domain choice; an adapter never constructs localized query keywords.
pub(super) const fn join_keyword(kind: JoinKind, russian: bool) -> &'static str {
    match (kind, russian) {
        (JoinKind::Inner, false) => "INNER JOIN",
        (JoinKind::Left, false) => "LEFT JOIN",
        (JoinKind::Right, false) => "RIGHT JOIN",
        (JoinKind::Full, false) => "FULL JOIN",
        (JoinKind::Inner, true) => "ВНУТРЕННЕЕ СОЕДИНЕНИЕ",
        (JoinKind::Left, true) => "ЛЕВОЕ СОЕДИНЕНИЕ",
        (JoinKind::Right, true) => "ПРАВОЕ СОЕДИНЕНИЕ",
        (JoinKind::Full, true) => "ПОЛНОЕ СОЕДИНЕНИЕ",
    }
}

/// A line comment moved before a comma or keyword must not swallow that following syntax.
fn safe_boundary(input: &str, offset: usize, mut text: String) -> Result<String, Error> {
    let tokens = lex::tokens(&text)?;
    if tokens.last().is_some_and(|token| {
        token.kind == Kind::Comment && text[token.range.clone()].starts_with("//")
    }) && !text.ends_with('\n')
        && !input[offset..]
            .trim_start_matches([' ', '\t'])
            .starts_with(['\r', '\n'])
    {
        text.push_str(newline(input));
    }
    Ok(text)
}

/// The original newline is a separator, so insertion before it needs its own line break before a comma.
fn trailing_comment(input: &str, item: &Item) -> bool {
    item.comments.last().is_some_and(|comment| {
        comment.end == item.range.end && input[comment.clone()].starts_with("//")
    })
}
