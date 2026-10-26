//! BSL planning connects syntax units to layout while preserving frozen ranges and source trivia.

mod expressions;
mod layout;
mod syntax;

use crate::lex::{Kind, Token};
use eska_project_core::formatting::{
    Comments, Error, Options,
    protocol::{Range, Request},
};
use std::ops::Range as ByteRange;
use syntax::{Unit, UnitKind};

/// Format whole grammar units and report the expanded source range to native editor adapters.
pub fn format(request: &Request, tokens: &[Token<'_>]) -> Result<(String, Range), Error> {
    let units = syntax::parse(&request.text, tokens)?;
    let frozen = disabled(&request.text, tokens)?;
    if units.is_empty() {
        return Ok((
            request.text.clone(),
            Range {
                start: 0,
                end: request.text.len(),
            },
        ));
    }
    let eol = eol(&request.text);
    let selected = request.range.unwrap_or(Range {
        start: 0,
        end: request.text.len(),
    });
    let mut output = String::new();
    let bom = usize::from(request.text.starts_with('\u{feff}')) * 3;
    output.push_str(&request.text[..bom]);
    let mut previous = bom;
    let mut actual = Range {
        start: request.text.len(),
        end: 0,
    };
    let alignments = alignment(&units, tokens, &request.text, &request.options);
    for (index, unit) in units.iter().enumerate() {
        let part = &tokens[unit.tokens.clone()];
        let end = part
            .last()
            .ok_or_else(|| Error::new("invalid_syntax", "unit"))?
            .range
            .end;
        let source = previous..end;
        let selected_unit = if selected.start == selected.end {
            source.start < selected.start && selected.end <= source.end
                || index == 0 && selected.start <= source.start
        } else {
            intersects(&source, selected.start..selected.end)
        };
        let protected = frozen
            .iter()
            .any(|range| intersects(&source, range.clone()));
        let comment = request.options.comments == Comments::Preserve
            && part.iter().any(|token| token.kind == Kind::Comment);
        if selected_unit && !protected && !comment {
            actual.start = actual.start.min(previous);
            actual.end = end;
            let indent = unit.level * usize::from(request.options.indent_width);
            output.push_str(&prefix(
                &request.text[previous..part[0].range.start],
                &request.options,
                indent,
                eol,
                index == 0,
                index > 0 && units[index - 1].kind == UnitKind::MethodEnd,
            ));
            output.push_str(&layout::render(
                &request.text,
                part,
                &request.options,
                indent,
                eol,
                alignments[index],
            )?);
        } else {
            output.push_str(&request.text[source.clone()]);
        }
        previous = end;
    }
    if selected.end == request.text.len() && !frozen.iter().any(|range| range.contains(&previous)) {
        let tail = &request.text[previous..];
        let old_lines = tail.chars().filter(|value| *value == '\n').count();
        let append_after_spaces = request.options.insert_final_newline
            && !request.options.trim_trailing_whitespace
            && tail.rsplit('\n').next().is_some_and(|row| !row.is_empty());
        let lines = (old_lines + usize::from(append_after_spaces))
            .min(usize::from(request.options.max_empty_lines) + 1)
            .max(usize::from(request.options.insert_final_newline));
        output.push_str(&breaks(
            tail,
            lines,
            eol,
            request.options.trim_trailing_whitespace,
        ));
        if !request.options.trim_trailing_whitespace
            && !request.options.insert_final_newline
            && lines == old_lines
        {
            output.push_str(tail.rsplit('\n').next().unwrap_or_default());
        }
        actual.end = request.text.len();
    } else {
        output.push_str(&request.text[previous..]);
    }
    syntax::parse(&output, &crate::lex::tokens(&output, request.language)?)?;
    if actual.start > actual.end {
        actual = selected;
    }
    Ok((output, actual))
}

/// Preserve ordinary blank-line intent within the selected maximum, with an explicit method gap.
fn prefix(
    raw: &str,
    options: &Options,
    indent: usize,
    eol: &str,
    first: bool,
    after_method: bool,
) -> String {
    let old = raw.chars().filter(|value| *value == '\n').count();
    let minimum = if first {
        0
    } else if after_method {
        usize::from(options.blank_lines_between_methods) + 1
    } else {
        1
    };
    let lines = old
        .min(usize::from(options.max_empty_lines) + usize::from(!first))
        .max(minimum);
    format!(
        "{}{}",
        breaks(raw, lines, eol, options.trim_trailing_whitespace),
        layout::indentation(indent, options)
    )
}

/// Retained physical trivia lines preserve their whitespace when trimming is disabled.
fn breaks(raw: &str, count: usize, eol: &str, trim: bool) -> String {
    let mut output = String::new();
    let mut rows = raw.split('\n');
    for _ in 0..count {
        let row = rows.next().unwrap_or_default().trim_end_matches('\r');
        if !trim {
            output.push_str(row);
        }
        output.push_str(eol);
    }
    output
}

/// Markers only act as separate lexical comments; a quote containing the same text is inert.
fn disabled(input: &str, tokens: &[Token<'_>]) -> Result<Vec<ByteRange<usize>>, Error> {
    let mut start = None;
    let mut deletion = None;
    let mut output = Vec::new();
    for token in tokens {
        if token.kind == Kind::Directive {
            match crate::spelling::directive(token.text) {
                Some("#Удаление" | "#Delete") => {
                    let line = input[..token.range.start]
                        .rfind('\n')
                        .map_or(0, |at| at + 1);
                    if deletion.replace(line).is_some() {
                        return Err(Error::new("invalid_syntax", "nested_deletion"));
                    }
                }
                Some("#КонецУдаления" | "#EndDelete") => output.push(
                    deletion
                        .take()
                        .ok_or_else(|| Error::new("invalid_syntax", "unmatched_deletion"))?
                        ..token.range.end,
                ),
                _ => {}
            }
            continue;
        }
        if token.kind != Kind::Comment || deletion.is_some() {
            continue;
        }
        let marker = token.text.strip_prefix("//").unwrap_or_default().trim();
        if marker != "eska-fmt off" && marker != "eska-fmt on" {
            continue;
        }
        let line = input[..token.range.start]
            .rfind('\n')
            .map_or(0, |at| at + 1);
        if !input[line..token.range.start]
            .trim_matches([' ', '\t', '\r', '\u{feff}'])
            .is_empty()
        {
            return Err(Error::new("invalid_format_marker", "position"));
        }
        if marker == "eska-fmt off" {
            if start.replace(line).is_some() {
                return Err(Error::new("invalid_format_marker", "nested"));
            }
        } else {
            output.push(
                start
                    .take()
                    .ok_or_else(|| Error::new("invalid_format_marker", "unmatched_on"))?
                    ..token.range.end,
            );
        }
    }
    if start.is_some() {
        return Err(Error::new("invalid_format_marker", "unclosed_off"));
    }
    if deletion.is_some() {
        return Err(Error::new("invalid_syntax", "unclosed_deletion"));
    }
    Ok(output)
}

/// Adjacent assignments share a column only inside one syntax level and without a blank gap.
fn alignment(
    units: &[Unit],
    tokens: &[Token<'_>],
    input: &str,
    options: &Options,
) -> Vec<Option<usize>> {
    let mut output = vec![None; units.len()];
    if !options.align_assignments {
        return output;
    }
    let mut group: Vec<(usize, usize)> = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        let part = &tokens[unit.tokens.clone()];
        let assign =
            unit.kind == UnitKind::Statement && part[0].kind == Kind::Word && part[0].key.is_none();
        let equals = assign.then(|| layout::assignment(part)).flatten();
        let contiguous = index == 0
            || unit.level == units[index - 1].level
                && input[tokens[units[index - 1].tokens.end - 1].range.end..part[0].range.start]
                    .chars()
                    .filter(|value| *value == '\n')
                    .count()
                    <= 1;
        if equals.is_none() || !contiguous {
            finish_alignment(&group, &mut output);
            group.clear();
        }
        if let Some(at) = equals {
            group.push((
                index,
                unit.level * usize::from(options.indent_width)
                    + layout::width(&layout::flat(&part[..at], options), 0, options.tab_width)
                    + usize::from(options.space_around_operators),
            ));
        }
    }
    finish_alignment(&group, &mut output);
    output
}

/// A single assignment is left to normal spacing; a group uses its widest left-hand side.
fn finish_alignment(group: &[(usize, usize)], output: &mut [Option<usize>]) {
    if group.len() > 1 {
        let maximum = group.iter().map(|(_, width)| *width).max();
        for (index, _) in group {
            output[*index] = maximum;
        }
    }
}

/// Half-open intersection is shared by range selection and frozen-source checks.
const fn intersects(left: &ByteRange<usize>, right: ByteRange<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

/// Existing physical line endings determine inserted syntax independently of the host OS.
fn eol(input: &str) -> &str {
    if input
        .find('\n')
        .is_some_and(|at| at > 0 && input.as_bytes()[at - 1] == b'\r')
    {
        "\r\n"
    } else {
        "\n"
    }
}
