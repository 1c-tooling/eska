//! Horizontal tables are planned across adjacent syntax units, without inspecting literal values.

use super::{
    groups, layout,
    syntax::{Unit, UnitKind, group_end},
};
use crate::lex::{Kind, Token};
use eska_project_core::formatting::{Options, Wrapping};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

/// Source token offsets keep nested calls distinct from the outer call's aligned separators.
pub type Columns = BTreeMap<usize, usize>;

/// Only adjacent units at the same block level can participate in one horizontal table.
pub fn plan(
    units: &[Unit],
    tokens: &[Token<'_>],
    options: &Options,
    gaps: &[usize],
    multiline: &[bool],
) -> Vec<Columns> {
    let mut output = vec![Columns::new(); units.len()];
    if options.align_assignments {
        assignments(units, tokens, gaps, multiline, options, &mut output);
    }
    if options.align_call_arguments
        && options.wrap_arguments != Wrapping::OnePerLine
        && options.wrap_binary != Wrapping::OnePerLine
    {
        calls(units, tokens, options, gaps, multiline, &mut output);
    }
    output
}

struct Assignment {
    unit: usize,
    offset: usize,
    column: usize,
    target: Range<usize>,
    end_column: usize,
}

/// A blank line, comment, directive or block transition ends an assignment table.
fn assignments(
    units: &[Unit],
    tokens: &[Token<'_>],
    gaps: &[usize],
    multiline: &[bool],
    options: &Options,
    output: &mut [Columns],
) {
    let mut group = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        let part = &tokens[unit.tokens.clone()];
        let assign =
            unit.kind == UnitKind::Statement && part[0].kind == Kind::Word && part[0].key.is_none();
        let equals = (assign && !multiline[index])
            .then(|| layout::assignment(part))
            .flatten();
        if equals.is_none() || !adjacent(index, units, gaps) {
            finish_assignments(&group, tokens, options, output);
            group.clear();
        }
        if let Some(at) = equals {
            group.push(Assignment {
                unit: index,
                offset: part[at].range.start,
                column: unit.level * usize::from(options.indent_width)
                    + layout::width(&layout::flat(&part[..at], options), 0, options.tab_width)
                    + usize::from(options.space_around_operators),
                target: unit.tokens.start..unit.tokens.start + at,
                end_column: layout::width(
                    &layout::flat(part, options),
                    unit.level * usize::from(options.indent_width),
                    options.tab_width,
                ),
            });
        }
    }
    finish_assignments(&group, tokens, options, output);
}

/// An assignment that provides a later field/index receiver is isolated from horizontal padding.
fn finish_assignments(
    group: &[Assignment],
    tokens: &[Token<'_>],
    options: &Options,
    output: &mut [Columns],
) {
    if !options.separate_object_initialization {
        assignment_columns(group, options, output);
        return;
    }
    let mut receivers = BTreeSet::new();
    let mut initializers = vec![false; group.len()];
    let targets: Vec<_> = group
        .iter()
        .map(|assignment| groups::path_key(&tokens[assignment.target.clone()]))
        .collect();
    for (index, target) in targets.iter().enumerate().rev() {
        initializers[index] = receivers.contains(target.key.as_str());
        receivers.extend(target.parents());
    }
    let mut start = 0;
    for (end, initializer) in initializers.into_iter().enumerate() {
        if initializer {
            assignment_columns(&group[start..end], options, output);
            start = end + 1;
        }
    }
    assignment_columns(&group[start..], options, output);
}

/// A single assignment retains ordinary spacing, while a table uses its widest left-hand side.
fn assignment_columns(group: &[Assignment], options: &Options, output: &mut [Columns]) {
    if group.len() > 1
        && let Some(maximum) = group.iter().map(|assignment| assignment.column).max()
    {
        if options.column_limit != 0
            && group.iter().any(|assignment| {
                assignment.end_column + maximum - assignment.column
                    > usize::from(options.column_limit)
            })
        {
            return;
        }
        for assignment in group {
            output[assignment.unit].insert(assignment.offset, maximum);
        }
    }
}

struct Call {
    unit: usize,
    callee: String,
    start: usize,
    arguments: Vec<(usize, String)>,
    tail: String,
}

/// Repeated calls form a table only when their callee, arity and indentation agree.
fn calls(
    units: &[Unit],
    tokens: &[Token<'_>],
    options: &Options,
    gaps: &[usize],
    multiline: &[bool],
    output: &mut [Columns],
) {
    let mut group: Vec<Call> = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        let candidate = (!multiline[index])
            .then(|| call(index, unit, tokens, options))
            .flatten();
        let compatible = candidate.as_ref().is_some_and(|current| {
            group.last().is_none_or(|previous| {
                current.callee == previous.callee
                    && current.start == previous.start
                    && current.arguments.len() == previous.arguments.len()
            })
        });
        if !compatible || !adjacent(index, units, gaps) {
            finish_calls(&group, options, output);
            group.clear();
        }
        if let Some(candidate) = candidate {
            group.push(candidate);
        }
    }
    finish_calls(&group, options, output);
}

/// Only calls that render on one line enter a table, including source lines collapsed by policy.
fn call(index: usize, unit: &Unit, tokens: &[Token<'_>], options: &Options) -> Option<Call> {
    let part = &tokens[unit.tokens.clone()];
    if unit.kind != UnitKind::Statement
        || !part.last()?.is(";")
        || part.iter().any(|token| token.kind == Kind::Comment)
    {
        return None;
    }
    let indent = unit.level * usize::from(options.indent_width);
    if options.column_limit != 0
        && layout::width(&layout::flat(part, options), indent, options.tab_width)
            > usize::from(options.column_limit)
    {
        return None;
    }
    let open = part.iter().position(|token| token.is("("))?;
    if open == 0
        || open.is_multiple_of(2)
        || !part[..open].iter().enumerate().all(|(at, token)| {
            if at.is_multiple_of(2) {
                token.kind == Kind::Word && token.key.is_none()
            } else {
                token.is(".")
            }
        })
        || group_end(part, open).ok()? != part.len() - 1
    {
        return None;
    }
    let mut arguments = Vec::new();
    let mut first = open + 1;
    let mut at = first;
    while at < part.len() - 2 {
        if part[at].is("(") || part[at].is("[") {
            at = group_end(part, at).ok()?;
        } else {
            if part[at].is(",") {
                arguments.push((
                    part[at].range.start,
                    layout::flat(&part[first..at], options),
                ));
                first = at + 1;
            }
            at += 1;
        }
    }
    if arguments.is_empty() {
        return None;
    }
    let callee = layout::flat(&part[..=open], options);
    Some(Call {
        unit: index,
        start: layout::width(&callee, indent, options.tab_width)
            + usize::from(options.space_inside_parentheses),
        callee,
        arguments,
        tail: layout::flat(&part[first..], options),
    })
}

/// Every separator gets one column; nested commas and punctuation inside literals are opaque.
fn finish_calls(group: &[Call], options: &Options, output: &mut [Columns]) {
    let Some(first) = group.first().filter(|_| group.len() > 1) else {
        return;
    };
    let mut start = first.start;
    let mut columns = Vec::with_capacity(first.arguments.len());
    for at in 0..first.arguments.len() {
        let column = group
            .iter()
            .map(|call| layout::width(&call.arguments[at].1, start, options.tab_width))
            .max()
            .unwrap_or(start);
        // A table must not manufacture unbounded padding for otherwise ordinary source text.
        if column > 8192 {
            return;
        }
        columns.push(column);
        start = column + 1 + usize::from(options.space_after_comma);
    }
    if options.column_limit != 0
        && group.iter().any(|call| {
            layout::width(&call.tail, start, options.tab_width) > usize::from(options.column_limit)
        })
    {
        return;
    }
    for call in group {
        for ((offset, _), column) in call.arguments.iter().zip(&columns) {
            output[call.unit].insert(*offset, *column);
        }
    }
}

/// Blank lines retained by the spacing policy separate horizontal tables; removed lines do not.
const fn adjacent(index: usize, units: &[Unit], gaps: &[usize]) -> bool {
    index > 0 && units[index].level == units[index - 1].level && gaps[index] <= 1
}
