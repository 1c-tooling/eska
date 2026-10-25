//! Package intentions preserve original separators and rename only proven references in one table lifetime.

use super::super::{
    Action, ClauseKind, CommentDecision, Error, Model, PackageOperation, editing,
    lex::{self, Kind},
    visual,
};
use crate::project::metadata_edit::{EditPlan, Replacement};

/// Package operations bypass row-shape checks, but retain dependency checks and one atomic publication.
pub(in super::super) fn plan(
    input: &str,
    model: &Model,
    action: &Action,
) -> Result<Option<EditPlan>, Error> {
    let edits = match action {
        Action::SetInto {
            statement,
            name,
            comments,
        } => destination(input, model, *statement, name.as_deref(), *comments)?,
        Action::AddStatement {
            statement,
            operation,
            name,
        } => add(input, model, *statement, *operation, name.as_deref())?,
        Action::RemoveStatement {
            statement,
            comments,
        } => remove(input, model, *statement, *comments)?,
        Action::MoveStatement { statement, to } => move_statement(input, model, *statement, *to)?,
        Action::Append {
            statement,
            clause: ClauseKind::Into,
            text,
            ..
        } => destination(input, model, *statement, Some(text), None)?,
        Action::Replace { selection, text } => {
            let Some(clause) = model
                .statements
                .get(selection.statement)
                .and_then(|s| s.clauses.get(selection.clause))
            else {
                return Ok(None);
            };
            if clause.kind == ClauseKind::Into && selection.item == 0 {
                destination(input, model, selection.statement, Some(text), None)?
            } else if clause.kind == ClauseKind::Destroy && selection.item == 0 {
                if !super::identifier(text) {
                    return Err(Error::InvalidExpression);
                }
                editing::replace(input, &clause.items[0], text)?
            } else {
                return Ok(None);
            }
        }
        Action::Remove {
            selection,
            comments,
        } => {
            let Some(clause) = model
                .statements
                .get(selection.statement)
                .and_then(|s| s.clauses.get(selection.clause))
            else {
                return Ok(None);
            };
            if selection.item != 0 {
                return Ok(None);
            }
            if clause.kind == ClauseKind::Into {
                destination(input, model, selection.statement, None, *comments)?
            } else if clause.kind == ClauseKind::Destroy {
                remove(input, model, selection.statement, *comments)?
            } else {
                return Ok(None);
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(
        EditPlan::from_replacements(input, edits).map_err(|_| Error::InvalidExpression)?,
    ))
}

/// Select/destroy insertion uses the selected query's spelling and leaves existing package separators intact.
fn add(
    input: &str,
    model: &Model,
    statement: usize,
    operation: PackageOperation,
    name: Option<&str>,
) -> Result<Vec<Replacement>, Error> {
    let scope = root(model, statement)?;
    let owner = &model.statements[scope.branches[0]];
    if name.is_some_and(|name| !super::identifier(name)) {
        return Err(Error::InvalidExpression);
    }
    let newline = editing::newline(input);
    let text = match operation {
        PackageOperation::Select => {
            let select = if owner.russian {
                "ВЫБРАТЬ 1 КАК Value"
            } else {
                "SELECT 1 AS Value"
            };
            name.map_or_else(
                || select.to_owned(),
                |name| {
                    format!(
                        "{select}{newline}{} {name}",
                        ClauseKind::Into.keyword(owner.russian)
                    )
                },
            )
        }
        PackageOperation::Destroy => format!(
            "{} {}",
            ClauseKind::Destroy.keyword(owner.russian),
            name.ok_or(Error::InvalidExpression)?
        ),
    };
    let previous = &input[scope.range.clone()];
    let prefix = if lex::tokens(previous)?.last().is_some_and(|token| {
        token.kind == Kind::Comment
            && previous[token.range.clone()].starts_with("//")
            && !previous[token.range.end..].contains(['\r', '\n'])
    }) {
        newline
    } else {
        ""
    };
    Ok(vec![Replacement {
        range: scope.range.end..scope.range.end,
        text: format!("{prefix};{newline}{text}"),
    }])
}

/// A temporary destination is owned by the first UNION branch; removing it cannot strand consumers.
fn destination(
    input: &str,
    model: &Model,
    statement: usize,
    name: Option<&str>,
    comments: Option<CommentDecision>,
) -> Result<Vec<Replacement>, Error> {
    let scope = root(model, statement)?;
    let first = scope.branches[0];
    if model.package_operation(first) != Some(PackageOperation::Select) {
        return Err(Error::Unsupported);
    }
    if name.is_some_and(|name| !super::identifier(name)) {
        return Err(Error::InvalidExpression);
    }
    let owner = &model.statements[first];
    let selected = owner
        .clauses
        .iter()
        .enumerate()
        .find(|(_, c)| c.kind == ClauseKind::Into);
    match (selected, name) {
        (None, None) => Ok(Vec::new()),
        (None, Some(name)) => Ok(editing::plan(
            input,
            model,
            &Action::Append {
                statement: first,
                clause: ClauseKind::Into,
                text: name.to_owned(),
                join: super::super::JoinKind::default(),
            },
        )?
        .replacements()
        .to_vec()),
        (Some((index, _)), None) => Ok(editing::plan(
            input,
            model,
            &Action::Remove {
                selection: super::super::Selection {
                    statement: first,
                    clause: index,
                    item: 0,
                },
                comments,
            },
        )?
        .replacements()
        .to_vec()),
        (Some((_, clause)), Some(name)) => {
            let old = clause.items[0].expression(input);
            let mut edits = editing::replace(input, &clause.items[0], name)?;
            if old != name {
                rename(input, model, owner.scope, &old, name, &mut edits)?;
            }
            Ok(edits)
        }
    }
}

/// Only consumers and the matching DROP in this creation lifetime are renamed; strings and later recreations stay exact.
fn rename(
    input: &str,
    model: &Model,
    producer: usize,
    old: &str,
    new: &str,
    edits: &mut Vec<Replacement>,
) -> Result<(), Error> {
    for (index, owner) in model.statements.iter().enumerate() {
        let visible = model.temporary_source(input, index, old)?;
        if visible.as_ref().is_none_or(|table| table.scope != producer) {
            continue;
        }
        if let Some(clause) = owner.clauses.iter().find(|c| c.kind == ClauseKind::Destroy) {
            if clause.items[0].expression(input).to_lowercase() == old.to_lowercase() {
                edits.extend(editing::replace(input, &clause.items[0], new)?);
            }
            continue;
        }
        let sources = owner.source_bindings(input);
        for source in sources
            .iter()
            .filter(|s| !s.derived && s.table.to_lowercase() == old.to_lowercase())
        {
            let item = &owner.clauses[source.clause].items[source.item];
            let text = &input[item.range.clone()];
            let token = lex::tokens(text)?
                .into_iter()
                .find(|t| t.kind != Kind::Comment)
                .ok_or(Error::InvalidExpression)?;
            edits.push(Replacement {
                range: item.range.start + token.range.start..item.range.start + token.range.end,
                text: new.to_owned(),
            });
            if implicit_source(text)? {
                if sources
                    .iter()
                    .filter(|s| s.alias.to_lowercase() == source.alias.to_lowercase())
                    .count()
                    != 1
                {
                    return Err(Error::Unsupported);
                }
                let mut protected = visual::source_prefixes(input, owner)?;
                protected.extend(
                    model
                        .scopes
                        .iter()
                        .filter(|s| s.parent == Some(index))
                        .map(|s| s.range.clone()),
                );
                edits.extend(
                    visual::path_references(input, &owner.range, old, new, true)?
                        .into_iter()
                        .filter(|edit| {
                            !protected
                                .iter()
                                .any(|range| range.contains(&edit.range.start))
                        }),
                );
            }
        }
    }
    edits.sort_by_key(|edit| edit.range.start);
    Ok(())
}

/// An explicit alias retains its own namespace even when it happens to equal the old table name.
fn implicit_source(text: &str) -> Result<bool, Error> {
    Ok(lex::tokens(text)?
        .iter()
        .filter(|token| token.kind != Kind::Comment)
        .take_while(|token| !(token.depth == 0 && token.keyword(text, &["ON", "ПО"])))
        .count()
        == 1)
}

/// Removing one whole root operator includes exactly one package separator and retains explicitly kept comments.
fn remove(
    input: &str,
    model: &Model,
    statement: usize,
    comments: Option<CommentDecision>,
) -> Result<Vec<Replacement>, Error> {
    let selected = root(model, statement)?;
    let roots: Vec<_> = model.scopes.iter().filter(|s| s.parent.is_none()).collect();
    if roots.len() < 2 {
        return Err(Error::RequiredItem);
    }
    let at = roots
        .iter()
        .position(|scope| scope.range == selected.range)
        .ok_or(Error::InvalidSelection)?;
    let range = roots.get(at + 1).map_or_else(
        || roots[at - 1].range.end..selected.range.end,
        |next| selected.range.start..next.range.start,
    );
    let retained = retain_comments(&input[range.clone()], comments)?;
    Ok(vec![Replacement {
        range,
        text: retained,
    }])
}

/// Rotate complete operator bodies; separators remain at their original byte boundaries.
fn move_statement(
    input: &str,
    model: &Model,
    statement: usize,
    to: usize,
) -> Result<Vec<Replacement>, Error> {
    let selected = root(model, statement)?;
    let roots: Vec<_> = model.scopes.iter().filter(|s| s.parent.is_none()).collect();
    if to >= roots.len() {
        return Err(Error::InvalidSelection);
    }
    let from = roots
        .iter()
        .position(|s| s.range == selected.range)
        .ok_or(Error::InvalidSelection)?;
    if from == to {
        return Ok(Vec::new());
    }
    let mut bodies: Vec<_> = roots.iter().map(|s| &input[s.range.clone()]).collect();
    let body = bodies.remove(from);
    bodies.insert(to, body);
    Ok((from.min(to)..=from.max(to))
        .map(|i| Replacement {
            range: roots[i].range.clone(),
            text: bodies[i].to_owned(),
        })
        .collect())
}

/// Opaque or nested operators cannot acquire package mutation authority through a flat ordinal.
fn root(model: &Model, statement: usize) -> Result<&super::super::QueryScope, Error> {
    let owner = model
        .statements
        .get(statement)
        .ok_or(Error::InvalidSelection)?;
    let scope = &model.scopes[owner.scope];
    if scope.parent.is_some() || owner.unavailable.is_some() {
        return Err(Error::Unsupported);
    }
    Ok(scope)
}

/// Comments can be retained at the next boundary or explicitly discarded, never silently removed.
fn retain_comments(text: &str, decision: Option<CommentDecision>) -> Result<String, Error> {
    let tokens = lex::tokens(text)?;
    let comments: Vec<_> = tokens.iter().filter(|t| t.kind == Kind::Comment).collect();
    if !comments.is_empty() && decision.is_none() {
        return Err(Error::CommentsRequireDecision);
    }
    let mut output = String::new();
    if matches!(decision, Some(CommentDecision::Keep)) {
        for comment in comments {
            output.push_str(&text[comment.range.clone()]);
            if !output.ends_with('\n') {
                output.push_str(editing::newline(text));
            }
        }
    }
    Ok(output)
}

/// Package navigation follows root ordinals returned by the core after a structural operation.
pub(in super::super) fn focus(before: &Model, after: &Model, action: &Action) -> Option<usize> {
    let owner = before.statements.get(action.statement())?;
    let roots: Vec<_> = before
        .scopes
        .iter()
        .filter(|s| s.parent.is_none())
        .collect();
    let ordinal = roots
        .iter()
        .position(|s| s.branches.contains(&action.statement()))?;
    let at = match action {
        Action::AddStatement { .. } => ordinal + 1,
        Action::RemoveStatement { .. } => ordinal.min(roots.len().saturating_sub(2)),
        Action::MoveStatement { to, .. } => *to,
        _ => return None,
    };
    after
        .scopes
        .iter()
        .filter(|s| s.parent.is_none())
        .nth(at)
        .and_then(|s| s.branches.get(owner.branch.min(s.branches.len() - 1)))
        .copied()
}
