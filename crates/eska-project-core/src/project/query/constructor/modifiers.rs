//! SELECT modifiers retain their own ranges instead of becoming field or predicate text.

use super::{
    Action, ClauseKind, Constructor, Error, Statement, editing,
    lex::{self, Kind, Token},
};
use crate::project::metadata_edit::{EditPlan, Replacement};
use std::{collections::BTreeSet, ops::Range};

/// TOP is a bounded unsigned integer in the portable protocol, independent of native number representation.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Modifiers {
    pub distinct: bool,
    pub allowed: bool,
    pub top: Option<u32>,
    pub auto_order: bool,
    pub for_update: Option<Vec<String>>,
}

/// Scope-specific controls are explicit; an adapter cannot infer availability from the query text.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifierView {
    pub value: Modifiers,
    pub editable: bool,
    pub auto_order_available: bool,
    pub for_update_available: bool,
    pub sources: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ControlKind {
    ForUpdate,
    AutoOrder,
}

#[derive(Clone, Debug)]
pub(super) struct Control {
    pub kind: ControlKind,
    pub range: Range<usize>,
}

impl Control {
    /// FOR UPDATE belongs to the definition, AUTOORDER to the result before totals.
    pub const fn rank(&self) -> u8 {
        match self.kind {
            ControlKind::ForUpdate => 7,
            ControlKind::AutoOrder => 9,
        }
    }
}

impl Constructor {
    /// Read the selected branch prefix and the enclosing scope's shared result autoordering.
    #[must_use]
    pub fn modifiers(&self, statement: usize) -> Option<ModifierView> {
        let owner = self.model.statements.get(statement)?;
        let (mut value, _) = prefix(self.text(), owner)?;
        let scope = self.model.scopes.get(owner.scope)?;
        let first = &self.model.statements[scope.branches[0]];
        let result = &self.model.statements[*scope.branches.last()?];
        value.auto_order = result
            .controls
            .iter()
            .any(|control| control.kind == ControlKind::AutoOrder);
        value.for_update = owner
            .controls
            .iter()
            .find(|control| control.kind == ControlKind::ForUpdate)
            .and_then(|control| locking_sources(self.text(), control).ok());
        let temporary = first
            .clauses
            .iter()
            .any(|clause| clause.kind == ClauseKind::Into);
        let bindings = owner.source_bindings(self.text());
        let lockable = bindings.iter().all(|source| {
            !source.derived && source.parameters.is_none() && source.table.matches('.').count() == 1
        });
        let mut seen = BTreeSet::new();
        let sources = bindings
            .into_iter()
            .filter(|source| {
                !source.derived
                    && source.parameters.is_none()
                    && source.table.contains('.')
                    && seen.insert(source.table.to_lowercase())
            })
            .map(|source| source.table)
            .collect();
        Some(ModifierView {
            value,
            editable: owner.unavailable.is_none(),
            auto_order_available: !owner.nested && !temporary,
            for_update_available: !owner.nested
                && !temporary
                && scope.branches.len() == 1
                && lockable,
            sources,
        })
    }
}

/// Prefix parsing is deliberately limited to the already supported literal TOP and known flags.
fn prefix(input: &str, owner: &Statement) -> Option<(Modifiers, Range<usize>)> {
    let text = &input[owner.range.clone()];
    let tokens = lex::tokens(text).ok()?;
    let top: Vec<_> = tokens
        .iter()
        .filter(|token| token.depth == 0 && token.kind != Kind::Comment)
        .collect();
    let first = top
        .first()
        .filter(|token| token.keyword(text, &["SELECT", "ВЫБРАТЬ"]))?;
    let mut value = Modifiers {
        distinct: false,
        allowed: false,
        top: None,
        auto_order: false,
        for_update: None,
    };
    let mut index = 1;
    let mut end = first.range.end;
    while let Some(token) = top.get(index) {
        if token.keyword(text, &["DISTINCT", "РАЗЛИЧНЫЕ"]) {
            if value.distinct {
                return None;
            }
            value.distinct = true;
        } else if token.keyword(text, &["ALLOWED", "РАЗРЕШЕННЫЕ"]) {
            if value.allowed {
                return None;
            }
            value.allowed = true;
        } else if token.keyword(text, &["TOP", "ПЕРВЫЕ"]) {
            if value.top.is_some() {
                return None;
            }
            index += 1;
            let number = top.get(index)?;
            value.top = Some(text[number.range.clone()].parse().ok()?);
        } else {
            break;
        }
        end = top[index].range.end;
        index += 1;
    }
    Some((
        value,
        owner.range.start + first.range.end..owner.range.start + end,
    ))
}

