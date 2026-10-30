//! Only source indentation before continuation bars is mutable inside BSL string tokens.

use crate::output::append;
use eska_project_core::formatting::Error;

/// Dates and single-line strings never enter continuation layout or relaxed token comparison.
pub fn multiline(text: &str) -> bool {
    text.starts_with('"') && text.contains('\n')
}

/// Preserve every content byte, physical newline and source comment while aligning continuation bars.
pub fn reindent(text: &str, indent: &str) -> Result<String, Error> {
    let mut output = String::new();
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let content = continuation(line, index);
        if content.len() != line.len() || index > 0 && content.starts_with('|') {
            append(&mut output, indent)?;
        }
        append(&mut output, content)?;
    }
    Ok(output)
}

/// Equality ignores only indentation removed by the same renderer, including zero-width prefixes.
pub fn equivalent(before: &str, after: &str) -> bool {
    if !multiline(before) || !multiline(after) {
        return before == after;
    }
    before
        .split_inclusive('\n')
        .enumerate()
        .map(|(index, line)| continuation(line, index))
        .eq(after
            .split_inclusive('\n')
            .enumerate()
            .map(|(index, line)| continuation(line, index)))
}

/// Whitespace after the bar is part of the value; comments between physical lines stay byte-exact.
fn continuation(line: &str, index: usize) -> &str {
    let trimmed = line.trim_start_matches([' ', '\t']);
    if index > 0 && trimmed.starts_with('|') {
        trimmed
    } else {
        line
    }
}
