//! Standalone syntax formatting; the shared project core owns policy and source protection.

mod bsl;
mod lex;
mod spelling;

#[cfg(test)]
mod tests;

use eska_project_core::formatting::{
    Error, Language,
    protocol::{Edit, Range, Request, Response, SCHEMA_VERSION, source_sha256},
};

/// Format a checked snapshot without reading a file, an analyzer index or editor settings.
///
/// # Errors
/// Rejects invalid/unsupported syntax, unsafe ranges, malformed directives and changed token values.
pub fn format(request: &Request) -> Result<Response, Error> {
    request.validate()?;
    let tokens = lex::tokens(&request.text, request.language)?;
    let (text, formatted_range) = match request.language {
        Language::Bsl => bsl::format(request, &tokens)?,
        Language::Sdbl => return Err(Error::new("unsupported_language", "sdbl")),
    };
    let after = lex::tokens(&text, request.language)?;
    if !lex::equivalent(&tokens, &after) {
        return Err(Error::new("token_mismatch", "text"));
    }
    let edits = minimal_edit(&request.text, &text).into_iter().collect();
    Ok(Response {
        schema_version: SCHEMA_VERSION,
        source_sha256: source_sha256(&request.text),
        edits,
        formatted_range,
    })
}

/// Trim only common UTF-8 character boundaries so one native edit remains undoable and exact.
fn minimal_edit(before: &str, after: &str) -> Option<Edit> {
    if before == after {
        return None;
    }
    let prefix: usize = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(value, _)| value.len_utf8())
        .sum();
    let suffix: usize = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(value, _)| value.len_utf8())
        .sum();
    Some(Edit {
        range: Range {
            start: prefix,
            end: before.len() - suffix,
        },
        text: after[prefix..after.len() - suffix].into(),
    })
}
