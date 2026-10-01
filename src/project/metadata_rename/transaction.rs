//! A rename journal owns synced staged bytes; publication and recovery use exact-source comparisons.

mod io;
mod recovery;
#[cfg(test)]
mod tests;

use super::{
    RenameMove, RenamePlan,
    inventory::{MAX_XML_BYTES, read_text},
};

const MAX_JOURNAL_BYTES: u64 = 16 * 1024 * 1024;
use crate::project::metadata_edit::{EditError, publish_snapshot_in, snapshot};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
};

/// All metadata writers in one project share this OS lock; an external editor does not participate.
pub struct Guard {
    lock: File,
    directory: PathBuf,
}

impl Drop for Guard {
    /// Release ownership explicitly: a concurrent fork may temporarily inherit an open file description.
    fn drop(&mut self) {
        let _ = self.lock.unlock();
    }
}

/// A prepared journal is private backend data, never a client-supplied list of paths or replacements.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    source: PathBuf,
    files: Vec<StoredFile>,
    moves: Vec<StoredMove>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredFile {
    path: PathBuf,
    before: String,
    after: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredMove {
    movement: RenameMove,
    after: String,
}

/// Explicit recovery binds its token to journal contents and reports conflicts in affected sources.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryStatus {
    pub pending: bool,
    pub snapshot: Option<String>,
    pub committed: bool,
    pub restored: bool,
    pub files: Vec<PathBuf>,
    pub conflicts: Vec<PathBuf>,
}

impl Guard {
    /// Serialize metadata writes without waiting or accepting aliases in service paths.
    pub fn acquire(project: &Path) -> Result<Self, EditError> {
        let service = io::directory(project, Path::new(".eska"))?;
        let directory = io::directory(&service, Path::new("metadata"))?;
        let path = io::path(&directory, Path::new("write.lock"))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(EditError::Io)?;
        lock.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => EditError::Busy,
            TryLockError::Error(error) => EditError::Io(error),
        })?;
        Ok(Self { lock, directory })
    }

    /// A pending journal prevents scalar writes from invalidating recoverable rename bytes.
    pub fn check_clear(&self, source: &Path) -> Result<(), EditError> {
        let path = io::path(&self.directory, Path::new("rename"))?;
        if path.try_exists().map_err(EditError::Io)? {
            let status = self.recovery_status(source)?;
            if status.committed || status.restored {
                self.recover(
                    source,
                    status
                        .snapshot
                        .as_deref()
                        .ok_or(EditError::RecoveryRequired)?,
                )?;
            } else {
                return Err(EditError::RecoveryRequired);
            }
        }
        Ok(())
    }

    /// Stage and sync every original/candidate before the first source file is replaced.
    pub fn publish(
        &self,
        source: &Path,
        plan: &RenamePlan,
        excluded: &[PathBuf],
    ) -> Result<(), EditError> {
        self.check_clear(source)?;
        let directory = io::path(&self.directory, Path::new("rename"))?;
        fs::create_dir(&directory).map_err(EditError::Io)?;
        let prepared = Self::prepare(source, plan, &directory);
        let journal = match prepared {
            Ok(journal) => journal,
            Err(error) => {
                // Preparation has no source writes. Unknown extra files still prevent cleanup.
                let _ = cleanup_preparation(&directory);
                return Err(error);
            }
        };
        let current =
            super::inventory::snapshot(source, excluded, plan.object_id.as_str(), &plan.new_name)
                .map_err(EditError::Io);
        match current {
            Ok(current) if current == plan.snapshot => Self::commit(&journal, &directory),
            result => {
                finish(&directory, b"rolled_back").map_err(|_| EditError::RecoveryRequired)?;
                Self::cleanup(&journal, &directory)?;
                Err(result.err().unwrap_or(EditError::Conflict))
            }
        }
    }

    /// A normal write failure rolls back already published members; failed restoration retains the journal.
    fn commit(journal: &Journal, directory: &Path) -> Result<(), EditError> {
        let result =
            Self::publish_steps(journal, directory).and_then(|()| finish(directory, b"committed"));
        if let Err(error) = result {
            if Self::rollback(journal, directory).is_err() {
                return Err(EditError::RecoveryRequired);
            }
            finish(directory, b"rolled_back").map_err(|_| EditError::RecoveryRequired)?;
            Self::cleanup(journal, directory)?;
            return Err(error);
        }
        // A committed marker preserves the actual result if cleanup is interrupted or unavailable.
        let _ = Self::cleanup(journal, directory);
        Ok(())
    }

    /// Plans are recomputed by the workspace; source hashes and exact ranges are checked again here.
    fn prepare(source: &Path, plan: &RenamePlan, directory: &Path) -> Result<Journal, EditError> {
        let canonical = source.canonicalize().map_err(EditError::Io)?;
        if canonical != source {
            return Err(EditError::UnsafePath);
        }
        let source = canonical;
        let mut journal = Journal {
            schema_version: 1,
            source,
            files: Vec::new(),
            moves: Vec::new(),
        };
        let mut staged = BTreeMap::new();
        for file in plan
            .files
            .iter()
            .filter(|file| !file.replacements.is_empty())
        {
            let path = io::path(&journal.source, &file.path)?;
            if fs::metadata(&path)
                .map_err(EditError::Io)?
                .permissions()
                .readonly()
            {
                return Err(EditError::ReadOnly);
            }
            let before = read_text(&path, MAX_XML_BYTES).map_err(EditError::Io)?;
            if snapshot(&before) != file.snapshot {
                return Err(EditError::Conflict);
            }
            let mut end = 0;
            for replacement in &file.replacements {
                if replacement.range.start < end
                    || before.get(replacement.range.clone()) != Some(&replacement.before)
                {
                    return Err(EditError::Conflict);
                }
                end = replacement.range.end;
            }
            let mut after = before.clone();
            for replacement in file.replacements.iter().rev() {
                after.replace_range(replacement.range.clone(), &replacement.after);
            }
            let index = journal.files.len();
            io::create(
                &directory.join(format!("{index}.before")),
                before.as_bytes(),
            )?;
            io::create(&directory.join(format!("{index}.after")), after.as_bytes())?;
            let after = snapshot(&after);
            staged.insert(file.path.clone(), after.clone());
            journal.files.push(StoredFile {
                path: file.path.clone(),
                before: file.snapshot.clone(),
                after,
            });
        }
        for movement in &plan.moves {
            let destination = io::path(&journal.source, &movement.to)?;
            if destination.try_exists().map_err(EditError::Io)? {
                return Err(EditError::Conflict);
            }
            journal.moves.push(StoredMove {
                movement: movement.clone(),
                after: io::tree_hash_with(
                    &journal.source,
                    &movement.from,
                    movement.directory,
                    &staged,
                )?,
            });
        }
        let bytes = serde_json::to_vec(&journal).map_err(|_| EditError::InvalidValue)?;
        if bytes.len() as u64 > MAX_JOURNAL_BYTES {
            return Err(EditError::UnsupportedValue);
        }
        io::create(&directory.join("journal.json"), &bytes)?;
        io::create(&directory.join("ready"), b"1")?;
        Ok(journal)
    }

    /// Publish file replacements first, then move the complete verified payloads.
    fn publish_steps(journal: &Journal, directory: &Path) -> Result<(), EditError> {
        for (index, file) in journal.files.iter().enumerate() {
            journal.publish_file(directory, index, &file.path)?;
        }
        for movement in &journal.moves {
            move_path(&journal.source, movement, false)?;
        }
        Ok(())
    }
}

