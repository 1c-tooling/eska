//! Layout events address original tokens; nested query boundaries take precedence over call wrapping.

use super::words::{self, Role};
use crate::lex::Token;
use eska_project_core::{
    formatting::{Error, Options, Wrapping},
    project::query::constructor::{Clause, ClauseKind, Model},
};

#[derive(Clone, Copy, Default)]
pub struct Event {
    pub lines: usize,
    pub indent: usize,
    pub alias: usize,
}

/// Clause and item ranges are produced by core, while scalar calls remain lossless token groups.
pub fn events(
    input: &str,
    tokens: &[Token<'_>],
    roles: &[Option<Role>],
    model: &Model,
    options: &Options,
) -> Result<Vec<Event>, Error> {
    let mut events = vec![Event::default(); tokens.len()];
    let depths = groups(tokens, roles)?;
    let step = usize::from(options.indent_width);
    for statement in &model.statements {
        let mut level = 0;
        let mut ancestor = model.scopes[statement.scope].parent;
        while let Some(index) = ancestor {
            level += 2;
            ancestor = model.scopes[model.statements[index].scope].parent;
        }
        let base = level * step;
        for clause in &statement.clauses {
            let start = at(tokens, clause.range.start);
            let body = at(tokens, clause.body.start);
            let indent = base + usize::from(clause.kind == ClauseKind::Joins) * step;
            if let Some(event) = events.get_mut(start) {
                event.lines =
                    usize::from(options.clause_newlines || clause.kind == ClauseKind::Fields);
                event.indent = indent;
            }
            let wrapping = wrapping(clause.kind, options);
            if options.clause_newlines
                && !matches!(
                    clause.kind,
                    ClauseKind::Into | ClauseKind::Destroy | ClauseKind::Joins
                )
                && let Some(event) = events.get_mut(body)
            {
                event.lines = 1;
                event.indent = indent + step;
            }
            item_events(input, tokens, roles, clause, options, &mut events, indent);
            let end = at(tokens, clause.range.end);
            let depth = depths.get(start).copied().unwrap_or(0);
            for index in body..end {
                if depths[index] == depth
                    && words::is(roles[index], &["ON", "ПО"])
                    && clause.kind == ClauseKind::Joins
                {
                    events[index] = Event {
                        lines: 1,
                        indent: indent + step,
                        alias: 0,
                    };
                }
                if depths[index] == depth
                    && words::is(roles[index], &["AND", "И", "OR", "ИЛИ"])
                    && wrapping == Wrapping::OnePerLine
                    && matches!(clause.kind, ClauseKind::Conditions | ClauseKind::Having)
                {
                    events[index] = Event {
                        lines: 1,
                        indent: indent + step,
                        alias: 0,
                    };
                }
            }
            if options.align_aliases
                && matches!(clause.kind, ClauseKind::Fields | ClauseKind::Totals)
            {
                align(tokens, roles, &depths, clause, &mut events, options);
            }
        }
    }
    scope_events(tokens, model, options, &mut events);
    calls(input, tokens, &depths, &mut events, options);
    Ok(events)
}

/// Each list owns a separate wrapping knob; scalar destination names never become lists.
const fn wrapping(kind: ClauseKind, options: &Options) -> Wrapping {
    match kind {
        ClauseKind::Fields | ClauseKind::Totals => options.select_items,
        ClauseKind::Conditions | ClauseKind::Having => options.condition_items,
        ClauseKind::Group | ClauseKind::TotalGroups | ClauseKind::Index => options.group_items,
        ClauseKind::Order => options.order_items,
        ClauseKind::Sources if options.clause_newlines => Wrapping::OnePerLine,
        _ => Wrapping::Fit,
    }
}

/// Fitting measures normalized tokens, so a second pass cannot depend on obsolete source padding.
fn item_events(
    input: &str,
    tokens: &[Token<'_>],
    roles: &[Option<Role>],
    clause: &Clause,
    options: &Options,
    events: &mut [Event],
    indent: usize,
) {
    if matches!(
        clause.kind,
        ClauseKind::Into | ClauseKind::Destroy | ClauseKind::Joins
    ) {
        return;
    }
    let continuation = indent + usize::from(options.indent_width);
    let body = at(tokens, clause.body.start);
    let mut column = if options.clause_newlines {
        continuation
    } else {
        indent
            + super::layout::flat_width(
                tokens,
                roles,
                at(tokens, clause.range.start)..body,
                options,
            )
            + 1
    };
    let mut previous = clause.body.start;
    for item in &clause.items {
        let first = at(tokens, item.range.start);
        if first >= tokens.len() {
            continue;
        }
        let length =
            super::layout::flat_width(tokens, roles, first..at(tokens, item.range.end), options);
        let break_item = match wrapping(clause.kind, options) {
            Wrapping::OnePerLine => true,
            Wrapping::Preserve => input[previous..tokens[first].range.start].contains('\n'),
            Wrapping::Fit => {
                options.column_limit > 0 && column + length > usize::from(options.column_limit)
            }
        };
        let after_boolean = matches!(clause.kind, ClauseKind::Conditions | ClauseKind::Having)
            && first > 0
            && words::is(roles[first - 1], &["AND", "И", "OR", "ИЛИ"]);
        if break_item {
            let index = if after_boolean { first - 1 } else { first };
            events[index] = Event {
                lines: 1,
                indent: continuation,
                alias: 0,
            };
            column = continuation;
        }
        column += length + 2;
        previous = item.range.end;
    }
}

/// Result tails and nested closing delimiters belong to their query scope, never an adjacent call.
fn scope_events(tokens: &[Token<'_>], model: &Model, options: &Options, events: &mut [Event]) {
    let step = usize::from(options.indent_width);
    for scope in &model.scopes {
        let first = scope
            .branches
            .first()
            .and_then(|index| model.statements.get(*index))
            .map(|statement| at(tokens, statement.clauses[0].range.start));
        if let Some(first) = first {
            let base = events[first].indent;
            for union in &scope.unions {
                let separator = at(tokens, union.keyword.start);
                events[separator] = Event {
                    lines: usize::from(options.blank_lines_between_queries) + 1,
                    indent: base,
                    alias: 0,
                };
                let next = at(tokens, union.range.end);
                if let Some(event) = events.get_mut(next) {
                    event.lines = usize::from(options.blank_lines_between_queries) + 1;
                }
            }
            if scope.parent.is_some() {
                let close = at(tokens, scope.range.end);
                if let Some(event) = events.get_mut(close) {
                    event.lines = 1;
                    event.indent = base.saturating_sub(step);
                }
            }
        }
    }
}

/// Binary search never scans preceding clauses again when a large package exposes many items.
fn at(tokens: &[Token<'_>], offset: usize) -> usize {
    tokens.partition_point(|token| token.range.start < offset)
}

/// Parentheses, DCS braces and CASE blocks are bounded before any layout allocation.
fn groups(tokens: &[Token<'_>], roles: &[Option<Role>]) -> Result<Vec<usize>, Error> {
    let mut stack = Vec::new();
    let mut cases = Vec::new();
    let mut depths = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        if matches!(token.text, ")" | "]" | "}") {
            let expected = match token.text {
                ")" => "(",
                "]" => "[",
                _ => "{",
            };
            if stack.pop() != Some(expected) {
                return Err(Error::new("invalid_syntax", "delimiter").at(token.range.start));
            }
        }
        depths.push(stack.len());
        if matches!(token.text, "(" | "[" | "{") {
            stack.push(token.text);
        }
        if let Some(role) = roles[index] {
            match role.word {
                "CASE" | "ВЫБОР" => cases.push(stack.len()),
                "END" | "КОНЕЦ" if cases.pop() != Some(stack.len()) => {
                    return Err(Error::new("invalid_syntax", "case").at(token.range.start));
                }
                _ => {}
            }
        }
        if stack.len() > 128 || cases.len() > 128 {
            return Err(Error::new("syntax_too_large", "nesting"));
        }
    }
    if !stack.is_empty() || !cases.is_empty() {
        return Err(Error::new("invalid_syntax", "delimiter"));
    }
    Ok(depths)
}

/// Explicit aliases align only at their owning list depth; AS inside CAST is never an alias boundary.
fn align(
    tokens: &[Token<'_>],
    roles: &[Option<Role>],
    depths: &[usize],
    clause: &eska_project_core::project::query::constructor::Clause,
    events: &mut [Event],
    options: &Options,
) {
    let mut aliases = Vec::new();
    for item in &clause.items {
        let start = at(tokens, item.range.start);
        let end = at(tokens, item.range.end);
        if let Some(index) = (start..end).find(|index| {
            depths[*index] == depths[start] && words::is(roles[*index], &["AS", "КАК"])
        }) {
            let length = super::layout::flat_width(tokens, roles, start..index, options);
            aliases.push((index, length));
        }
    }
    let maximum = aliases.iter().map(|(_, length)| *length).max().unwrap_or(0);
    for (index, length) in aliases {
        events[index].alias = maximum - length;
    }
}

/// Only scalar calls wrap here; clauses inside a nested SELECT already have stronger events.
fn calls(
    input: &str,
    tokens: &[Token<'_>],
    depths: &[usize],
    events: &mut [Event],
    options: &Options,
) {
    let mut stack = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.is("(") {
            stack.push(index);
        }
        if !token.is(")") {
            continue;
        }
        let Some(open) = stack.pop() else {
            continue;
        };
        if index == open + 1 || events[open + 1..index].iter().any(|event| event.lines > 0) {
            continue;
        }
        let raw = &input[tokens[open].range.end..token.range.start];
        let wrap = match options.wrap_arguments {
            Wrapping::OnePerLine => true,
            Wrapping::Fit => {
                options.column_limit > 0
                    && crate::bsl::layout::width(raw, 0, options.tab_width)
                        > usize::from(options.column_limit)
                            .saturating_sub(usize::from(options.continuation_indent_width))
            }
            Wrapping::Preserve => raw.contains('\n'),
        };
        if !wrap {
            continue;
        }
        let base = events[..=open]
            .iter()
            .rev()
            .find(|event| event.lines > 0)
            .map_or(0, |event| event.indent);
        let indent = base + usize::from(options.continuation_indent_width);
        events[open + 1] = Event {
            lines: 1,
            indent,
            alias: 0,
        };
        for comma in open + 1..index {
            if tokens[comma].is(",") && depths[comma] == depths[open] + 1 {
                events[comma + 1] = Event {
                    lines: 1,
                    indent,
                    alias: 0,
                };
            }
        }
        events[index] = Event {
            lines: 1,
            indent: base,
            alias: 0,
        };
    }
}
