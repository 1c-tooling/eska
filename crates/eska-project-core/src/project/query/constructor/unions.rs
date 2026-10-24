//! UNION operations preserve result tails and keep positional columns aligned across branches.

use super::{
    Action, ClauseKind, CommentDecision, Error, Expression, Model, Selection, editing,
    lex::{self, Kind},
};
use crate::project::metadata_edit::{EditPlan, Replacement};

/// Branch operations change only their owned separator/body; comments require an explicit deletion choice.
pub(super) fn plan(input: &str, model: &Model, action: &Action) -> Result<Option<EditPlan>, Error> {
    let statement = match action {
        Action::AddUnion { statement, .. }
        | Action::SetUnion { statement, .. }
        | Action::RemoveUnion { statement, .. } => *statement,
        _ => return Ok(None),
    };
    let owner = model
        .statements
        .get(statement)
        .ok_or(Error::InvalidSelection)?;
    let scope = model
        .scopes
        .get(owner.scope)
        .ok_or(Error::InvalidSelection)?;
    if owner.unavailable.is_some() {
        return Err(Error::Unsupported);
    }
    let edits = match action {
        Action::AddUnion { all, .. } => {
            let body = input[owner.range.start..owner.range.end.min(scope.tail)].trim();
            let keyword = if owner.russian {
                "ОБЪЕДИНИТЬ"
            } else {
                "UNION"
            };
            let modifier = if *all {
                if owner.russian { " ВСЕ" } else { " ALL" }
            } else {
                ""
            };
            vec![Replacement {
                range: scope.tail..scope.tail,
                text: {
                    let newline = editing::newline(input);
                    format!("{newline}{keyword}{modifier}{newline}{body}{newline}")
                },
            }]
        }
        Action::SetUnion { all, .. } => {
            let separator = scope
                .unions
                .get(owner.branch.checked_sub(1).ok_or(Error::InvalidSelection)?)
                .ok_or(Error::InvalidSelection)?;
            match (&separator.all, all) {
                (Some(range), false) => vec![Replacement {
                    range: range.clone(),
                    text: String::new(),
                }],
                (None, true) => vec![Replacement {
                    range: separator.keyword.end..separator.keyword.end,
                    text: if input[separator.keyword.clone()].eq_ignore_ascii_case("UNION") {
                        " ALL".into()
                    } else {
                        " ВСЕ".into()
                    },
                }],
                _ => Vec::new(),
            }
        }
        Action::RemoveUnion { comments, .. } => remove(input, model, statement, *comments)?,
        _ => return Err(Error::InvalidSelection),
    };
    Ok(Some(
        EditPlan::from_replacements(input, edits).map_err(|_| Error::InvalidExpression)?,
    ))
}

/// Removing the first branch preserves shared output aliases in its successor and never removes global ordering/totals.
fn remove(
    input: &str,
    model: &Model,
    statement: usize,
    decision: Option<CommentDecision>,
) -> Result<Vec<Replacement>, Error> {
    let owner = &model.statements[statement];
    let scope = &model.scopes[owner.scope];
    if scope.branches.len() < 2 {
        return Err(Error::RequiredItem);
    }
    let range = if owner.branch == 0 {
        owner.range.start..scope.unions[0].range.end
    } else {
        scope.unions[owner.branch - 1].range.start..owner.range.end.min(scope.tail)
    };
    let comments: Vec<_> = lex::tokens(&input[range.clone()])?
        .into_iter()
        .filter(|token| token.kind == Kind::Comment)
        .collect();
    if !comments.is_empty() && decision.is_none() {
        return Err(Error::CommentsRequireDecision);
    }
    let mut retained = String::new();
    if matches!(decision, Some(CommentDecision::Keep)) {
        for comment in comments {
            retained.push_str(
                &input[range.start + comment.range.start..range.start + comment.range.end],
            );
            if !retained.ends_with('\n') {
                retained.push('\n');
            }
        }
    }
    let mut edits = vec![Replacement {
        range,
        text: retained,
    }];
    if owner.branch == 0 {
        let first = owner
            .clauses
            .iter()
            .find(|clause| clause.kind == ClauseKind::Fields)
            .ok_or(Error::Unsupported)?;
        let next = &model.statements[scope.branches[1]];
        let fields = next
            .clauses
            .iter()
            .find(|clause| clause.kind == ClauseKind::Fields)
            .ok_or(Error::Unsupported)?;
        for (old, new) in first.items.iter().zip(&fields.items) {
            if let Some(name) = old.output_name(input, first)
                && new.output_name(input, fields).as_ref() != Some(&name)
                && let Some(Expression::Field {
                    expression,
                    aggregate,
                    ..
                }) = new.visual(input, fields)
            {
                let mut text = if let Some(aggregate) = aggregate {
                    format!("{}({expression})", aggregate.keyword(next.russian))
                } else {
                    expression
                };
                text.push_str(if next.russian { " КАК " } else { " AS " });
                text.push_str(&name);
                edits.extend(editing::replace(input, new, &text)?);
            }
        }
    }
    Ok(edits)
}

