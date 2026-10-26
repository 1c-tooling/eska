//! Query layout uses the core's lossless clause model, never the constructor's write permissions.

mod layout;
mod structure;
mod words;

#[cfg(test)]
mod tests;

use crate::lex::{Kind, Token};
use eska_project_core::{
    formatting::{
        Comments, Error,
        protocol::{Range, Request},
    },
    project::query::constructor::{self, Constructor, DcsKind, Model},
};
use std::{ops::Range as Bytes, sync::Arc};

pub use words::equivalent;

/// Ranges expand to complete top-level package statements, including every UNION branch.
pub fn format(request: &Request, tokens: &[Token<'_>]) -> Result<(String, Range), Error> {
    let model = inspect(&request.text)?;
    let roles = words::classify(tokens);
    let events = structure::events(&request.text, tokens, &roles, &model, &request.options)?;
    let disabled = disabled(&request.text, tokens)?;
    let selected = request.range.unwrap_or(Range {
        start: 0,
        end: request.text.len(),
    });
    let bom = usize::from(request.text.starts_with('\u{feff}')) * 3;
    let eol = if request
        .text
        .find('\n')
        .is_some_and(|at| at > 0 && request.text.as_bytes()[at - 1] == b'\r')
    {
        "\r\n"
    } else {
        "\n"
    };
    let mut output = request.text[..bom].to_owned();
    let mut previous = bom;
    let mut actual = Range {
        start: request.text.len(),
        end: 0,
    };
    let roots: Vec<_> = model
        .scopes
        .iter()
        .filter(|scope| scope.parent.is_none())
        .collect();
    for (index, scope) in roots.iter().enumerate() {
        let end = if index + 1 == roots.len() {
            request.text.len()
        } else if request.text.as_bytes().get(scope.range.end) == Some(&b';') {
            scope.range.end + 1
        } else {
            scope.range.end
        };
        let source = previous..end;
        let first = tokens.partition_point(|token| token.range.start < previous);
        let last = tokens.partition_point(|token| token.range.start < end);
        let chosen = if selected.start == selected.end {
            source.start <= selected.start && selected.end <= source.end
        } else {
            source.start < selected.end && selected.start < source.end
        };
        let frozen = disabled
            .iter()
            .any(|range| range.start < source.end && source.start < range.end)
            || request.options.comments == Comments::Preserve
                && tokens[first..last]
                    .iter()
                    .any(|token| token.kind == Kind::Comment);
        if chosen && !frozen {
            actual.start = actual.start.min(previous);
            actual.end = end;
            if index > 0 {
                output.push_str(
                    &eol.repeat(usize::from(request.options.blank_lines_between_queries) + 1),
                );
            }
            output.push_str(&layout::render(
                &request.text,
                tokens,
                &roles,
                &events,
                first..last,
                &request.options,
                eol,
            )?);
        } else {
            output.push_str(&request.text[source]);
        }
        previous = end;
    }
    if actual.end == request.text.len() {
        let last = tokens.last().map_or(bom, |token| token.range.end);
        output.push_str(&tail(&request.text[last..], &request.options, eol));
    }
    output.push_str(&request.text[previous..]);
    if actual.start > actual.end {
        actual = selected;
    }
    let after = inspect(&output)?;
    if shape(&model) != shape(&after) {
        return Err(Error::new("structure_mismatch", "sdbl"));
    }
    Ok((output, actual))
}

/// End-of-file trivia follows explicit newline and trimming options without touching opaque token bodies.
fn tail(raw: &str, options: &eska_project_core::formatting::Options, eol: &str) -> String {
    let old = raw.chars().filter(|value| *value == '\n').count();
    let count = old
        .min(usize::from(options.max_empty_lines) + 1)
        .max(usize::from(options.insert_final_newline));
    let mut output = String::new();
    let mut rows = raw.split('\n');
    for _ in 0..count {
        if !options.trim_trailing_whitespace {
            output.push_str(rows.next().unwrap_or_default().trim_end_matches('\r'));
        }
        output.push_str(eol);
    }
    if !options.trim_trailing_whitespace && !options.insert_final_newline && count == old {
        output.push_str(raw.rsplit('\n').next().unwrap_or_default());
    }
    output
}

/// CASE and typed DCS projections prove clause boundaries without broadening visual editing authority.
fn inspect(input: &str) -> Result<Model, Error> {
    let model =
        constructor::formatting_model(input).map_err(|_| Error::new("invalid_syntax", "sdbl"))?;
    if let Some(statement) = model
        .statements
        .iter()
        .find(|statement| statement.unavailable.is_some())
    {
        return Err(Error::new("unsupported_syntax", "sdbl").at(statement.range.start));
    }
    let original =
        Constructor::new(Arc::from(input)).map_err(|_| Error::new("invalid_syntax", "sdbl"))?;
    let dcs = original
        .dcs_model()
        .map_err(|_| Error::new("invalid_syntax", "dcs"))?;
    if let Some(node) = dcs
        .statements
        .iter()
        .flat_map(|statement| &statement.nodes)
        .find(|node| node.enabled && node.kind == DcsKind::Opaque)
    {
        return Err(Error::new("unsupported_syntax", "dcs").at(node.range.start));
    }
    Ok(model)
}

/// Stable grammar shape ignores offsets and casing while retaining packages, clauses and list ownership.
fn shape(model: &Model) -> Vec<Vec<(constructor::ClauseKind, usize)>> {
    model
        .statements
        .iter()
        .map(|statement| {
            statement
                .clauses
                .iter()
                .map(|clause| (clause.kind, clause.items.len()))
                .collect()
        })
        .collect()
}

/// Only standalone lexical comments control frozen regions; a containing query is preserved as a unit.
fn disabled(input: &str, tokens: &[Token<'_>]) -> Result<Vec<Bytes<usize>>, Error> {
    let mut start = None;
    let mut result = Vec::new();
    for token in tokens.iter().filter(|token| token.kind == Kind::Comment) {
        let line = input[..token.range.start]
            .rfind('\n')
            .map_or(0, |at| at + 1);
        if !input[line..token.range.start].trim().is_empty() {
            continue;
        }
        match token.text.trim() {
            "// eska-fmt off" if start.is_none() => start = Some(line),
            "// eska-fmt on" if start.is_some() => {
                result.push(start.take().unwrap_or(line)..token.range.end);
            }
            "// eska-fmt off" | "// eska-fmt on" => {
                return Err(Error::new("invalid_directive", "eska-fmt").at(token.range.start));
            }
            _ => {}
        }
    }
    if start.is_some() {
        return Err(Error::new("invalid_directive", "eska-fmt"));
    }
    Ok(result)
}