/// Recognize only complete native controls; a dot-qualified field segment is never a keyword.
pub(super) fn controls(input: &str, top: &[&Token]) -> Result<Vec<Control>, Error> {
    let mut controls = Vec::new();
    for (index, token) in top.iter().enumerate() {
        if index > 0 && top[index - 1].kind == Kind::Symbol('.') {
            continue;
        }
        let kind = if token.keyword(input, &["FOR", "ДЛЯ"]) {
            if top
                .get(index + 1)
                .is_none_or(|next| !next.keyword(input, &["UPDATE", "ИЗМЕНЕНИЯ"]))
            {
                return Err(Error::Unsupported);
            }
            ControlKind::ForUpdate
        } else if token.keyword(input, &["AUTOORDER", "АВТОУПОРЯДОЧИВАНИЕ"]) {
            if top
                .get(index + 1)
                .is_some_and(|next| !next.keyword(input, &["TOTALS", "ИТОГИ"]))
            {
                return Err(Error::Unsupported);
            }
            ControlKind::AutoOrder
        } else {
            continue;
        };
        if controls
            .iter()
            .any(|control: &Control| control.kind == kind)
        {
            return Err(Error::Unsupported);
        }
        let end = if kind == ControlKind::AutoOrder {
            token.range.end
        } else {
            let tail = &top[index + 2..];
            let end = tail
                .iter()
                .position(|next| {
                    next.keyword(
                        input,
                        &[
                            "ORDER",
                            "УПОРЯДОЧИТЬ",
                            "AUTOORDER",
                            "АВТОУПОРЯДОЧИВАНИЕ",
                            "TOTALS",
                            "ИТОГИ",
                            "INDEX",
                            "ИНДЕКСИРОВАТЬ",
                        ],
                    )
                })
                .unwrap_or(tail.len());
            tail.get(end.saturating_sub(1))
                .filter(|_| end > 0)
                .map_or(top[index + 1].range.end, |token| token.range.end)
        };
        let control = Control {
            kind,
            range: token.range.start..end,
        };
        if kind == ControlKind::ForUpdate {
            locking_sources(input, &control)?;
        }
        controls.push(control);
    }
    Ok(controls)
}

/// Native FOR UPDATE accepts source table paths, including an empty list meaning all sources.
fn locking_sources(input: &str, control: &Control) -> Result<Vec<String>, Error> {
    let text = &input[control.range.clone()];
    let tokens = lex::tokens(text)?;
    let tokens: Vec<_> = tokens
        .iter()
        .filter(|token| token.kind != Kind::Comment)
        .collect();
    if tokens.len() < 2 {
        return Err(Error::InvalidExpression);
    }
    let mut names = Vec::new();
    let mut name = String::new();
    let mut word = true;
    for token in &tokens[2..] {
        match (word, token.kind) {
            (true, Kind::Word) => {
                name.push_str(&text[token.range.clone()]);
                word = false;
            }
            (false, Kind::Symbol('.')) => {
                name.push('.');
                word = true;
            }
            (false, Kind::Symbol(',')) => {
                names.push(std::mem::take(&mut name));
                word = true;
            }
            _ => return Err(Error::InvalidExpression),
        }
    }
    if !name.is_empty() {
        if word {
            return Err(Error::InvalidExpression);
        }
        names.push(name);
    } else if !names.is_empty() {
        return Err(Error::InvalidExpression);
    }
    Ok(names)
}

/// Every new clause is inserted relative to both clause and modifier boundaries.
pub(super) fn insertion(owner: &Statement, kind: ClauseKind) -> usize {
    owner
        .clauses
        .iter()
        .filter(|clause| clause.kind.rank() > kind.rank())
        .map(|clause| clause.range.start)
        .chain(
            owner
                .controls
                .iter()
                .filter(|control| control.rank() > kind.rank())
                .map(|control| control.range.start),
        )
        .min()
        .unwrap_or_else(|| {
            owner
                .clauses
                .iter()
                .filter_map(|clause| clause.items.last().map(|item| item.range.end))
                .chain(
                    owner
                        .controls
                        .iter()
                        .filter(|control| control.rank() < kind.rank())
                        .map(|control| control.range.end),
                )
                .max()
                .unwrap_or(owner.range.end)
        })
}

/// Validate modifier ordering together with ordinary clause headers before exposing any editable rows.
pub(super) fn ordered(headers: &[(ClauseKind, usize, usize)], controls: &[Control]) -> bool {
    let mut ranks: Vec<_> = headers
        .iter()
        .map(|&(kind, start, _)| (start, kind.rank()))
        .chain(
            controls
                .iter()
                .map(|control| (control.range.start, control.rank())),
        )
        .collect();
    ranks.sort_unstable();
    ranks.windows(2).all(|pair| pair[0].1 <= pair[1].1)
}