/// Known branch counts and query nesting must remain consistent after an atomic branch operation.
pub(super) fn check_shape(before: &Model, after: &Model, action: &Action) -> Result<(), Error> {
    let (statement, change) = match action {
        Action::AddUnion { statement, .. } => (*statement, 1),
        Action::SetUnion { statement, .. } => (*statement, 0),
        Action::RemoveUnion { statement, .. } => (*statement, -1),
        _ => return Err(Error::InvalidSelection),
    };
    let old = &before.scopes[before.statements[statement].scope];
    let new = after
        .scopes
        .get(before.statements[statement].scope)
        .ok_or(Error::InvalidExpression)?;
    if new.branches.len().checked_add_signed(-change) != Some(old.branches.len())
        || new.parent != old.parent
        || before
            .scopes
            .iter()
            .filter(|scope| scope.parent.is_none())
            .count()
            != after
                .scopes
                .iter()
                .filter(|scope| scope.parent.is_none())
                .count()
    {
        return Err(Error::InvalidExpression);
    }
    Ok(())
}

/// Adding, removing or moving a projection column is one common positional intention across a UNION.
pub(super) fn field_actions(
    input: &str,
    model: &Model,
    action: &Action,
) -> Result<Option<Vec<Action>>, Error> {
    let statement = match action {
        Action::Append {
            statement,
            clause: ClauseKind::Fields,
            ..
        }
        | Action::Compose {
            statement,
            selection: None,
            expression: Expression::Field { .. },
        } => *statement,
        Action::Remove { selection, .. } | Action::Move { selection, .. }
            if model
                .statements
                .get(selection.statement)
                .and_then(|owner| owner.clauses.get(selection.clause))
                .is_some_and(|clause| clause.kind == ClauseKind::Fields) =>
        {
            selection.statement
        }
        _ => return Ok(None),
    };
    let owner = model
        .statements
        .get(statement)
        .ok_or(Error::InvalidSelection)?;
    let scope = &model.scopes[owner.scope];
    if scope.branches.len() < 2 {
        return Ok(None);
    }
    let name = append_name(input, owner, action);
    let mut actions = Vec::new();
    for &branch in &scope.branches {
        if branch == statement {
            actions.push(action.clone());
            continue;
        }
        let clause = model.statements[branch]
            .clauses
            .iter()
            .position(|clause| clause.kind == ClauseKind::Fields)
            .ok_or(Error::Unsupported)?;
        let action = match action {
            Action::Remove {
                selection,
                comments,
            } => Action::Remove {
                selection: Selection {
                    statement: branch,
                    clause,
                    item: selection.item,
                },
                comments: *comments,
            },
            Action::Move { selection, to } => Action::Move {
                selection: Selection {
                    statement: branch,
                    clause,
                    item: selection.item,
                },
                to: *to,
            },
            _ => Action::Compose {
                statement: branch,
                selection: None,
                expression: Expression::Field {
                    expression: "NULL".into(),
                    alias: name.clone(),
                    aggregate: None,
                },
            },
        };
        actions.push(action);
    }
    Ok(Some(actions))
}

/// A newly shared result column has a stable alias even when its value exists in only one branch initially.
fn append_name(input: &str, owner: &super::Statement, action: &Action) -> String {
    let fields = owner
        .clauses
        .iter()
        .find(|clause| clause.kind == ClauseKind::Fields);
    let count = fields.map_or(0, |clause| clause.items.len());
    let requested = match action {
        Action::Compose {
            expression: Expression::Field {
                expression, alias, ..
            },
            ..
        } => {
            if alias.is_empty() {
                expression.rsplit('.').next().map(str::to_owned)
            } else {
                Some(alias.clone())
            }
        }
        Action::Append { text, .. } => {
            let source = format!("SELECT {text}");
            Model::parse(&source).ok().and_then(|model| {
                model
                    .statements
                    .first()
                    .and_then(|statement| statement.clauses.first())
                    .and_then(|clause| {
                        clause
                            .items
                            .first()
                            .and_then(|item| item.output_name(&source, clause))
                    })
            })
        }
        _ => None,
    };
    let _ = input;
    requested
        .filter(|name| super::visual::identifier(name).is_ok())
        .unwrap_or_else(|| format!("Field{}", count + 1))
}
