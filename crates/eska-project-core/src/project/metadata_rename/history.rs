//! Exact inverse replacements keep undo independent of later reference analysis.

use super::{RenameFile, RenameMove, RenamePlan, RenameReplacement};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Retain only the backend's confirmed replacements and source snapshots in session history.
#[derive(Clone, Debug)]
pub struct SavedRename {
    pub forward: RenamePlan,
    pub reverse: RenamePlan,
}

impl SavedRename {
    /// Uncertain occurrences have no replay authority and need not accumulate in memory.
    pub fn new(plan: &RenamePlan, reverse: RenamePlan) -> Self {
        let mut forward = plan.clone();
        forward.files.retain(|file| !file.replacements.is_empty());
        for file in &mut forward.files {
            file.uncertain.clear();
        }
        Self { forward, reverse }
    }

    /// Include path and operation overhead in the same history budget as scalar edits.
    pub fn bytes(&self) -> usize {
        [&self.forward, &self.reverse]
            .into_iter()
            .map(|plan| {
                std::mem::size_of::<RenamePlan>()
                    + plan
                        .moves
                        .iter()
                        .map(|movement| {
                            movement.from.as_os_str().len()
                                + movement.to.as_os_str().len()
                                + std::mem::size_of::<RenameMove>()
                        })
                        .sum::<usize>()
                    + plan
                        .files
                        .iter()
                        .map(|file| {
                            std::mem::size_of::<RenameFile>()
                                + file.path.as_os_str().len()
                                + file.snapshot.len()
                                + file
                                    .replacements
                                    .iter()
                                    .map(|change| {
                                        std::mem::size_of::<RenameReplacement>()
                                            + change.before.len()
                                            + change.after.len()
                                    })
                                    .sum::<usize>()
                        })
                        .sum::<usize>()
            })
            .sum()
    }
}

/// Map a source path through disjoint descriptor/payload moves without inventing missing directories.
pub fn moved_path(path: &Path, moves: &[RenameMove]) -> PathBuf {
    for movement in moves {
        if path == movement.from {
            return movement.to.clone();
        }
        if movement.directory
            && let Ok(suffix) = path.strip_prefix(&movement.from)
        {
            return movement.to.join(suffix);
        }
    }
    path.to_path_buf()
}

/// Reverse UTF-8 ranges using accumulated byte deltas, preserving all untouched text exactly.
pub fn reversed(
    plan: &RenamePlan,
    hashes: &BTreeMap<PathBuf, String>,
) -> Result<RenamePlan, crate::project::metadata_edit::EditError> {
    use crate::project::metadata_edit::EditError;
    let files = plan
        .files
        .iter()
        .filter(|file| !file.replacements.is_empty())
        .map(|file| {
            let mut removed = 0;
            let mut inserted = 0;
            let replacements = file
                .replacements
                .iter()
                .map(|change| {
                    let start = change.range.start - removed + inserted;
                    removed += change.range.len();
                    inserted += change.after.len();
                    RenameReplacement {
                        range: start..start + change.after.len(),
                        before: change.after.clone(),
                        after: change.before.clone(),
                    }
                })
                .collect();
            Ok(RenameFile {
                path: moved_path(&file.path, &plan.moves),
                snapshot: hashes.get(&file.path).ok_or(EditError::Conflict)?.clone(),
                replacements,
                uncertain: Vec::new(),
            })
        })
        .collect::<Result<_, EditError>>()?;
    Ok(RenamePlan {
        object_id: plan.new_object_id.clone(),
        new_object_id: plan.object_id.clone(),
        uuid: plan.uuid.clone(),
        old_name: plan.new_name.clone(),
        new_name: plan.old_name.clone(),
        snapshot: String::new(),
        files,
        moves: plan
            .moves
            .iter()
            .rev()
            .map(|movement| RenameMove {
                from: movement.to.clone(),
                to: movement.from.clone(),
                directory: movement.directory,
            })
            .collect(),
        issues: Vec::new(),
    })
}
