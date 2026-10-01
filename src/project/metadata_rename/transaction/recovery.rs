//! Recovery derives completed actions from exact bytes and path presence, never from a guessed step number.

use super::super::inventory::read_text;
use super::{Guard, Journal, RecoveryStatus, cleanup_preparation, io, move_path};
use crate::project::metadata_edit::{EditError, publish_snapshot_in};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

impl Guard {
    /// Inspect interrupted publication without changing source files or discarding a journal.
    pub fn recovery_status(&self, source: &Path) -> Result<RecoveryStatus, EditError> {
        let directory = io::path(&self.directory, Path::new("rename"))?;
        if !directory.try_exists().map_err(EditError::Io)? {
            return Ok(RecoveryStatus {
                pending: false,
                snapshot: None,
                committed: false,
                restored: false,
                files: Vec::new(),
                conflicts: Vec::new(),
            });
        }
        let token = io::tree_hash(&self.directory, Path::new("rename"), true)?;
        let finish_path = io::path(&directory, Path::new("finished"))?;
        let (committed, restored) = if finish_path.try_exists().map_err(EditError::Io)? {
            match read_text(&finish_path, 32).map_err(EditError::Io)?.as_str() {
                "committed" => (true, false),
                "rolled_back" => (false, true),
                _ => return Err(EditError::RecoveryRequired),
            }
        } else {
            (false, false)
        };
        let journal = load(source, &directory)?;
        let (files, conflicts) = if let Some(journal) = journal {
            let mut files: BTreeSet<_> =
                journal.files.iter().map(|file| file.path.clone()).collect();
            for movement in &journal.moves {
                files.insert(movement.movement.from.clone());
                files.insert(movement.movement.to.clone());
            }
            let conflicts = if committed || restored {
                Vec::new()
            } else {
                journal.conflicts(&directory)?
            };
            (files.into_iter().collect(), conflicts)
        } else {
            (Vec::new(), Vec::new())
        };
        Ok(RecoveryStatus {
            pending: true,
            snapshot: Some(token),
            committed,
            restored,
            files,
            conflicts,
        })
    }

    /// Restore a reviewed interrupted journal; an already committed journal only needs cleanup.
    pub fn recover(&self, source: &Path, expected: &str) -> Result<(), EditError> {
        let status = self.recovery_status(source)?;
        if status.snapshot.as_deref() != Some(expected) || !status.conflicts.is_empty() {
            return Err(EditError::Conflict);
        }
        let directory = io::path(&self.directory, Path::new("rename"))?;
        if let Some(journal) = load(source, &directory)? {
            if !status.committed && !status.restored {
                Self::rollback(&journal, &directory)?;
                super::finish(&directory, b"rolled_back")?;
            }
            Self::cleanup(&journal, &directory)
        } else {
            cleanup_preparation(&directory)
        }
    }

    /// Preflight all bytes before restoring any path; later races retain the journal for another review.
    pub(super) fn rollback(journal: &Journal, directory: &Path) -> Result<(), EditError> {
        if !journal.conflicts(directory)?.is_empty() {
            return Err(EditError::Conflict);
        }
        remove(&io::path(directory, Path::new("publish.tmp"))?)?;
        for stored in journal.moves.iter().rev() {
            if io::path(&journal.source, &stored.movement.to)?
                .try_exists()
                .map_err(EditError::Io)?
            {
                move_path(&journal.source, stored, true)?;
            }
        }
        for (index, file) in journal.files.iter().enumerate().rev() {
            let actual = io::file_digest(&io::path(&journal.source, &file.path)?)?;
            if actual == file.before {
                continue;
            }
            let (before, after) = journal.data(directory, index)?;
            publish_snapshot_in(&journal.source, &file.path, &after, &before, directory)?;
        }
        Ok(())
    }

    /// Remove only declared journal artifacts. The source directory is never a cleanup target.
    pub(super) fn cleanup(journal: &Journal, directory: &Path) -> Result<(), EditError> {
        for index in 0..journal.files.len() {
            for suffix in ["before", "after"] {
                remove(&io::path(
                    directory,
                    Path::new(&format!("{index}.{suffix}")),
                )?)?;
            }
        }
        remove(&io::path(directory, Path::new("ready"))?)?;
        remove(&io::path(directory, Path::new("journal.json"))?)?;
        remove(&io::path(directory, Path::new("finish.tmp"))?)?;
        remove(&io::path(directory, Path::new("publish.tmp"))?)?;
        remove(&io::path(directory, Path::new("finished"))?)?;
        fs::remove_dir(directory).map_err(EditError::Io)
    }
}

