//! Containment and exclusive parent creation for a module migration.

use super::{PublishError, VersionError, io_error, publication};
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

/// No existing component may be a symlink or redirect the selected native source.
pub(super) fn existing(source: &Path, path: &Path) -> Result<Option<fs::Metadata>, VersionError> {
    if source
        .canonicalize()
        .map_err(|error| io_error(source, error))?
        != source
    {
        return Err(publication(source, PublishError::UnsafePath));
    }
    let relative = path
        .strip_prefix(source)
        .map_err(|_| publication(path, PublishError::UnsafePath))?;
    let mut current = source.to_owned();
    let mut found = None;
    for part in relative.components() {
        if !matches!(part, Component::Normal(_)) {
            return Err(publication(path, PublishError::UnsafePath));
        }
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if !metadata.file_type().is_symlink() => found = Some(metadata),
            Ok(_) => return Err(publication(&current, PublishError::UnsafePath)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(&current, error)),
        }
    }
    Ok(found)
}

/// On failure remove only this operation's newly created empty directories, in reverse order.
pub(super) struct CreatedParents(Vec<PathBuf>);

impl Drop for CreatedParents {
    /// A published module or concurrent child makes removal fail without removing that child.
    fn drop(&mut self) {
        for directory in self.0.iter().rev() {
            let _ = fs::remove_dir(directory);
        }
    }
}

/// Prepare only the selected module's missing parents; no existing path is replaced.
pub(super) fn create_parents(source: &Path, path: &Path) -> Result<CreatedParents, VersionError> {
    let parent = path
        .parent()
        .ok_or_else(|| publication(path, PublishError::UnsafePath))?;
    let relative = parent
        .strip_prefix(source)
        .map_err(|_| publication(path, PublishError::UnsafePath))?;
    let mut current = source.to_owned();
    let mut created = CreatedParents(Vec::new());
    for part in relative.components() {
        current.push(part);
        match existing(source, &current)? {
            Some(metadata) if metadata.is_dir() => (),
            Some(_) => return Err(publication(&current, PublishError::UnsafePath)),
            None => {
                fs::create_dir(&current).map_err(|error| io_error(&current, error))?;
                created.0.push(current.clone());
            }
        }
    }
    Ok(created)
}
