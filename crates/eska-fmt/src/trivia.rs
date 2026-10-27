//! Physical whitespace policy is identical for BSL modules and standalone query packages.

use eska_project_core::formatting::Options;

/// Existing physical line endings determine inserted syntax independently of the host OS.
pub fn eol(input: &str) -> &str {
    if input
        .find('\n')
        .is_some_and(|at| at > 0 && input.as_bytes()[at - 1] == b'\r')
    {
        "\r\n"
    } else {
        "\n"
    }
}

/// Retained physical trivia lines preserve their whitespace when trimming is disabled.
pub fn breaks(raw: &str, count: usize, eol: &str, trim: bool) -> String {
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

/// A retained final whitespace line needs its own newline before the empty-line budget is applied.
pub fn tail(raw: &str, options: &Options, eol: &str) -> String {
    let old = raw.chars().filter(|value| *value == '\n').count();
    let append_after_spaces = options.insert_final_newline
        && !options.trim_trailing_whitespace
        && raw.rsplit('\n').next().is_some_and(|row| !row.is_empty());
    let count = (old + usize::from(append_after_spaces))
        .min(usize::from(options.max_empty_lines) + 1)
        .max(usize::from(options.insert_final_newline));
    let mut output = breaks(raw, count, eol, options.trim_trailing_whitespace);
    if !options.trim_trailing_whitespace && !options.insert_final_newline && count == old {
        output.push_str(raw.rsplit('\n').next().unwrap_or_default());
    }
    output
}
