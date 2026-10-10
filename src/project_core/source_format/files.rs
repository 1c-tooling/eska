//! Bounded text reads and byte fingerprints; inventories never follow filesystem aliases.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::Path,
};

use sha2::{Digest, Sha256};

use super::{Error, FileEntry, FileRole, SourcePath};

pub(super) const MANIFEST: &str = "source.json";
pub(super) const MAX_TEXT_BYTES: u64 = 64 * 1024 * 1024;

/// A sorted, complete snapshot includes empty directories and every input payload.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Inventory {
    pub files: BTreeMap<SourcePath, FileEntry>,
    pub directories: Vec<SourcePath>,
    pub revision: String,
}

impl Inventory {
    /// Hash the entire chosen XML source tree without exclusions or guessed active objects.
    pub fn read(root: &Path) -> Result<Self, Error> {
        Self::read_cancellable(root, &|| false)
    }

    /// A stopped verification never returns a partial inventory or skips required payloads.
    pub fn read_cancellable(root: &Path, cancelled: &dyn Fn() -> bool) -> Result<Self, Error> {
        check_cancelled(cancelled)?;
        if !fs::symlink_metadata(root)
            .map_err(|error| io(&error))?
            .file_type()
            .is_dir()
        {
            return Err(Error::root("source_directory_required"));
        }
        let mut files = BTreeMap::new();
        let mut directories = Vec::new();
        let mut pending = vec![String::new()];
        let mut names = BTreeSet::new();
        while let Some(parent) = pending.pop() {
            check_cancelled(cancelled)?;
            for entry in fs::read_dir(root.join(&parent)).map_err(|error| io(&error))? {
                check_cancelled(cancelled)?;
                let entry = entry.map_err(|error| io(&error))?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| Error::root("source_path_invalid"))?;
                let path = SourcePath::parse(&if parent.is_empty() {
                    name
                } else {
                    format!("{parent}/{name}")
                })?;
                if !names.insert(path.as_str().to_lowercase()) {
                    return Err(Error::root("source_path_collision").in_field(path.as_str()));
                }
                let kind = entry.file_type().map_err(|error| io(&error))?;
                if kind.is_dir() {
                    directories.push(path.clone());
                    pending.push(path.as_str().into());
                } else if kind.is_file() {
                    let (bytes, sha256) = fingerprint(&root.join(path.as_path()), cancelled)?;
                    files.insert(
                        path.clone(),
                        FileEntry {
                            path,
                            bytes,
                            sha256,
                            role: FileRole::Preserved,
                        },
                    );
                } else {
                    return Err(Error::root("source_entry_unsupported").in_field(path.as_str()));
                }
            }
        }
        directories.sort();
        let revision = revision(&files, &directories);
        check_cancelled(cancelled)?;
        Ok(Self {
            files,
            directories,
            revision,
        })
    }
}

/// Metadata parsing has a bounded allocation even if a file grows after inventory creation.
pub(super) fn read_text(path: &Path) -> Result<String, Error> {
    read_text_limited(path, MAX_TEXT_BYTES)
}

/// A caller can tighten the source limit without weakening the format's hard bound.
pub(super) fn read_text_limited(path: &Path, max_bytes: u64) -> Result<String, Error> {
    read_text_cancellable(path, max_bytes, &|| false)
}

/// Read bounded text in chunks so a caller can stop before completing a large accepted blob.
pub(super) fn read_text_cancellable(
    path: &Path,
    max_bytes: u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<String, Error> {
    check_cancelled(cancelled)?;
    let max_bytes = max_bytes.min(MAX_TEXT_BYTES);
    let mut bytes = Vec::new();
    let mut input = fs::File::open(path)
        .map_err(|error| io(&error))?
        .take(max_bytes + 1);
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        check_cancelled(cancelled)?;
        let count = match input.read(&mut chunk) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result.map_err(|error| io(&error))?,
        };
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    check_cancelled(cancelled)?;
    if bytes.len() as u64 > max_bytes {
        return Err(Error::root("source_document_too_large"));
    }
    String::from_utf8(bytes).map_err(|_| Error::root("source_encoding_unsupported"))
}

