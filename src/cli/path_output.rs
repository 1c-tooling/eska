//! Safe local filesystem links for interactive human output.

use std::{fmt::Write as _, path::Path};

/// Link a path to itself only when its output stream is a terminal.
pub(super) fn render(path: &Path, hyperlink: bool) -> String {
    render_link(path, path, hyperlink)
}

/// Keep the visible label separate from the target, including build directory links.
pub(super) fn render_link(label: &Path, target: &Path, hyperlink: bool) -> String {
    let label = display_path(label);
    let Some(target) = hyperlink.then(|| file_uri(target)).flatten() else {
        return label;
    };
    format!("\x1b]8;;{target}\x1b\\{label}\x1b]8;;\x1b\\")
}

/// Escape control characters so filesystem names cannot inject terminal commands.
pub(super) fn display_path(path: &Path) -> String {
    let mut display = String::new();
    for character in path.to_string_lossy().chars() {
        if character.is_control() {
            display.extend(character.escape_default());
        } else {
            display.push(character);
        }
    }
    display
}

/// Resolve relative or missing targets without requiring the linked file to exist.
fn file_uri(path: &Path) -> Option<String> {
    let path = std::path::absolute(path).ok()?;
    #[cfg(unix)]
    let normalized = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let normalized = {
        let path = path.to_str()?.replace('\\', "/");
        // Win32 verbatim prefixes are filesystem syntax, not part of a file URI.
        let path = if let Some(unc) = path.strip_prefix("//?/UNC/") {
            format!("//{unc}")
        } else {
            path.strip_prefix("//?/").unwrap_or(&path).to_owned()
        };
        path.into_bytes()
    };
    let prefix = if normalized.starts_with(b"//") {
        "file:"
    } else if normalized.starts_with(b"/") {
        "file://"
    } else {
        "file:///"
    };
    let mut uri = prefix.to_owned();
    for byte in normalized {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            uri.push(char::from(byte));
        } else {
            write!(uri, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    Some(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode URI delimiters and Unicode while keeping the readable path label intact.
    #[test]
    fn links_paths_without_confusing_uri_delimiters() {
        let path = std::env::current_dir().unwrap().join("Проект #1%?.cf");
        let linked = render(&path, true);
        assert!(linked.starts_with("\x1b]8;;file:"));
        assert!(linked.contains("%D0%9F%D1%80%D0%BE%D0%B5%D0%BA%D1%82%20%231%25%3F.cf"));
        assert!(linked.ends_with(&format!("\x1b\\{}\x1b]8;;\x1b\\", path.display())));
        assert_eq!(render(&path, false), path.to_string_lossy());
    }

    /// A relative error path still points to an absolute target even before creation.
    #[test]
    fn resolves_relative_missing_targets() {
        let path = Path::new("missing project/src");
        assert_eq!(
            file_uri(path),
            file_uri(&std::env::current_dir().unwrap().join(path))
        );
        assert!(render(path, true).ends_with("\x1b\\missing project/src\x1b]8;;\x1b\\"));
    }

    /// Preserve Unix path bytes and backslashes instead of linking a lossy replacement.
    #[cfg(unix)]
    #[test]
    fn encodes_non_unicode_and_control_bytes() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        let path = Path::new(OsStr::from_bytes(b"/tmp/name\\\xff\x1b\n.cf"));
        let linked = render(path, true);
        assert!(linked.starts_with("\x1b]8;;file:///tmp/name%5C%FF%1B%0A.cf\x1b\\"));
        let label = display_path(path);
        assert!(!label.contains('\x1b'));
        assert!(!label.contains('\n'));
        assert!(label.contains("\\u{1b}\\n"));
    }
}
