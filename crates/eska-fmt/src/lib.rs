//! Standalone syntax formatting; the shared project core owns policy and source protection.

mod bsl;
mod lex;
mod output;
mod query;
mod sdbl;
mod spelling;
mod trivia;

pub use query::format_query;

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
        Language::Sdbl => sdbl::format(request, &tokens)?,
    };
    let after = lex::tokens(&text, request.language)?;
    let equivalent = match request.language {
        Language::Bsl => lex::equivalent(&tokens, &after),
        Language::Sdbl => sdbl::equivalent(&tokens, &after),
    };
    if !equivalent {
        return Err(Error::new("token_mismatch", "text"));
    }
    let edits = edits(request.language, &request.text, &text)?;
    let response = Response {
        schema_version: SCHEMA_VERSION,
        source_sha256: source_sha256(&request.text),
        edits,
        formatted_range,
    };
    eska_project_core::formatting::protection::check_response(request, &response)?;
    Ok(response)
}

/// Protected methods divide the edit stream so native replacements never include their unchanged bytes.
fn edits(language: Language, before: &str, after: &str) -> Result<Vec<Edit>, Error> {
    if language != Language::Bsl {
        return Ok(minimal_edit(before, after).into_iter().collect());
    }
    let old = eska_project_core::formatting::protection::controlled_ranges(before)?;
    let new = eska_project_core::formatting::protection::controlled_ranges(after)?;
    if old.len() != new.len() {
        return Err(Error::new("change_control_protected", "method"));
    }
    let mut result = Vec::new();
    let (mut source, mut output) = (0, 0);
    for (old, new) in old.into_iter().zip(new).chain(std::iter::once((
        before.len()..before.len(),
        after.len()..after.len(),
    ))) {
        if before[old.clone()] != after[new.clone()] {
            return Err(Error::new("change_control_protected", "method"));
        }
        if let Some(mut edit) = minimal_edit(&before[source..old.start], &after[output..new.start])
        {
            edit.range.start += source;
            edit.range.end += source;
            result.push(edit);
        }
        source = old.end;
        output = new.end;
    }
    Ok(result)
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
