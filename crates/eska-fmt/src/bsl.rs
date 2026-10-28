//! BSL planning connects syntax units to layout while preserving frozen ranges and source trivia.

mod alignment;
mod expressions;
pub mod layout;
mod spacing;
mod syntax;

use crate::lex::{Kind, Token};
use crate::{output::append, trivia};
use eska_project_core::formatting::{
    Comments, Error, Options,
    protocol::{Range, Request},
};
use std::ops::Range as ByteRange;

/// Format whole grammar units and report the expanded source range to native editor adapters.
pub fn format(request: &Request, tokens: &[Token<'_>]) -> Result<(String, Range), Error> {
    let units = syntax::parse(&request.text, tokens)?;
    let mut frozen = disabled(&request.text, tokens)?;
    frozen.extend(eska_project_core::formatting::protection::controlled_ranges(&request.text)?);
    if units.is_empty() {
        return Ok((
            request.text.clone(),
            Range {
                start: 0,
                end: request.text.len(),
            },
        ));
    }
    let eol = trivia::eol(&request.text);
    let selected = request.range.unwrap_or(Range {
        start: 0,
        end: request.text.len(),
    });
    let mut output = String::new();
    let bom = usize::from(request.text.starts_with('\u{feff}')) * 3;
    append(&mut output, &request.text[..bom])?;
    let mut previous = bom;
    let mut actual = Range {
        start: request.text.len(),
        end: 0,
    };
    let alignments = alignment::plan(&units, tokens, &request.text, &request.options);
    let gaps = spacing::plan(&units, tokens, &request.text, &request.options);
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
            append(
                &mut output,
                &prefix(
                    &request.text[previous..part[0].range.start],
                    &request.options,
                    indent,
                    eol,
                    index == 0,
                    gaps[index],
                ),
            )?;
            append(
                &mut output,
                &layout::render(
                    &request.text,
                    part,
                    &request.options,
                    indent,
                    eol,
                    &alignments[index],
                )?,
            )?;
        } else {
            append(&mut output, &request.text[source.clone()])?;
        }
        previous = end;
    }
    if selected.end == request.text.len() && !frozen.iter().any(|range| range.contains(&previous)) {
        append(
            &mut output,
            &trivia::tail(&request.text[previous..], &request.options, eol),
        )?;
        actual.start = actual.start.min(previous);
        actual.end = request.text.len();
    } else {
        append(&mut output, &request.text[previous..])?;
    }
    syntax::parse(&output, &crate::lex::tokens(&output, request.language)?)?;
    if actual.start > actual.end {
        actual = selected;
    }
    Ok((output, actual))
}

/// Preserve ordinary blank-line intent within the selected structural gap policy.
fn prefix(
    raw: &str,
    options: &Options,
    indent: usize,
    eol: &str,
    first: bool,
    gap: spacing::Gap,
) -> String {
    let old = raw.chars().filter(|value| *value == '\n').count();
    let lines = gap.lines(old, options, first);
    format!(
        "{}{}",
        trivia::breaks(raw, lines, eol, options.trim_trailing_whitespace),
        layout::indentation(indent, options)
    )
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

/// Half-open intersection is shared by range selection and frozen-source checks.
const fn intersects(left: &ByteRange<usize>, right: ByteRange<usize>) -> bool {
    left.start < right.end && right.start < left.end
}
