//! Structured source edits update only proven alias references and JOIN keywords, retaining all comment bytes.

use std::ops::Range;

use super::super::{
    Action, Clause, ClauseKind, Error, JoinKind, Model, Statement, editing,
    lex::{self, Kind},
};
use super::Expression;
use crate::project::metadata_edit::{EditPlan, Replacement};

/// Extend an item edit with the dependent syntax that belongs to the same structured intention.
pub(in super::super) fn augment(
    input: &str,
    model: &Model,
    action: &Action,
    plan: EditPlan,
) -> Result<EditPlan, Error> {
    let Action::Compose {
        statement,
        selection: Some(selection),
        expression,
    } = action
    else {
        return Ok(plan);
    };
    let owner = model
        .statements
        .get(*statement)
        .ok_or(Error::InvalidSelection)?;
    let clause = owner
        .clauses
        .get(selection.clause)
        .ok_or(Error::InvalidSelection)?;
    let item = clause
        .items
        .get(selection.item)
        .ok_or(Error::InvalidSelection)?;
    let mut edits = plan.replacements().to_vec();
    if let Expression::Join { join, .. } | Expression::VirtualJoin { join, .. } = expression {
        edits.extend(join_header(input, clause, *join, owner.russian)?);
    }
    let old = item.visual(input, clause);
    if let (
        Some(
            Expression::Source {
                table: old_table,
                alias: old_alias,
            }
            | Expression::VirtualSource {
                table: old_table,
                alias: old_alias,
                ..
            }
            | Expression::VirtualJoin {
                table: old_table,
                alias: old_alias,
                ..
            }
            | Expression::Join {
                table: old_table,
                alias: old_alias,
                ..
            },
        ),
        Expression::Source { table, alias }
        | Expression::Join { table, alias, .. }
        | Expression::VirtualSource { table, alias, .. }
        | Expression::VirtualJoin { table, alias, .. },
    ) = (old, expression)
    {
        let previous = implicit_alias(&old_table, &old_alias);
        let replacement = implicit_alias(table, alias);
        if previous.to_lowercase() != replacement.to_lowercase() {
            if owner
                .source_bindings(input)
                .iter()
                .any(|binding| binding.alias.to_lowercase() == replacement.to_lowercase())
            {
                return Err(Error::InvalidExpression);
            }
            let protected = source_prefixes(input, owner)?;
            let found = references(input, &owner.range, previous, replacement)?;
            for reference in found {
                if !protected
                    .iter()
                    .any(|range| range.contains(&reference.range.start))
                    && !edits
                        .iter()
                        .any(|edit| edit.range.contains(&reference.range.start))
                {
                    edits.push(reference);
                }
            }
            if matches!(
                expression,
                Expression::Join { .. } | Expression::VirtualJoin { .. }
            ) {
                for edit in edits.iter_mut().filter(|edit| edit.range == item.range) {
                    let start = on_end(&edit.text)?.unwrap_or(edit.text.len());
                    let replacements =
                        references(&edit.text, &(start..edit.text.len()), previous, replacement)?;
                    let renamed = EditPlan::from_replacements(&edit.text, replacements)
                        .map_err(|_| Error::InvalidExpression)?;
                    renamed.output().clone_into(&mut edit.text);
                }
            }
        }
    }
    edits.sort_by_key(|edit| edit.range.start);
    EditPlan::from_replacements(input, edits).map_err(|_| Error::InvalidExpression)
}

/// A missing explicit alias retains the fully qualified source name; no SQL is inferred by an adapter.
fn implicit_alias<'a>(table: &'a str, alias: &'a str) -> &'a str {
    if alias.is_empty() {
        table.split('(').next().unwrap_or(table)
    } else {
        alias
    }
}

