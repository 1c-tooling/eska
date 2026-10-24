//! Proven output aliases propagate through derived sources; shadows, strings and metadata paths stay exact.

use super::super::{
    Action, ClauseKind, Error, Expression, Model, Selection,
    lex::{self, Kind},
    visual,
};
use crate::project::metadata_edit::{EditPlan, Replacement};
use std::collections::BTreeSet;

/// Rename only an explicitly edited projection; adding/removing/moving shared columns is handled separately.
pub(in super::super) fn augment(
    input: &str,
    before: &Model,
    action: &Action,
    plan: EditPlan,
) -> Result<EditPlan, Error> {
    let selection = match action {
        Action::Replace { selection, .. }
        | Action::Compose {
            selection: Some(selection),
            expression: Expression::Field { .. },
            ..
        } => *selection,
        _ => return Ok(plan),
    };
    let owner = &before.statements[selection.statement];
    let clause = &owner.clauses[selection.clause];
    if clause.kind != ClauseKind::Fields || owner.branch != 0 || plan.is_empty() {
        return Ok(plan);
    }
    let Some(old) = clause.items[selection.item].output_name(input, clause) else {
        return Ok(plan);
    };
    let after = Model::parse(plan.output())?;
    let next = &after.statements[selection.statement].clauses[selection.clause];
    let Some(new) = next.items[selection.item].output_name(plan.output(), next) else {
        return if before.scopes[owner.scope].parent.is_some() {
            Err(Error::InvalidExpression)
        } else {
            Ok(plan)
        };
    };
    if old.to_lowercase() == new.to_lowercase() {
        return Ok(plan);
    }
    let mut edits = plan.replacements().to_vec();
    let mut pending = vec![(owner.scope, old, new)];
    let mut visited = BTreeSet::new();
    while let Some((scope, old, new)) = pending.pop() {
        if !visited.insert((scope, old.clone(), new.clone())) {
            continue;
        }
        edits.extend(tail_references(input, &before.scopes[scope], &old, &new)?);
        if let Some(parent) = propagate(input, before, scope, &old, &new, &mut edits)? {
            pending.push(parent);
        }
    }
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    edits.dedup_by(|left, right| left.range == right.range && left.text == right.text);
    EditPlan::from_replacements(input, edits).map_err(|_| Error::InvalidExpression)
}

/// A derived source's output references belong to its parent branch, outside every child SELECT body.
fn propagate(
    input: &str,
    model: &Model,
    scope: usize,
    old: &str,
    new: &str,
    edits: &mut Vec<Replacement>,
) -> Result<Option<(usize, String, String)>, Error> {
    let Some(parent) = model.scopes[scope].parent else {
        return Ok(None);
    };
    let owner = &model.statements[parent];
    let binding = owner.source_bindings(input).into_iter().find(|binding| {
        binding.derived
            && model.source_scope(Selection {
                statement: parent,
                clause: binding.clause,
                item: binding.item,
            }) == Some(scope)
    });
    let Some(binding) = binding else {
        return Ok(None);
    };
    if binding.alias.is_empty() || owner.unavailable.is_some() {
        return Err(Error::Unsupported);
    }
    let mut protected = visual::source_prefixes(input, owner)?;
    protected.extend(
        model
            .scopes
            .iter()
            .filter(|value| value.parent == Some(parent))
            .map(|value| value.range.clone()),
    );
    let references = visual::path_references(
        input,
        &owner.range,
        &format!("{}.{old}", binding.alias),
        &format!("{}.{new}", binding.alias),
        false,
    )?;
    let references: Vec<_> = references
        .into_iter()
        .filter(|edit| {
            !protected
                .iter()
                .any(|range| range.contains(&edit.range.start))
        })
        .collect();
    let implicit = owner.branch == 0 && owner.clauses.iter().filter(|clause| clause.kind == ClauseKind::Fields).any(|clause| clause.items.iter().any(|item| {
        matches!(item.visual(input, clause), Some(Expression::Field { alias, .. }) if alias.is_empty())
            && item.output_name(input, clause).is_some_and(|name| name.to_lowercase() == old.to_lowercase())
            && references.iter().any(|edit| item.range.contains(&edit.range.start))
    }));
    edits.extend(references);
    Ok(implicit.then(|| (owner.scope, old.to_owned(), new.to_owned())))
}

/// Global result tails use the first branch's output names; parameters and qualified source fields are not aliases.
fn tail_references(
    input: &str,
    scope: &super::super::QueryScope,
    old: &str,
    new: &str,
) -> Result<Vec<Replacement>, Error> {
    let tail = &input[scope.tail..scope.range.end];
    let tokens: Vec<_> = lex::tokens(tail)?
        .into_iter()
        .filter(|token| token.kind != Kind::Comment)
        .collect();
    Ok(tokens
        .iter()
        .enumerate()
        .filter(|(index, token)| {
            token.kind == Kind::Word
                && tail[token.range.clone()].to_lowercase() == old.to_lowercase()
                && index.checked_sub(1).is_none_or(|previous| {
                    !matches!(tokens[previous].kind, Kind::Symbol('.' | '&'))
                })
                && tokens
                    .get(index + 1)
                    .is_none_or(|next| !matches!(next.kind, Kind::Symbol('.' | '(')))
        })
        .map(|(_, token)| Replacement {
            range: scope.tail + token.range.start..scope.tail + token.range.end,
            text: new.to_owned(),
        })
        .collect())
}
