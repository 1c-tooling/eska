//! Platform output addresses are hints; unresolved or ambiguous messages retain their raw bytes.

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::project_core::source_format::{MappedFile, SourceMap, SourcePath};

/// Match an ibcmd `File:` address to exactly one file in a caller-verified source map.
///
/// The accepted grammar is a bracketed level followed by `File: <absolute path>, <message>`.
/// Only paths inside the exact snapshot root participate. Commas inside filenames can make
/// a line ambiguous; this method never guesses between two known file prefixes. Unsupported
/// output/encoding produces no annotation and must remain visible to the caller.
#[must_use]
pub fn locate_output_file<'a>(
    map: &'a SourceMap,
    native_root: &Path,
    line: &[u8],
) -> Option<&'a MappedFile> {
    if !native_root.is_absolute() {
        return None;
    }
    let text = std::str::from_utf8(line)
        .ok()?
        .trim_end_matches(['\r', '\n']);
    let (level, address) = text.strip_prefix('[')?.split_once("] File: ")?;
    if !matches!(level, "WARN" | "ERROR" | "FATAL") {
        return None;
    }
    let root = native_root.to_str()?.trim_end_matches(['/', '\\']);
    let relative = address.strip_prefix(root)?.strip_prefix(['/', '\\'])?;
    let relative = relative.replace('\\', "/");
    let mut result = None;
    for (index, _) in relative.match_indices(", ") {
        let Ok(path) = SourcePath::parse(&relative[..index]) else {
            continue;
        };
        if let Some(file) = map.file(&path) {
            if result.is_some() {
                return None;
            }
            result = Some(file);
        }
    }
    result
}