/// Unsupported native combinations remain opaque instead of acquiring visual editing authority.
pub(super) fn constrain(input: &str, model: &mut super::Model) {
    let invalid: Vec<_> = model
        .statements
        .iter()
        .enumerate()
        .filter_map(|(index, owner)| {
            let scope = &model.scopes[owner.scope];
            let temporary = model.statements[scope.branches[0]]
                .clauses
                .iter()
                .any(|clause| clause.kind == ClauseKind::Into);
            owner
                .controls
                .iter()
                .any(|control| {
                    owner.nested
                        || temporary
                        || control.kind == ControlKind::ForUpdate
                            && (scope.branches.len() > 1
                                || owner
                                    .source_bindings(input)
                                    .iter()
                                    .any(|source| source.derived || source.parameters.is_some())
                                || locking_sources(input, control).is_ok_and(|sources| {
                                    sources.iter().any(|path| {
                                        !owner.source_bindings(input).iter().any(|source| {
                                            source.table.to_lowercase() == path.to_lowercase()
                                        })
                                    })
                                }))
                })
                .then_some(index)
        })
        .collect();
    for index in invalid {
        model.statements[index].unavailable = Some(super::Unavailable::UnsupportedConstruct);
    }
}

/// Related edits cannot strand an explicit lock target or silently convert controls into an invalid operator.
pub(super) fn guard(value: &Constructor, action: &Action) -> Result<(), Error> {
    let Some(owner) = value.model.statements.get(action.statement()) else {
        return Ok(());
    };
    if owner.controls.is_empty() {
        return Ok(());
    }
    if matches!(
        action,
        Action::SetInto { name: Some(_), .. }
            | Action::Append {
                clause: ClauseKind::Into,
                ..
            }
    ) || matches!(action, Action::AddUnion { .. })
        && owner
            .controls
            .iter()
            .any(|control| control.kind == ControlKind::ForUpdate)
    {
        return Err(Error::Unsupported);
    }
    let Some(control) = owner
        .controls
        .iter()
        .find(|control| control.kind == ControlKind::ForUpdate)
    else {
        return Ok(());
    };
    let paths = locking_sources(value.text(), control)?;
    if paths.is_empty() {
        return Ok(());
    }
    let (Action::Compose {
        selection: Some(selected),
        ..
    }
    | Action::Remove {
        selection: selected,
        ..
    }
    | Action::Replace {
        selection: selected,
        ..
    }) = action
    else {
        return Ok(());
    };
    let Some(binding) = owner
        .source_bindings(value.text())
        .into_iter()
        .find(|source| {
            source.clause == selected.clause
                && source.item == selected.item
                && paths
                    .iter()
                    .any(|path| path.to_lowercase() == source.table.to_lowercase())
        })
    else {
        return Ok(());
    };
    if let Action::Compose {
        expression: super::Expression::Source { table, .. },
        ..
    } = action
        && table.to_lowercase() == binding.table.to_lowercase()
    {
        return Ok(());
    }
    if let Action::Replace { text, .. } = action
        && owner
            .clauses
            .get(selected.clause)
            .and_then(|clause| clause.items.get(selected.item))
            .is_some_and(|item| item.expression(value.text()) == text.trim())
    {
        return Ok(());
    }
    Err(Error::Unsupported)
}

