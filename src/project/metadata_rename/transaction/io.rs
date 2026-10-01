//! Filesystem checks shared by preparation, publication and explicit recovery.

use super::super::inventory::{Inventory, file_hash};
use crate::project::metadata_edit::EditError;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write as _};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

/// Reject aliases and special entries at every component, including a not-yet-created final path.
pub(super) fn path(root: &Path, relative: &Path) -> Result<PathBuf, EditError> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(EditError::UnsafePath);
    }
    let mut current = root.canonicalize().map_err(EditError::Io)?;
    if current != root {
        return Err(EditError::UnsafePath);
    }
    let count = relative.components().count();
    for (index, part) in relative.components().enumerate() {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata)
                if metadata.file_type().is_symlink()
                    || !(metadata.is_file() || metadata.is_dir()) =>
            {
                return Err(EditError::UnsafePath);
            }
            Ok(metadata) if index + 1 != count && !metadata.is_dir() => {
                return Err(EditError::UnsafePath);
            }
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound && index + 1 == count => (),
            Err(error) => return Err(EditError::Io(error)),
        }
    }
    Ok(current)
}

/// Journal directories are created one level at a time and never through an alias.
pub(super) fn directory(root: &Path, relative: &Path) -> Result<PathBuf, EditError> {
    let path = path(root, relative)?;
    match fs::create_dir(&path) {
        Ok(()) => (),
        Err(error)
            if error.kind() == io::ErrorKind::AlreadyExists
                && fs::symlink_metadata(&path).is_ok_and(|metadata| {
                    metadata.is_dir() && !metadata.file_type().is_symlink()
                }) => {}
        Err(error) => return Err(EditError::Io(error)),
    }
    Ok(path)
}

/// Persist owned journal entries exclusively; an existing file can never be silently replaced.
pub(super) fn create(path: &Path, bytes: &[u8]) -> Result<(), EditError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(EditError::Io)?;
    file.write_all(bytes).map_err(EditError::Io)?;
    file.sync_all().map_err(EditError::Io)
}

/// A payload digest includes exact file names, bytes and empty directories, without following links.
pub(super) fn tree_hash(
    root: &Path,
    relative: &Path,
    directory: bool,
) -> Result<String, EditError> {
    tree_hash_with(root, relative, directory, &BTreeMap::new())
}

/// During preparation, staged file hashes describe the future payload without modifying its source.
pub(super) fn tree_hash_with(
    root: &Path,
    relative: &Path,
    directory: bool,
    staged: &BTreeMap<PathBuf, String>,
) -> Result<String, EditError> {
    let path = path(root, relative)?;
    let metadata = fs::symlink_metadata(&path).map_err(EditError::Io)?;
    if metadata.is_dir() != directory {
        return Err(EditError::Conflict);
    }
    if !directory {
        return staged
            .get(relative)
            .cloned()
            .map_or_else(|| file_digest(&path), Ok);
    }
    let inventory = Inventory::read(&path, &[]).map_err(EditError::Io)?;
    let mut digest = Sha256::new();
    for file in inventory.files {
        digest.update([0]);
        digest.update((file.as_os_str().as_encoded_bytes().len() as u64).to_le_bytes());
        digest.update(file.as_os_str().as_encoded_bytes());
        digest.update(
            staged
                .get(&relative.join(&file))
                .cloned()
                .map_or_else(|| file_digest(&path.join(file)), Ok)?,
        );
    }
    for directory in inventory.directories {
        digest.update([1]);
        digest.update((directory.as_os_str().as_encoded_bytes().len() as u64).to_le_bytes());
        digest.update(directory.as_os_str().as_encoded_bytes());
    }
    Ok(hex(&digest.finalize()))
}

/// Use the same fixed-width SHA-256 encoding as property and source snapshots.
pub(super) fn file_digest(path: &Path) -> Result<String, EditError> {
    Ok(hex(&file_hash(path).map_err(EditError::Io)?))
}

/// Hash values are ASCII and locale independent.
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}