/// Change the JOIN type without deleting comments between LEFT/OUTER/JOIN or their Russian equivalents.
fn join_header(
    input: &str,
    clause: &Clause,
    kind: JoinKind,
    russian: bool,
) -> Result<Vec<Replacement>, Error> {
    let header = &input[clause.range.start..clause.body.start];
    let tokens = lex::tokens(header)?;
    let first = tokens
        .iter()
        .find(|token| token.kind == Kind::Word)
        .ok_or(Error::InvalidExpression)?;
    let keyword = editing::join_keyword(kind, russian)
        .split_whitespace()
        .next()
        .ok_or(Error::InvalidExpression)?;
    let mut edits = Vec::new();
    if first.keyword(header, &["JOIN", "СОЕДИНЕНИЕ"]) {
        if !matches!(kind, JoinKind::Inner) {
            let at = clause.range.start + first.range.start;
            edits.push(Replacement {
                range: at..at,
                text: format!("{keyword} "),
            });
        }
    } else if header[first.range.clone()].to_uppercase() != keyword {
        edits.push(Replacement {
            range: clause.range.start + first.range.start..clause.range.start + first.range.end,
            text: keyword.to_owned(),
        });
    }
    if matches!(kind, JoinKind::Inner) {
        for token in tokens
            .iter()
            .filter(|token| token.keyword(header, &["OUTER", "ВНЕШНЕЕ"]))
        {
            edits.push(Replacement {
                range: clause.range.start + token.range.start..clause.range.start + token.range.end,
                text: String::new(),
            });
        }
    }
    Ok(edits)
}

/// Metadata paths and source declarations are not alias references; JOIN predicates remain eligible.
fn source_prefixes(input: &str, owner: &Statement) -> Result<Vec<Range<usize>>, Error> {
    let mut ranges = Vec::new();
    for clause in &owner.clauses {
        if clause.kind == ClauseKind::Sources {
            ranges.extend(clause.items.iter().map(|item| item.range.clone()));
        }
        if clause.kind == ClauseKind::Joins {
            for item in &clause.items {
                let text = &input[item.range.clone()];
                ranges
                    .push(item.range.start..item.range.start + on_end(text)?.unwrap_or(text.len()));
            }
        }
    }
    Ok(ranges)
}

/// Locate ON through the shared lexer, never by matching inside a comment, function argument or string.
fn on_end(input: &str) -> Result<Option<usize>, Error> {
    Ok(lex::tokens(input)?
        .iter()
        .find(|token| token.depth == 0 && token.keyword(input, &["ON", "ПО"]))
        .map(|token| token.range.end))
}

/// Only a word followed by a qualification dot is renamed; strings, comments and unqualified field names remain exact.
fn references(
    input: &str,
    range: &Range<usize>,
    old: &str,
    new: &str,
) -> Result<Vec<Replacement>, Error> {
    let text = &input[range.clone()];
    let tokens: Vec<_> = lex::tokens(text)?
        .into_iter()
        .filter(|token| token.kind != Kind::Comment)
        .collect();
    let prefix = lex::tokens(old)?;
    if prefix.is_empty()
        || prefix.len() % 2 == 0
        || prefix.iter().enumerate().any(|(index, token)| {
            token.kind
                != if index % 2 == 0 {
                    Kind::Word
                } else {
                    Kind::Symbol('.')
                }
        })
    {
        return Err(Error::Unsupported);
    }
    let mut edits = Vec::new();
    for (index, window) in tokens.windows(prefix.len() + 1).enumerate() {
        if index > 0 && tokens[index - 1].kind == Kind::Symbol('.') {
            continue;
        }
        if window[prefix.len()].kind != Kind::Symbol('.')
            || !prefix.iter().zip(window).all(|(expected, actual)| {
                let expected_text = &old[expected.range.clone()];
                let actual_text = &text[actual.range.clone()];
                expected.kind == actual.kind
                    && expected_text.to_lowercase() == actual_text.to_lowercase()
            })
        {
            continue;
        }
        let start = window[0].range.start;
        let end = window[prefix.len() - 1].range.end;
        if lex::tokens(&text[start..end])?
            .iter()
            .any(|token| token.kind == Kind::Comment)
        {
            return Err(Error::Unsupported);
        }
        edits.push(Replacement {
            range: range.start + start..range.start + end,
            text: new.to_owned(),
        });
    }
    Ok(edits)
}