/// Only the requested modifier ranges change; a repeated projection returns an exact no-op.
pub(super) fn plan(value: &Constructor, action: &Action) -> Result<Option<EditPlan>, Error> {
    let Action::SetModifiers {
        statement,
        modifiers,
    } = action
    else {
        return Ok(None);
    };
    let owner = value
        .model
        .statements
        .get(*statement)
        .ok_or(Error::InvalidSelection)?;
    let view = value.modifiers(*statement).ok_or(Error::Unsupported)?;
    if view.value == *modifiers {
        return EditPlan::from_replacements(value.text(), Vec::new())
            .map(Some)
            .map_err(|_| Error::InvalidExpression);
    }
    if !view.editable
        || modifiers.auto_order && !view.auto_order_available
        || modifiers.for_update.is_some() && !view.for_update_available
    {
        return Err(Error::Unsupported);
    }
    if let Some(sources) = &modifiers.for_update {
        let names: BTreeSet<_> = sources.iter().map(|name| name.to_lowercase()).collect();
        if sources.len() > 64
            || names.len() != sources.len()
            || sources.iter().any(|name| {
                !view
                    .sources
                    .iter()
                    .any(|source| source.to_lowercase() == name.to_lowercase())
            })
        {
            return Err(Error::InvalidExpression);
        }
    }
    let input = value.text();
    let mut edits = Vec::new();
    let russian = owner.russian;
    let (_, range) = prefix(input, owner).ok_or(Error::Unsupported)?;
    if view.value.distinct != modifiers.distinct
        || view.value.allowed != modifiers.allowed
        || view.value.top != modifiers.top
    {
        let text = prefix_text(modifiers, russian);
        edits.push(replacement(input, range, text)?);
    }
    if view.value.for_update != modifiers.for_update {
        let text = modifiers.for_update.as_ref().map(|sources| {
            format!(
                "{}{}",
                if russian {
                    "ДЛЯ ИЗМЕНЕНИЯ"
                } else {
                    "FOR UPDATE"
                },
                if sources.is_empty() {
                    String::new()
                } else {
                    format!(" {}", sources.join(", "))
                }
            )
        });
        control_edit(input, owner, ControlKind::ForUpdate, text, &mut edits)?;
    }
    if view.value.auto_order != modifiers.auto_order {
        let result = &value.model.statements[*value.model.scopes[owner.scope]
            .branches
            .last()
            .ok_or(Error::InvalidSelection)?];
        control_edit(
            input,
            result,
            ControlKind::AutoOrder,
            modifiers.auto_order.then(|| {
                if result.russian {
                    "АВТОУПОРЯДОЧИВАНИЕ"
                } else {
                    "AUTOORDER"
                }
                .to_owned()
            }),
            &mut edits,
        )?;
    }
    EditPlan::from_replacements(input, merge_insertions(edits))
        .map(Some)
        .map_err(|_| Error::InvalidExpression)
}

/// Prefix generation is shared by both query languages and never rewrites the selected fields.
fn prefix_text(value: &Modifiers, russian: bool) -> String {
    let mut text = String::new();
    if value.allowed {
        text.push_str(if russian {
            " РАЗРЕШЕННЫЕ"
        } else {
            " ALLOWED"
        });
    }
    if value.distinct {
        text.push_str(if russian {
            " РАЗЛИЧНЫЕ"
        } else {
            " DISTINCT"
        });
    }
    if let Some(top) = value.top {
        text.push_str(if russian { " ПЕРВЫЕ " } else { " TOP " });
        text.push_str(&top.to_string());
    }
    text
}

/// A control can be added or removed without consuming the neighbouring comment/statement separator.
fn control_edit(
    input: &str,
    owner: &Statement,
    kind: ControlKind,
    text: Option<String>,
    edits: &mut Vec<Replacement>,
) -> Result<(), Error> {
    if let Some(control) = owner.controls.iter().find(|control| control.kind == kind) {
        edits.push(replacement(
            input,
            control.range.clone(),
            text.unwrap_or_default(),
        )?);
    } else if let Some(text) = text {
        let rank = if kind == ControlKind::ForUpdate { 7 } else { 9 };
        let at = owner
            .clauses
            .iter()
            .filter(|clause| clause.kind.rank() > rank)
            .map(|clause| clause.range.start)
            .chain(
                owner
                    .controls
                    .iter()
                    .filter(|control| control.rank() > rank)
                    .map(|control| control.range.start),
            )
            .min()
            .unwrap_or(owner.range.end);
        let newline = editing::newline(input);
        edits.push(Replacement {
            range: at..at,
            text: format!("{newline}{text}{newline}"),
        });
    }
    Ok(())
}

/// Rewritten controls retain exact comment bytes with a safe physical newline.
fn replacement(input: &str, range: Range<usize>, mut text: String) -> Result<Replacement, Error> {
    let original = &input[range.clone()];
    let mut comments = String::new();
    for token in lex::tokens(original)?
        .iter()
        .filter(|token| token.kind == Kind::Comment)
    {
        comments.push_str(&original[token.range.clone()]);
        comments.push_str(editing::newline(input));
    }
    text.insert_str(0, &comments);
    Ok(Replacement { range, text })
}

/// FOR UPDATE and AUTOORDER can share an insertion boundary; their native order is the action's order.
fn merge_insertions(edits: Vec<Replacement>) -> Vec<Replacement> {
    let mut output: Vec<Replacement> = Vec::new();
    for edit in edits {
        if edit.range.is_empty()
            && let Some(previous) = output
                .iter_mut()
                .find(|previous| previous.range == edit.range)
        {
            previous.text.push_str(&edit.text);
        } else {
            output.push(edit);
        }
    }
    output
}
