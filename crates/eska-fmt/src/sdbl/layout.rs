//! A token printer inserts trivia only; comments and literal bodies remain opaque byte sequences.

use super::{
    structure::Event,
    words::{self, Role},
};
use crate::{
    bsl::layout::{indentation, width},
    lex::{Kind, Token},
    output::append,
};
use eska_project_core::formatting::{Error, Options, Wrapping};
use std::ops::Range;

/// Print a complete package unit using the same bounded policy as document and range operations.
pub fn render(
    input: &str,
    tokens: &[Token<'_>],
    roles: &[Option<Role>],
    events: &[Event],
    range: Range<usize>,
    options: &Options,
    eol: &str,
) -> Result<String, Error> {
    let mut output = String::new();
    let mut column = 0;
    let mut indent = 0;
    let mut comment = false;
    for index in range.clone() {
        let token = &tokens[index];
        let previous = index
            .checked_sub(1)
            .filter(|index| range.contains(index))
            .map(|index| &tokens[index]);
        let raw = previous.map_or("", |previous| &input[previous.range.end..token.range.start]);
        let original_line = raw.contains('\n');
        let own_comment = token.kind == Kind::Comment && original_line;
        let mut lines = events[index].lines;
        let mut next_indent = if lines > 0 {
            events[index].indent
        } else {
            indent
        };
        if comment || own_comment {
            lines = lines.max(1);
        }
        if options.wrap_binary == Wrapping::Preserve && original_line {
            lines = lines.max(1);
        }
        let text = words::render(token, roles[index], options.keyword_case);
        let binary = words::is(roles[index], &["AND", "И", "OR", "ИЛИ"]) || operator(token.text);
        if options.wrap_binary == Wrapping::OnePerLine
            && binary
            && previous.is_some_and(|token| !token.is("(") && !token.is(","))
        {
            if options.break_before_binary_operators {
                lines = lines.max(1);
                next_indent = indent;
            }
        } else if options.wrap_binary == Wrapping::Fit
            && binary
            && options.column_limit > 0
            && column + text.chars().count() + 2 > usize::from(options.column_limit)
        {
            lines = lines.max(1);
            next_indent = indent + usize::from(options.continuation_indent_width);
        }
        if options.wrap_binary == Wrapping::OnePerLine
            && !options.break_before_binary_operators
            && index > range.start
            && (operator(tokens[index - 1].text)
                || words::is(roles[index - 1], &["AND", "И", "OR", "ИЛИ"]))
        {
            lines = lines.max(1);
        }
        // A source comment following a comma stays on that comma's line, before the next item break.
        if token.kind == Kind::Comment && !original_line && previous.is_some() {
            lines = 0;
        }
        if lines > 0 && !output.is_empty() {
            let retained = raw
                .chars()
                .filter(|value| *value == '\n')
                .count()
                .min(usize::from(options.max_empty_lines) + 1);
            if !options.trim_trailing_whitespace && original_line {
                append(
                    &mut output,
                    raw.split('\n')
                        .next()
                        .unwrap_or_default()
                        .trim_end_matches('\r'),
                )?;
            }
            append(&mut output, &eol.repeat(lines.max(retained)))?;
            append(&mut output, &indentation(next_indent, options))?;
            column = next_indent;
            indent = next_indent;
        } else if output.is_empty() {
            append(&mut output, &indentation(next_indent, options))?;
            column = next_indent;
            indent = next_indent;
        } else if space(tokens, roles, index, options) {
            append(&mut output, " ")?;
            column += 1;
        }
        if events[index].alias > 0 {
            append(&mut output, &" ".repeat(events[index].alias))?;
            column += events[index].alias;
        }
        append(&mut output, &text)?;
        column = width(&text, column, options.tab_width);
        comment = token.kind == Kind::Comment && token.text.starts_with("//");
    }
    Ok(output)
}

/// Flat expression width uses exactly the printer's spelling and separators, keeping alias alignment stable.
pub fn flat_width(
    tokens: &[Token<'_>],
    roles: &[Option<Role>],
    range: Range<usize>,
    options: &Options,
) -> usize {
    let mut column = 0;
    for index in range.clone() {
        if index > range.start && space(tokens, roles, index, options) {
            column += 1;
        }
        column = width(
            &words::render(&tokens[index], roles[index], options.keyword_case),
            column,
            options.tab_width,
        );
    }
    column
}

/// Lexical separation takes precedence over compact operator and parenthesis preferences.
fn space(tokens: &[Token<'_>], roles: &[Option<Role>], index: usize, options: &Options) -> bool {
    let Some(previous) = index.checked_sub(1).and_then(|index| tokens.get(index)) else {
        return false;
    };
    let current = &tokens[index];
    if current.is(".") || previous.is(".") || current.is(",") || current.is(";") {
        return false;
    }
    if current.is(")")
        || current.is("]")
        || current.is("}")
        || previous.is("(")
        || previous.is("[")
        || previous.is("{")
    {
        return options.space_inside_parentheses && !(previous.is("(") && current.is(")"));
    }
    if previous.is(",") {
        return options.space_after_comma;
    }
    if current.is("(") {
        return options.space_before_call_parenthesis
            || !(previous.kind == Kind::Word && roles[index - 1].is_none_or(|role| role.function)
                || previous.is(")"));
    }
    if operator(current.text) || operator(previous.text) {
        return options.space_around_operators
            || previous.text == "/" && matches!(current.text, "/" | "*");
    }
    true
}

/// Punctuation operators are formatting boundaries, never rewritten or regrouped expressions.
fn operator(text: &str) -> bool {
    matches!(
        text,
        "=" | "<>" | "<" | ">" | "<=" | ">=" | "+" | "-" | "*" | "/" | "%"
    )
}
