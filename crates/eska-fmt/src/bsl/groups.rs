//! Object groups use assignment destinations and standalone call receivers, never RHS mentions.

use super::{
    layout,
    syntax::{Unit, UnitKind, group_end},
};
use crate::lex::{Kind, Token};
use std::borrow::Cow;

#[derive(Clone, Copy, Default)]
pub(super) struct Boundary {
    pub before: bool,
    pub after: bool,
}

pub(super) struct PathKey {
    pub key: String,
    parents: Vec<usize>,
}

impl PathKey {
    /// Parent offsets borrow one canonical path rather than allocating every increasingly long prefix.
    pub fn parents(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.parents.iter().map(|end| &self.key[..*end])
    }
}

struct Operation {
    path: PathKey,
    assignment: bool,
}

struct Group<'a> {
    owner: &'a str,
    first: usize,
    last: usize,
    level: usize,
    operations: usize,
    initialized: bool,
}

/// Two or more adjacent operations on one receiver form a block, optionally starting at its initializer.
pub(super) fn plan(units: &[Unit], tokens: &[Token<'_>]) -> Vec<Boundary> {
    let operations: Vec<_> = units.iter().map(|unit| operation(unit, tokens)).collect();
    let mut output = vec![Boundary::default(); units.len()];
    let mut active: Option<Group<'_>> = None;
    for (index, unit) in units.iter().enumerate() {
        if transparent_comment(unit, tokens) {
            continue;
        }
        let Some(current) = &operations[index] else {
            finish(active.take(), &mut output);
            continue;
        };
        let initializer = current.assignment
            && units[index + 1..]
                .iter()
                .enumerate()
                .find(|(_, next)| !transparent_comment(next, tokens))
                .filter(|(_, next)| next.level == unit.level)
                .and_then(|(offset, _)| operations[index + 1 + offset].as_ref())
                .is_some_and(|next| next.path.parents().any(|parent| parent == current.path.key));
        if !initializer
            && let Some(group) = &mut active
            && group.level == unit.level
            && if group.initialized {
                current.path.parents().any(|parent| parent == group.owner)
            } else {
                current.path.parents().next_back() == Some(group.owner)
            }
        {
            group.last = index;
            group.operations += 1;
            continue;
        }
        finish(active.take(), &mut output);
        let owner = if initializer {
            Some(current.path.key.as_str())
        } else {
            current.path.parents().next_back()
        };
        active = owner.map(|owner| Group {
            owner,
            first: index,
            last: index,
            level: unit.level,
            operations: 1,
            initialized: initializer,
        });
    }
    finish(active, &mut output);
    output
}

/// A one-off member access is ordinary code; only a real run requests surrounding separators.
fn finish(group: Option<Group<'_>>, output: &mut [Boundary]) {
    if let Some(group) = group.filter(|group| group.operations >= 2) {
        output[group.first].before = true;
        output[group.last].after = true;
    }
}

/// Formatting fences terminate grouping, while explanatory comments remain attached to their statements.
fn transparent_comment(unit: &Unit, tokens: &[Token<'_>]) -> bool {
    unit.kind == UnitKind::Comment
        && !matches!(
            tokens[unit.tokens.start]
                .text
                .strip_prefix("//")
                .map(str::trim),
            Some("eska-fmt off" | "eska-fmt on")
        )
}

/// Assignments belong to their destination; only standalone calls contribute a method receiver.
fn operation(unit: &Unit, tokens: &[Token<'_>]) -> Option<Operation> {
    let part = &tokens[unit.tokens.clone()];
    if unit.kind != UnitKind::Statement || part[0].kind != Kind::Word || part[0].key.is_some() {
        return None;
    }
    if let Some(at) = layout::assignment(part) {
        return Some(Operation {
            path: path_key(&part[..at]),
            assignment: true,
        });
    }
    let mut at = 1;
    while let Some(token) = part.get(at) {
        if token.is("[") {
            at = group_end(part, at).ok()?;
        } else if token.is(".") && part.get(at + 1).is_some_and(|next| next.kind == Kind::Word) {
            at += 2;
        } else if token.is("(") && at >= 3 && part[at - 2].is(".") {
            return Some(Operation {
                path: path_key(&part[..at]),
                assignment: false,
            });
        } else {
            return None;
        }
    }
    None
}

/// Length-framed tokens preserve path boundaries, case-insensitive names and exact string index values.
pub(super) fn path_key(tokens: &[Token<'_>]) -> PathKey {
    let mut key = String::new();
    let mut parents = Vec::new();
    let mut depth = 0_usize;
    for token in tokens.iter().filter(|token| token.kind != Kind::Comment) {
        if depth == 0 && (token.is(".") || token.is("[")) {
            parents.push(key.len());
        }
        let text = if token.kind == Kind::Word {
            Cow::Owned(token.text.to_lowercase())
        } else {
            Cow::Borrowed(token.text)
        };
        key.push_str(&text.len().to_string());
        key.push(':');
        key.push_str(&text);
        match token.text {
            "(" | "[" => depth += 1,
            ")" | "]" => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    PathKey { key, parents }
}