/// Stream resource bytes once, checking the exact snapshot while exclusively creating output.
pub(super) fn copy(root: &Path, destination: &Path, entry: &FileEntry) -> Result<(), Error> {
    copy_to(root, &destination.join(entry.path.as_path()), entry)
}

/// Copy a verified payload to its physical storage path without intermediate trees.
pub(super) fn copy_to(root: &Path, path: &Path, entry: &FileEntry) -> Result<(), Error> {
    create_parent(path)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io(&error))?;
    let mut input = fs::File::open(root.join(entry.path.as_path())).map_err(|error| io(&error))?;
    let mut digest = Sha256::new();
    let mut bytes = [0_u8; 16 * 1024];
    let mut length = 0_u64;
    loop {
        let count = input.read(&mut bytes).map_err(|error| io(&error))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&bytes[..count])
            .map_err(|error| io(&error))?;
        digest.update(&bytes[..count]);
        length += count as u64;
    }
    if length != entry.bytes || hex(&digest.finalize()) != entry.sha256 {
        return Err(Error::root("source_input_changed").in_field(entry.path.as_str()));
    }
    Ok(())
}

/// Create an owned output file without replacing an existing path.
pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    create_parent(path)?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io(&error))?
        .write_all(bytes)
        .map_err(|error| io(&error))
}

/// Check already-created parents instead of following an unexpected symlink in output.
fn create_parent(path: &Path) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    match fs::symlink_metadata(parent) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(Error::root("source_entry_unsupported")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_parent(parent)?;
            fs::create_dir(parent).map_err(|error| io(&error))
        }
        Err(error) => Err(io(&error)),
    }
}

/// Hash payloads without retaining binary resources in memory.
fn fingerprint(path: &Path, cancelled: &dyn Fn() -> bool) -> Result<(u64, String), Error> {
    check_cancelled(cancelled)?;
    let mut input = fs::File::open(path).map_err(|error| io(&error))?;
    let mut digest = Sha256::new();
    let mut bytes = [0_u8; 16 * 1024];
    let mut length = 0;
    loop {
        check_cancelled(cancelled)?;
        let count = match input.read(&mut bytes) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result.map_err(|error| io(&error))?,
        };
        if count == 0 {
            check_cancelled(cancelled)?;
            return Ok((length, hex(&digest.finalize())));
        }
        digest.update(&bytes[..count]);
        length += count as u64;
    }
}

/// Only an explicit caller stop has this code; ordinary storage failures retain their old codes.
pub(super) fn check_cancelled(cancelled: &dyn Fn() -> bool) -> Result<(), Error> {
    if cancelled() {
        Err(Error::root("source_read_cancelled"))
    } else {
        Ok(())
    }
}

/// Frame paths, lengths and hashes unambiguously; no timestamp or private absolute path is stored.
pub(super) fn revision(
    files: &BTreeMap<SourcePath, FileEntry>,
    directories: &[SourcePath],
) -> String {
    let mut digest = Sha256::new();
    for path in directories {
        digest.update([0]);
        digest.update((path.as_str().len() as u64).to_le_bytes());
        digest.update(path.as_str());
    }
    for file in files.values() {
        digest.update([1]);
        digest.update((file.path.as_str().len() as u64).to_le_bytes());
        digest.update(file.path.as_str());
        digest.update(file.bytes.to_le_bytes());
        digest.update(&file.sha256);
    }
    hex(&digest.finalize())
}

/// Render fingerprints with their canonical machine spelling.
pub(super) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut value, byte| {
            let _ = write!(value, "{byte:02x}");
            value
        })
}

/// Keep filesystem errors structured and stable across locales.
pub(super) fn io(error: &std::io::Error) -> Error {
    Error::root(match error.kind() {
        std::io::ErrorKind::NotFound => "source_file_missing",
        std::io::ErrorKind::AlreadyExists => "source_destination_exists",
        std::io::ErrorKind::PermissionDenied => "source_permission_denied",
        _ => "source_io_failed",
    })
}
