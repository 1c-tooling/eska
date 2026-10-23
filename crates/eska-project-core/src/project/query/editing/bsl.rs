//! Preserve untouched literal bytes; only explicit comment relocation rebuilds the literal.

use crate::project::metadata_edit::{EditPlan, Replacement};

use super::{CommentPlacement, Query, QueryError, difference, replacement};
use crate::project::query::Affinity;

struct Style {
    newline: &'static str,
    prefix: String,
}

/// An unaffected comment remains byte-for-byte outside the replacement, including its line endings.
pub(super) fn plan(
    input: &str,
    query: &Query,
    value: &str,
    decisions: &[CommentPlacement],
    placed: &[CommentPlacement],
) -> Result<EditPlan, QueryError> {
    let style = style(input, query);
    let (old, new) = difference(&query.text, value);
    let start = query
        .map
        .query_to_source(&query.text, old.start, Affinity::After)
        .ok_or(QueryError::InvalidPosition)?;
    let end = if old.is_empty() {
        start
    } else {
        query
            .map
            .query_to_source(
                &query.text,
                old.end,
                if old.end == query.text.len() {
                    Affinity::After
                } else {
                    Affinity::Before
                },
            )
            .ok_or(QueryError::InvalidPosition)?
    };
    let touches_comment = query
        .comments
        .iter()
        .any(|comment| comment.source.start < end && comment.source.end > start);
    let edit = if decisions.is_empty() && !touches_comment {
        Replacement {
            range: start..end,
            text: encode(&value[new], &style),
        }
    } else {
        Replacement {
            range: query.source.clone(),
            text: with_comments(query, value, placed, &style),
        }
    };
    replacement(input, edit)
}

/// Existing continuation indentation wins; a one-line literal inherits its owner's indentation.
fn style(input: &str, query: &Query) -> Style {
    let original = &input[query.source.clone()];
    let newline = original.find(['\r', '\n']).map_or("\n", |offset| {
        if original[offset..].starts_with("\r\n") {
            "\r\n"
        } else if original[offset..].starts_with('\r') {
            "\r"
        } else {
            "\n"
        }
    });
    let prefix = original
        .lines()
        .skip(1)
        .find_map(|line| {
            let spaces = line.len() - line.trim_start_matches([' ', '\t']).len();
            line[spaces..]
                .starts_with('|')
                .then(|| line[..=spaces].to_owned())
        })
        .unwrap_or_else(|| {
            let line = input[..query.source.start]
                .rsplit(['\r', '\n'])
                .next()
                .unwrap_or_default();
            let spaces = line.len() - line.trim_start_matches([' ', '\t']).len();
            format!("{}\t|", &line[..spaces])
        });
    Style { newline, prefix }
}

/// Literal bars and query comments remain ordinary characters; only BSL delimiters are escaped.
fn encode(value: &str, style: &Style) -> String {
    let mut result = String::new();
    for character in value.chars() {
        match character {
            '"' => result.push_str("\"\""),
            '\n' => {
                result.push_str(style.newline);
                result.push_str(&style.prefix);
            }
            _ => result.push(character),
        }
    }
    result
}

/// A comment destination is a verified line start, never interpolation inside a quoted SDBL token.
fn with_comments(query: &Query, value: &str, placed: &[CommentPlacement], style: &Style) -> String {
    let mut result = String::from("\"");
    let mut offset = 0;
    for (line, text) in value.split('\n').enumerate() {
        if line > 0 {
            result.push_str(style.newline);
            for placement in placed
                .iter()
                .filter(|placement| placement.query_offset == Some(offset))
            {
                result.push_str(&query.comments[placement.comment].text);
                result.push_str(style.newline);
            }
            result.push_str(&style.prefix);
        }
        result.push_str(&encode(text, style));
        offset += text.len() + 1;
    }
    result.push('"');
    result
}
