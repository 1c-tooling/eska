//! Nested source controls change only their outer alias/predicate; the inner query stays byte-exact.

mod outputs;
pub(super) use outputs::augment as output_plan;

use super::{
    Action, ClauseKind, Error, Expression, Item, Model, editing,
    lex::{self, Kind},
    visual,
};
use crate::project::metadata_edit::{EditPlan, Replacement};
use std::ops::Range;

/// Ranges are relative to the original item, including comments inside its inner SELECT.
pub(super) struct Source {
    pub body: Range<usize>,
    pub alias: String,
    alias_range: Option<Range<usize>>,
    on: Option<Range<usize>>,
}

/// Recognize one parenthesized SELECT and its own terminal alias, never AS or ON in the inner query.
pub(super) fn source(input: &str) -> Option<Source> {
    let tokens: Vec<_> = lex::tokens(input)
        .ok()?
        .into_iter()
        .filter(|token| token.kind != Kind::Comment)
        .collect();
    let open = tokens.first()?;
    if open.kind != Kind::Symbol('(') || !tokens.get(1)?.keyword(input, &["SELECT", "ВЫБРАТЬ"])
    {
        return None;
    }
    let close = tokens
        .iter()
        .position(|token| token.kind == Kind::Symbol(')') && token.depth == 0)?;
    let mut index = close + 1;
    if tokens
        .get(index)
        .is_some_and(|token| token.keyword(input, &["AS", "КАК"]))
    {
        index += 1;
    }
    let alias = tokens
        .get(index)
        .filter(|token| token.kind == Kind::Word && !token.keyword(input, &["ON", "ПО"]));
    if alias.is_some() {
        index += 1;
    }
    let on = tokens
        .get(index)
        .filter(|token| token.keyword(input, &["ON", "ПО"]));
    if index != tokens.len() && on.is_none() {
        return None;
    }
    Some(Source {
        body: open.range.end..tokens[close].range.start,
        alias: alias.map_or_else(String::new, |token| input[token.range.clone()].to_owned()),
        alias_range: alias.map(|token| token.range.clone()),
        on: on.map(|token| token.range.clone()),
    })
}

/// Structured subquery edits never route the entire inner text through a comment-stripping expression control.
pub(super) fn plan(input: &str, model: &Model, action: &Action) -> Result<Option<EditPlan>, Error> {
    let Action::Compose {
        statement,
        selection: Some(selection),
        expression,
    } = action
    else {
        return Ok(None);
    };
    let (alias, condition) = match expression {
        Expression::SubquerySource { alias } => (alias, None),
        Expression::SubqueryJoin {
            alias,
            left,
            comparison,
            right,
            ..
        } => (
            alias,
            Some(Expression::Condition {
                left: left.clone(),
                comparison: *comparison,
                right: right.clone(),
            }),
        ),
        _ => return Ok(None),
    };
    visual::identifier(alias)?;
    let owner = model
        .statements
        .get(*statement)
        .ok_or(Error::InvalidSelection)?;
    if owner.unavailable.is_some() || selection.statement != *statement {
        return Err(Error::InvalidSelection);
    }
    let clause = owner
        .clauses
        .get(selection.clause)
        .ok_or(Error::InvalidSelection)?;
    if !matches!(clause.kind, ClauseKind::Sources | ClauseKind::Joins) {
        return Err(Error::InvalidSelection);
    }
    let item = clause
        .items
        .get(selection.item)
        .ok_or(Error::InvalidSelection)?;
    let raw = &input[item.range.clone()];
    let existing = source(raw).ok_or(Error::InvalidSelection)?;
    let mut edits = Vec::new();
    if &existing.alias != alias {
        let (range, text) = existing.alias_range.map_or_else(
            || {
                (
                    existing.body.end + 1..existing.body.end + 1,
                    format!(" {} {alias}", if owner.russian { "КАК" } else { "AS" }),
                )
            },
            |range| (range, alias.clone()),
        );
        edits.push(Replacement {
            range: item.range.start + range.start..item.range.start + range.end,
            text,
        });
    }
    if let Some(condition) = condition {
        let on = existing.on.ok_or(Error::InvalidSelection)?;
        let range = item.range.start + on.end..item.range.end;
        let predicate = Item {
            comments: item
                .comments
                .iter()
                .filter(|comment| comment.start >= range.start)
                .cloned()
                .collect(),
            range,
        };
        let replacements = editing::replace(input, &predicate, &condition.text(owner.russian)?)?;
        edits.extend(replacements.into_iter().map(|mut edit| {
            edit.text.insert(0, ' ');
            edit
        }));
    }
    Ok(Some(
        EditPlan::from_replacements(input, edits).map_err(|_| Error::InvalidExpression)?,
    ))
}
