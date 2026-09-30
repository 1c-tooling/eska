//! Shared terminal diagnostics and progress for commands invoking the 1C platform.

use std::io::{self, IsTerminal, Write};

pub(super) mod progress;
use progress::ProgressLine;

/// Preserve native bytes and line endings, styling only recognized severity prefixes.
pub(super) fn write_diagnostic(
    line: &[u8],
    styled: bool,
    progress: Option<&ProgressLine>,
) -> io::Result<()> {
    let Ok(message) = std::str::from_utf8(line) else {
        return write_diagnostic_bytes(line, progress);
    };
    let message = style_diagnostic_level(message, styled);
    write_diagnostic_bytes(message.as_bytes(), progress)
}

/// Write one diagnostic while keeping an interactive progress line at the bottom.
fn write_diagnostic_bytes(line: &[u8], progress: Option<&ProgressLine>) -> io::Result<()> {
    if let Some(progress) = progress {
        return progress.write_diagnostic(line);
    }
    let mut stderr = io::stderr().lock();
    stderr.write_all(line)?;
    if !line.ends_with(b"\n") {
        stderr.write_all(b"\n")?;
    }
    stderr.flush()
}

/// Highlight a recognized ibcmd severity prefix while keeping its message unchanged.
fn style_diagnostic_level(message: &str, styled: bool) -> String {
    if !styled {
        return message.to_owned();
    }
    for (level, color) in [
        ("[TRACE]", "90"),
        ("[DEBUG]", "34"),
        ("[INFO]", "36"),
        ("[WARN]", "33"),
        ("[ERROR]", "31"),
        ("[FATAL]", "35"),
    ] {
        if let Some(rest) = message.strip_prefix(level) {
            return format!("\x1b[1;{color}m{level}\x1b[0m{rest}");
        }
    }
    message.to_owned()
}

/// Enable diagnostic colors only for an interactive stderr that permits color.
pub(super) fn diagnostic_styling_enabled() -> bool {
    io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Enable result colors only for an interactive stdout that permits color.
pub(super) fn result_styling_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Add a stable marker and optionally color only that marker.
pub(super) fn decorate_status(marker: &str, message: &str, styled: bool, color: &str) -> String {
    if styled {
        format!("\x1b[1;{color}m{marker}\x1b[0m {message}")
    } else {
        format!("{marker} {message}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Color only the recognized diagnostic prefix and preserve the message bytes.
    fn styles_known_diagnostic_levels() {
        for (level, color) in [
            ("TRACE", "90"),
            ("DEBUG", "34"),
            ("INFO", "36"),
            ("WARN", "33"),
            ("ERROR", "31"),
            ("FATAL", "35"),
        ] {
            let message = format!("[{level}] diagnostic\n");
            assert_eq!(
                style_diagnostic_level(&message, true),
                format!("\x1b[1;{color}m[{level}]\x1b[0m diagnostic\n")
            );
            assert_eq!(style_diagnostic_level(&message, false), message);
        }
    }

    #[test]
    /// Leave unknown prefixes unchanged even when terminal styling is enabled.
    fn preserves_unknown_diagnostic_prefixes() {
        assert_eq!(
            style_diagnostic_level("plain diagnostic\n", true),
            "plain diagnostic\n"
        );
    }
}
