//! Vertical spacing follows parsed statement boundaries, never physical line or semicolon counts.

use super::syntax::{Unit, UnitKind};
use crate::{lex::Token, spelling::Key};
use eska_project_core::formatting::Options;

#[derive(Clone, Copy, Default)]
struct Gap {
    minimum: u16,
    maximum: Option<u16>,
}

impl Gap {
    /// Explicit structural minima override both the ordinary cap and compact single-statement bodies.
    fn lines(self, old: usize, options: &Options, first: bool) -> usize {
        let separator = usize::from(!first);
        let maximum = self.maximum.unwrap_or(options.max_empty_lines);
        old.min(usize::from(maximum) + separator).max(if first {
            0
        } else {
            usize::from(self.minimum) + 1
        })
    }

    /// Adjacent structural requests share one gap instead of accumulating empty lines.
    fn require(&mut self, minimum: u16) {
        self.minimum = self.minimum.max(minimum);
    }
}

struct Branch {
    header: usize,
    statements: u8,
    opaque: bool,
}

/// Resolve gaps once so alignment and rendering agree on the final boundaries between units.
pub(super) fn plan(
    units: &[Unit],
    tokens: &[Token<'_>],
    input: &str,
    options: &Options,
) -> Vec<usize> {
    let mut gaps = vec![Gap::default(); units.len()];
    let mut branches: Vec<Option<Branch>> = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        let previous = index.checked_sub(1).map(|at| &units[at]);
        gaps[index].require(
            if previous.is_some_and(|unit| unit.kind == UnitKind::MethodEnd) {
                options.blank_lines_between_methods
            } else if unit.kind == UnitKind::MethodEnd
                || previous.is_some_and(|unit| unit.kind == UnitKind::Method)
            {
                options.blank_lines_inside_methods
            } else {
                0
            },
        );
        match unit.kind {
            UnitKind::Header => {
                branches.resize_with(unit.level + 1, || None);
                if opening(tokens[unit.tokens.start].key) {
                    count_statement(&mut branches, unit.level);
                } else {
                    finish_branch(branches[unit.level].take(), index, &mut gaps, options);
                }
                branches[unit.level] = Some(Branch {
                    header: index,
                    statements: 0,
                    opaque: false,
                });
            }
            UnitKind::End => {
                if let Some(branch) = branches.get_mut(unit.level) {
                    finish_branch(branch.take(), index, &mut gaps, options);
                }
            }
            UnitKind::Statement if !tokens[unit.tokens.start].is(";") => {
                count_statement(&mut branches, unit.level);
            }
            UnitKind::Directive | UnitKind::Deleted => {
                // Compiler alternatives can open the same runtime block in different source branches.
                for branch in branches.iter_mut().flatten() {
                    branch.opaque = true;
                }
            }
            _ => {}
        }
    }
    separate_statements(units, tokens, input, options, &mut gaps);
    gaps.into_iter()
        .enumerate()
        .map(|(index, gap)| {
            let start = index.checked_sub(1).map_or(0, |previous| {
                tokens[units[previous].tokens.end - 1].range.end
            });
            let end = tokens[units[index].tokens.start].range.start;
            let old = input[start..end]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            gap.lines(old, options, index == 0)
        })
        .collect()
}

/// A nested control structure counts once in its parent, independently of its descendants.
fn count_statement(branches: &mut [Option<Branch>], level: usize) {
    if let Some(branch) = level
        .checked_sub(1)
        .and_then(|at| branches.get_mut(at))
        .and_then(Option::as_mut)
    {
        branch.statements = branch.statements.saturating_add(1).min(2);
    }
}

/// Each IF/ELSIF/ELSE and TRY/EXCEPT body has its own direct-operation count.
fn finish_branch(branch: Option<Branch>, end: usize, gaps: &mut [Gap], options: &Options) {
    let Some(branch) = branch.filter(|branch| !branch.opaque) else {
        return;
    };
    if options.blank_lines_inside_multistatement_blocks == 0 {
        return;
    }
    for index in [branch.header + 1, end] {
        if branch.statements > 1 {
            gaps[index].require(options.blank_lines_inside_multistatement_blocks);
        } else {
            gaps[index].maximum = Some(0);
        }
    }
}

/// Outer gaps separate sibling operations; a sole nested block stays against its parent's boundaries.
fn separate_statements(
    units: &[Unit],
    tokens: &[Token<'_>],
    input: &str,
    options: &Options,
    gaps: &mut [Gap],
) {
    for (index, unit) in units.iter().enumerate() {
        let key = tokens[unit.tokens.start].key;
        if unit.kind == UnitKind::Header && opening(key) {
            let start = leading_comments(units, tokens, input, index);
            if start.checked_sub(1).is_some_and(|previous| {
                units[previous].level == unit.level
                    && matches!(units[previous].kind, UnitKind::Statement | UnitKind::End)
            }) {
                gaps[start].require(options.blank_lines_around_blocks);
            }
        }
        if unit.kind == UnitKind::End {
            let next = units[index + 1..]
                .iter()
                .find(|unit| unit.kind != UnitKind::Comment);
            if next.is_some_and(|next| {
                next.level == unit.level
                    && matches!(next.kind, UnitKind::Statement | UnitKind::Header)
            }) {
                gaps[index + 1].require(options.blank_lines_around_blocks);
            }
        }
        if unit.kind == UnitKind::Statement && key == Some(Key::Return) {
            gaps[leading_comments(units, tokens, input, index)]
                .require(options.blank_lines_before_return);
        }
    }
}

/// Explanatory own-line comments stay attached to the following block or return statement.
fn leading_comments(units: &[Unit], tokens: &[Token<'_>], input: &str, mut index: usize) -> usize {
    while index > 0
        && units[index - 1].kind == UnitKind::Comment
        && units[index - 1].level == units[index].level
    {
        let start = tokens[units[index - 1].tokens.end - 1].range.end;
        let end = tokens[units[index].tokens.start].range.start;
        if input[start..end]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            > 1
        {
            break;
        }
        index -= 1;
    }
    index
}

/// Branch separators share the header kind but do not introduce another operation in their parent.
const fn opening(key: Option<Key>) -> bool {
    matches!(key, Some(Key::If | Key::For | Key::While | Key::Try))
}