impl Journal {
    /// Reuse the same exact-byte writer for each prepared member and interruption regression tests.
    fn publish_file(
        &self,
        directory: &Path,
        index: usize,
        relative: &Path,
    ) -> Result<(), EditError> {
        let (before, after) = self.data(directory, index)?;
        publish_snapshot_in(&self.source, relative, &before, &after, directory)
    }
    /// Journal payload corruption can never turn recovery into an arbitrary file replacement.
    fn data(&self, directory: &Path, index: usize) -> Result<(String, String), EditError> {
        let file = self.files.get(index).ok_or(EditError::InvalidValue)?;
        let before = read_text(
            &io::path(directory, Path::new(&format!("{index}.before")))?,
            MAX_XML_BYTES,
        )
        .map_err(EditError::Io)?;
        let after = read_text(
            &io::path(directory, Path::new(&format!("{index}.after")))?,
            MAX_XML_BYTES,
        )
        .map_err(EditError::Io)?;
        if snapshot(&before) != file.before || snapshot(&after) != file.after {
            return Err(EditError::Conflict);
        }
        Ok((before, after))
    }
}

/// Recheck both physical names immediately before moving; no delete-then-rename fallback exists.
fn move_path(root: &Path, stored: &StoredMove, reverse: bool) -> Result<(), EditError> {
    let movement = &stored.movement;
    let (from, to) = if reverse {
        (&movement.to, &movement.from)
    } else {
        (&movement.from, &movement.to)
    };
    let source = io::path(root, from)?;
    let target = io::path(root, to)?;
    if target.try_exists().map_err(EditError::Io)?
        || io::tree_hash(root, from, movement.directory)? != stored.after
    {
        return Err(EditError::Conflict);
    }
    fs::rename(source, target).map_err(EditError::Io)
}

/// Without a journal no source action has started; remove only the staged files this format owns.
fn cleanup_preparation(directory: &Path) -> Result<(), EditError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory).map_err(EditError::Io)? {
        let entry = entry.map_err(EditError::Io)?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(EditError::UnsafePath)?;
        let owned = matches!(
            name,
            "journal.json" | "ready" | "finished" | "finish.tmp" | "publish.tmp"
        ) || name.split_once('.').is_some_and(|(index, suffix)| {
            index.parse::<usize>().is_ok() && matches!(suffix, "before" | "after")
        });
        if !owned || !entry.file_type().map_err(EditError::Io)?.is_file() {
            return Err(EditError::UnsafePath);
        }
        files.push(entry.path());
    }
    // Removing ready first makes every later cleanup interruption explicitly non-publishing.
    files.sort_by_key(|path| {
        (
            path.file_name().is_none_or(|name| name != "ready"),
            path.clone(),
        )
    });
    for path in files {
        fs::remove_file(path).map_err(EditError::Io)?;
    }
    fs::remove_dir(directory).map_err(EditError::Io)
}

/// Publish the terminal marker atomically, so a killed process never exposes a partial decision.
fn finish(directory: &Path, state: &[u8]) -> Result<(), EditError> {
    let temporary = io::path(directory, Path::new("finish.tmp"))?;
    if temporary.try_exists().map_err(EditError::Io)? {
        fs::remove_file(&temporary).map_err(EditError::Io)?;
    }
    io::create(&temporary, state)?;
    let target = io::path(directory, Path::new("finished"))?;
    if target.try_exists().map_err(EditError::Io)? {
        return Err(EditError::RecoveryRequired);
    }
    fs::rename(temporary, target).map_err(EditError::Io)
}
