//! A bounded token-tree printer chooses line breaks without changing expression order.

use super::syntax::group_end;
use crate::{
    lex::{Kind, Token},
    spelling::{self, Key},
};
use eska_project_core::formatting::{Error, IndentStyle, Options, Wrapping};

const MAX_OUTPUT: usize = 16 * 1024 * 1024;

/// Flat spelling is shared by fit decisions and assignment-column measurement.
pub fn flat(tokens: &[Token<'_>], options: &Options) -> String {
    let mut text = String::new();
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 && space(tokens, index, options) {
            text.push(' ');
        }
        text.push_str(&spelling::render(
            token.text,
            token.key,
            options.keyword_case,
        ));
    }
    text
}

/// Render one complete logical unit, keeping its first indentation outside the returned body.
pub fn render(
    input: &str,
    tokens: &[Token<'_>],
    options: &Options,
    indent: usize,
    eol: &str,
    alignment: Option<usize>,
) -> Result<String, Error> {
    let mut writer = Writer {
        text: String::new(),
        options,
        eol,
        column: indent,
        line_indent: indent,
        has_text: false,
        line_comment_open: false,
    };
    sequence(input, tokens, &mut writer, indent, alignment)?;
    Ok(writer.text)
}

/// Print groups recursively while allowing their own fit decisions to use the current column.
fn sequence(
    input: &str,
    tokens: &[Token<'_>],
    writer: &mut Writer<'_>,
    base: usize,
    alignment: Option<usize>,
) -> Result<(), Error> {
    let flattened = flat(tokens, writer.options);
    let long = writer.options.column_limit != 0
        && width(&flattened, writer.column, writer.options.tab_width)
            > usize::from(writer.options.column_limit);
    let mut at = 0;
    while at < tokens.len() {
        let token = &tokens[at];
        let binary = binary(tokens, at);
        let before = at > 0 && input[tokens[at - 1].range.end..token.range.start].contains('\n');
        let after = tokens
            .get(at + 1)
            .is_some_and(|next| input[token.range.end..next.range.start].contains('\n'));
        let wrap = binary
            && match writer.options.wrap_binary {
                Wrapping::OnePerLine => true,
                Wrapping::Fit => long,
                Wrapping::Preserve => before || after,
            };
        let leading = if writer.options.wrap_binary == Wrapping::Preserve {
            before
        } else {
            writer.options.break_before_binary_operators
        };
        if token.kind == Kind::Comment && before && writer.has_text {
            writer.newline(base + usize::from(writer.options.continuation_indent_width))?;
        }
        if wrap && leading && writer.has_text {
            writer.newline(base + usize::from(writer.options.continuation_indent_width))?;
        }
        let aligned = alignment.is_some() && assignment(tokens) == Some(at);
        if aligned {
            writer.padding(alignment.unwrap_or_default().saturating_sub(writer.column))?;
        } else if at > 0 && writer.has_text && space(tokens, at, writer.options) {
            writer.padding(1)?;
        }
        if token.is("(") || token.is("[") {
            let end = group_end(tokens, at)?;
            group(input, tokens, at, end, writer)?;
            at = end;
            continue;
        }
        writer.push(&spelling::render(
            token.text,
            token.key,
            writer.options.keyword_case,
        ))?;
        writer.line_comment_open = token.kind == Kind::Comment && token.text.starts_with("//");
        if token.kind == Kind::Comment && at + 1 < tokens.len() {
            writer.newline(
                base + if at > 0 {
                    usize::from(writer.options.continuation_indent_width)
                } else {
                    0
                },
            )?;
        } else if wrap && !leading && at + 1 < tokens.len() {
            writer.newline(base + usize::from(writer.options.continuation_indent_width))?;
        }
        at += 1;
    }
    Ok(())
}

/// Only actual call argument lists use call wrapping; grouping and indexing retain their grammar.
fn group(
    input: &str,
    tokens: &[Token<'_>],
    start: usize,
    end: usize,
    writer: &mut Writer<'_>,
) -> Result<(), Error> {
    let open = &tokens[start];
    let mut body = &tokens[start + 1..end - 1];
    let call = open.is("(")
        && start > 0
        && (tokens[start - 1].kind == Kind::Annotation
            || tokens[start - 1].kind == Kind::Word && tokens[start - 1].key.is_none()
            || tokens[start - 1].is("?"));
    let initial = writer.line_indent;
    let interior = flat(body, writer.options);
    let long = writer.options.column_limit != 0
        && width(&interior, writer.column + 2, writer.options.tab_width)
            > usize::from(writer.options.column_limit);
    let existing = input[open.range.end..tokens[end - 1].range.start].contains('\n');
    let wrap = call
        && !body.is_empty()
        && match writer.options.wrap_arguments {
            Wrapping::OnePerLine => true,
            Wrapping::Fit => long || body.iter().any(|token| token.kind == Kind::Comment),
            Wrapping::Preserve => existing,
        };
    writer.push(open.text)?;
    if wrap
        && body.first().is_some_and(|token| {
            token.kind == Kind::Comment
                && token.text.starts_with("//")
                && !input[open.range.end..token.range.start].contains('\n')
        })
    {
        writer.padding(1)?;
        writer.push(body[0].text)?;
        writer.line_comment_open = true;
        body = &body[1..];
    }
    if wrap && !body.is_empty() {
        let indent = initial + usize::from(writer.options.continuation_indent_width);
        let mut at = 0;
        for end in argument_ends(input, body)? {
            writer.newline(indent)?;
            let comma = end > at && body[end - 1].is(",");
            sequence(
                input,
                &body[at..end - usize::from(comma)],
                writer,
                indent,
                None,
            )?;
            if comma {
                if writer.line_comment_open {
                    writer.newline(indent)?;
                }
                writer.push(",")?;
            }
            at = end;
        }
    } else {
        if open.is("(") && writer.options.space_inside_parentheses {
            writer.padding(1)?;
        }
        sequence(input, body, writer, initial, None)?;
    }
    if writer.line_comment_open {
        writer.newline(initial)?;
    }
    if open.is("(") && writer.options.space_inside_parentheses && !body.is_empty() {
        writer.padding(1)?;
    }
    writer.push(tokens[end - 1].text)
}

/// Top-level commas delimit arguments; nested calls keep their own comma lists.
fn argument_ends(input: &str, tokens: &[Token<'_>]) -> Result<Vec<usize>, Error> {
    let mut output = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        if tokens[at].is("(") || tokens[at].is("[") {
            at = group_end(tokens, at)?;
        } else {
            if tokens[at].is(",") {
                if tokens.get(at + 1).is_some_and(|token| {
                    token.kind == Kind::Comment
                        && !input[tokens[at].range.end..token.range.start].contains('\n')
                }) {
                    at += 1;
                }
                output.push(at + 1);
            }
            at += 1;
        }
    }
    output.push(tokens.len());
    Ok(output)
}

/// Indentation uses tabs only for complete stops; continuation remainders use ordinary spaces.
pub fn indentation(columns: usize, options: &Options) -> String {
    if options.indent_style == IndentStyle::Tab {
        let tab = usize::from(options.tab_width);
        format!(
            "{}{}",
            "\t".repeat(columns / tab),
            " ".repeat(columns % tab)
        )
    } else {
        " ".repeat(columns)
    }
}

/// Column counting is Unicode-safe and treats a tab as a stop rather than one character.
pub fn width(text: &str, initial: usize, tab: u16) -> usize {
    text.chars().fold(initial, |column, value| match value {
        '\n' => 0,
        '\r' => column,
        '\t' => column + usize::from(tab) - column % usize::from(tab),
        _ => column + 1,
    })
}

/// Parentheses, member access and omitted arguments are spaced by token roles, never regex.
fn space(tokens: &[Token<'_>], at: usize, options: &Options) -> bool {
    let previous = &tokens[at - 1];
    let current = &tokens[at];
    if current.kind == Kind::Comment {
        return true;
    }
    if previous.is(".")
        || previous.is("~")
        || current.is(".")
        || current.is("[")
        || current.is(",")
        || current.is(";")
        || current.is(":")
        || current.is("]")
    {
        return false;
    }
    if current.is(")") {
        return options.space_inside_parentheses && !previous.is("(");
    }
    if previous.is("(") {
        return options.space_inside_parentheses;
    }
    if previous.is("[") {
        return false;
    }
    if current.is("(") {
        if operator(previous) && !unary(tokens, at - 1) {
            return options.space_around_operators;
        }
        return if previous.kind == Kind::Word && previous.key.is_none()
            || previous.kind == Kind::Annotation
            || previous.is("?")
        {
            options.space_before_call_parenthesis
        } else {
            previous.kind == Kind::Word
        };
    }
    if previous.is(",") {
        return options.space_after_comma;
    }
    if matches!(
        previous.key,
        Some(Key::And | Key::Or | Key::Not | Key::Await)
    ) || matches!(current.key, Some(Key::And | Key::Or))
    {
        return true;
    }
    if unary(tokens, at - 1) {
        return false;
    }
    if operator(current) || operator(previous) {
        return options.space_around_operators;
    }
    if unary(tokens, at) {
        return previous.kind == Kind::Word
            || previous.kind == Kind::Literal
            || previous.kind == Kind::Number;
    }
    true
}

/// Only the first outer assignment can participate in a group alignment.
pub fn assignment(tokens: &[Token<'_>]) -> Option<usize> {
    let mut depth = 0_u16;
    for (index, token) in tokens.iter().enumerate() {
        match token.text {
            "(" | "[" => depth += 1,
            ")" | "]" => depth = depth.saturating_sub(1),
            "=" if depth == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

/// A leading sign is not an infix break opportunity and must stay attached to its operand.
fn unary(tokens: &[Token<'_>], at: usize) -> bool {
    (tokens[at].is("+") || tokens[at].is("-"))
        && (at == 0
            || tokens[at - 1].is("(")
            || tokens[at - 1].is("[")
            || tokens[at - 1].is(",")
            || operator(&tokens[at - 1])
            || matches!(
                tokens[at - 1].key,
                Some(Key::Return | Key::Raise | Key::Then | Key::Do)
            ))
}

/// The first assignment equals is a spacing boundary, but not a continuation break target.
fn binary(tokens: &[Token<'_>], at: usize) -> bool {
    operator(&tokens[at])
        && !unary(tokens, at)
        && !(tokens[at].is("=")
            && tokens[..at]
                .iter()
                .all(|token| token.key.is_none() && !token.is("(") && !token.is("=")))
}

/// Boolean keyword boundaries always retain lexical separation, even with compact operator style.
fn operator(token: &Token<'_>) -> bool {
    matches!(token.key, Some(Key::And | Key::Or))
        || matches!(
            token.text,
            "=" | "<>" | "<" | ">" | "<=" | ">=" | "+" | "-" | "*" | "/" | "%"
        )
}

struct Writer<'a> {
    text: String,
    options: &'a Options,
    eol: &'a str,
    column: usize,
    line_indent: usize,
    has_text: bool,
    line_comment_open: bool,
}

impl Writer<'_> {
    /// Every append is bounded before allocation; opaque multiline tokens keep their own bytes.
    fn push(&mut self, text: &str) -> Result<(), Error> {
        if self.text.len().saturating_add(text.len()) > MAX_OUTPUT {
            return Err(Error::new("output_too_large", "text"));
        }
        self.column = width(text, self.column, self.options.tab_width);
        if let Some((_, tail)) = text.rsplit_once('\n') {
            self.line_indent = width(
                &tail[..tail
                    .find(|value: char| !value.is_whitespace())
                    .unwrap_or(tail.len())],
                0,
                self.options.tab_width,
            );
        }
        self.text.push_str(text);
        self.has_text = true;
        Ok(())
    }

    /// Generated spaces are separate from token bodies and can never alter a literal or comment.
    fn padding(&mut self, columns: usize) -> Result<(), Error> {
        if columns > 8192 {
            return Err(Error::new("output_too_large", "padding"));
        }
        self.push(&" ".repeat(columns))
    }

    /// Newline indentation is measured from a syntax block rather than the previous token column.
    fn newline(&mut self, columns: usize) -> Result<(), Error> {
        self.push(self.eol)?;
        self.push(&indentation(columns, self.options))?;
        self.line_indent = columns;
        self.has_text = false;
        self.line_comment_open = false;
        Ok(())
    }
}