impl Journal {
    /// Resolve each stored source path against moves which actually reached the filesystem.
    fn current_path(&self, relative: &Path) -> Result<PathBuf, EditError> {
        for stored in &self.moves {
            let movement = &stored.movement;
            if relative == movement.from
                || movement.directory && relative.starts_with(&movement.from)
            {
                let from = io::path(&self.source, &movement.from)?
                    .try_exists()
                    .map_err(EditError::Io)?;
                let to = io::path(&self.source, &movement.to)?
                    .try_exists()
                    .map_err(EditError::Io)?;
                if from == to {
                    return Err(EditError::Conflict);
                }
                return if to {
                    Ok(movement.to.join(
                        relative
                            .strip_prefix(&movement.from)
                            .map_err(|_| EditError::UnsafePath)?,
                    ))
                } else {
                    Ok(relative.to_path_buf())
                };
            }
        }
        Ok(relative.to_path_buf())
    }

    /// A foreign edit, duplicate destination or changed moved payload is visible before rollback starts.
    fn conflicts(&self, directory: &Path) -> Result<Vec<PathBuf>, EditError> {
        let mut conflicts = BTreeSet::new();
        for stored in &self.moves {
            let movement = &stored.movement;
            let from = io::path(&self.source, &movement.from)?
                .try_exists()
                .map_err(EditError::Io)?;
            let to = io::path(&self.source, &movement.to)?
                .try_exists()
                .map_err(EditError::Io)?;
            if from == to
                || to
                    && io::tree_hash(&self.source, &movement.to, movement.directory)?
                        != stored.after
            {
                conflicts.insert(movement.from.clone());
                conflicts.insert(movement.to.clone());
            }
        }
        for (index, file) in self.files.iter().enumerate() {
            self.data(directory, index)?;
            let actual = self
                .current_path(&file.path)
                .and_then(|relative| io::file_digest(&io::path(&self.source, &relative)?));
            if !actual.is_ok_and(|hash| hash == file.before || hash == file.after) {
                conflicts.insert(file.path.clone());
            }
        }
        Ok(conflicts.into_iter().collect())
    }
}

/// Bound persisted input and reject paths/overlaps before accepting it as recovery authority.
fn load(source: &Path, directory: &Path) -> Result<Option<Journal>, EditError> {
    let ready = io::path(directory, Path::new("ready"))?;
    if !ready.try_exists().map_err(EditError::Io)? {
        return Ok(None);
    }
    let path = io::path(directory, Path::new("journal.json"))?;
    let input = read_text(&path, super::MAX_JOURNAL_BYTES).map_err(EditError::Io)?;
    let journal: Journal = serde_json::from_str(&input).map_err(|_| EditError::InvalidValue)?;
    if journal.schema_version != 1
        || journal.source != source.canonicalize().map_err(EditError::Io)?
    {
        return Err(EditError::UnsafePath);
    }
    let mut paths = BTreeSet::new();
    for file in &journal.files {
        validate_relative(&file.path)?;
        if !paths.insert(&file.path) {
            return Err(EditError::UnsafePath);
        }
    }
    let mut moves: Vec<&Path> = Vec::new();
    for stored in &journal.moves {
        for path in [&stored.movement.from, &stored.movement.to] {
            validate_relative(path)?;
            if moves
                .iter()
                .any(|other| path.starts_with(other) || other.starts_with(path))
            {
                return Err(EditError::UnsafePath);
            }
            moves.push(path);
        }
    }
    Ok(Some(journal))
}

/// Recovery can only touch source-relative files, excluding the service area which owns its authority.
fn validate_relative(path: &Path) -> Result<(), EditError> {
    if path.as_os_str().is_empty()
        || path.starts_with(".eska")
        || path.starts_with(".git")
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(EditError::UnsafePath);
    }
    Ok(())
}

/// Repeated cleanup after interruption is safe, but aliases are checked by the caller first.
fn remove(path: &Path) -> Result<(), EditError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(EditError::Io(error)),
    }
}
