//! Project-local Git attributes preserve input bytes; effective Git policy belongs to VCS.

use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use super::{
    Project, SourceFormat,
    publish::{self, PublishError},
    source_format::{Error, PROJECT_SOURCE_IMPORT_DIRECTORY, SourcePath},
};

const ATTRIBUTES: &str = ".gitattributes";
const MAX_ATTRIBUTES_BYTES: u64 = 1024 * 1024;
const BYTE_ATTRIBUTES: &str = "-text -filter -ident !working-tree-encoding";

/// A reviewable exact-byte addition; no Git command, index or configuration is consulted.
#[derive(Debug)]
pub struct TransportPlan {
    root: PathBuf,
    original: Option<String>,
    updated: String,
}

impl TransportPlan {
    /// Append scoped rules after existing attributes, preserving all original bytes.
    ///
    /// The current project must select ESKA. Nested attributes and Git's info/global files
    /// are deliberately not rewritten: a VCS caller must verify its effective attributes.
    ///
    /// # Errors
    /// Returns an unsupported format, unsafe path, oversized/non-UTF-8 file or I/O failure.
    pub fn prepare(project: &Project) -> Result<Self, Error> {
        let SourceFormat::Eska { accepted_base } = project.configuration().source_format() else {
            return Err(Error::root("source_format_mismatch"));
        };
        let relative = project
            .source()
            .strip_prefix(project.root())
            .map_err(|_| Error::root("source_path_invalid"))?;
        let source = portable_path(relative)?;
        if source.as_path().starts_with(accepted_base.as_path())
            || accepted_base.as_path().starts_with(source.as_path())
        {
            return Err(Error::root("source_baseline_overlap"));
        }
        let root = project.root();
        if root.canonicalize().map_err(io_error)? != root {
            return Err(Error::root("transport_unsafe_path"));
        }
        let original = read_attributes(root)?;
        let previous = original.as_deref().unwrap_or_default();
        let newline = if previous
            .split_once('\n')
            .is_some_and(|(line, _)| line.ends_with('\r'))
        {
            "\r\n"
        } else {
            "\n"
        };
        let block = format!(
            "# eska: preserve source and accepted bytes{newline}{} {BYTE_ATTRIBUTES}{newline}{} {BYTE_ATTRIBUTES}{newline}{} {BYTE_ATTRIBUTES}{newline}",
            quoted_pattern(&source),
            quoted_pattern(accepted_base),
            quoted_pattern(&SourcePath::parse(PROJECT_SOURCE_IMPORT_DIRECTORY)?),
        );
        let mut updated = previous.to_owned();
        if !previous.ends_with(&block) {
            if !previous.is_empty() && !previous.ends_with('\n') {
                updated.push_str(newline);
            }
            updated.push_str(&block);
        }
        Ok(Self {
            root: root.to_owned(),
            original,
            updated,
        })
    }

    /// Preview all output bytes, including the untouched existing attributes.
    #[must_use]
    pub fn updated(&self) -> &str {
        &self.updated
    }

    /// Repeating preparation with the same final rules does not write or duplicate them.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.original.as_deref() == Some(&self.updated)
    }

    /// Publish only when the attributes still match the previewed snapshot.
    ///
    /// # Errors
    /// Returns a concurrent change, unsafe/read-only path or filesystem failure.
    pub fn publish(&self) -> Result<(), Error> {
        let path = Path::new(ATTRIBUTES);
        let result = self.original.as_ref().map_or_else(
            || publish::publish_new(&self.root, path, &self.updated),
            |original| publish::publish_snapshot(&self.root, path, original, &self.updated),
        );
        result
            .map_err(publication_error)
            .map_err(|error| error.in_field(ATTRIBUTES))
    }
}

/// Normalize host separators and redundant dots before deriving an anchored literal pattern.
fn portable_path(path: &Path) -> Result<SourcePath, Error> {
    let mut parts = Vec::new();
    for part in path.components() {
        match part {
            Component::CurDir => (),
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .ok_or_else(|| Error::root("source_path_invalid"))?,
            ),
            _ => return Err(Error::root("source_path_invalid")),
        }
    }
    SourcePath::parse(&parts.join("/"))
}

/// Git decodes C-style quoting before wildmatch; double escapes keep brackets literal.
fn quoted_pattern(path: &SourcePath) -> String {
    let mut pattern = String::from("\"/");
    for character in path.as_str().chars() {
        if matches!(character, '[' | ']') {
            pattern.push_str("\\\\");
        }
        pattern.push(character);
    }
    pattern.push_str("/**\"");
    pattern
}

/// Read only a bounded ordinary file; aliases cannot redirect a project attributes update.
fn read_attributes(root: &Path) -> Result<Option<String>, Error> {
    let path = root.join(ATTRIBUTES);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => (),
        Ok(_) => return Err(Error::root("transport_unsafe_path")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(io_error)?
        .take(MAX_ATTRIBUTES_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > MAX_ATTRIBUTES_BYTES {
        return Err(Error::root("transport_attributes_too_large"));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| Error::root("transport_encoding_unsupported"))
}

/// Keep publication failures stable and leave localization to the tool boundary.
fn publication_error(error: PublishError) -> Error {
    match error {
        PublishError::Conflict => Error::root("transport_conflict"),
        PublishError::UnsafePath => Error::root("transport_unsafe_path"),
        PublishError::ReadOnly => Error::root("transport_read_only"),
        PublishError::Io(error) => io_error(error),
    }
}

/// The transport plan exposes a machine code without OS-localized error text.
fn io_error(_error: std::io::Error) -> Error {
    Error::root("transport_io")
}

#[cfg(test)]
mod tests;
