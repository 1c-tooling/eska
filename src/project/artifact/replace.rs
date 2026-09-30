//! Preflight and reversible source-directory replacement for one selected project.

use std::{fs, path::PathBuf};

use gix::bstr::ByteSlice;

use super::{
    ArtifactError, Identity, PreparedArtifact, inspect_identity, io_error,
    snapshot::{self, Snapshot},
    staging::Staging,
};
use crate::{
    config::ProjectConfig,
    project::Project,
    vcs::repository::{Error as RepositoryError, Repository},
};

/// The original source state and identity observed before unpacking or asking the user.
pub struct ImportPlan {
    project: Project,
    config: Vec<u8>,
    snapshot: Snapshot,
    current: Option<Identity>,
    local_changes: usize,
}

/// Successful publication may retain an old tree only when cleanup failed.
pub struct ImportResult {
    pub retained_backup: Option<PathBuf>,
}

impl ImportPlan {
    /// Validate the entire replacement boundary without changing project or Git files.
    ///
    /// # Errors
    /// Rejects root-level sources, symlinks, nested projects and unreadable source/Git state.
    pub fn inspect(project: &Project) -> Result<Self, ArtifactError> {
        validate_location(project)?;
        let config_path = project.root().join(crate::config::FILE_NAME);
        let config = fs::read(&config_path).map_err(|error| io_error(&config_path, error))?;
        let snapshot = snapshot::capture(project.source())?;
        let current = inspect_identity(project.source())?;
        if current.is_none() && !snapshot.empty_scaffold {
            return Err(ArtifactError::MissingDescriptor(
                project.source().to_owned(),
            ));
        }
        if let Some(identity) = &current {
            ensure_type(project, identity)?;
        }
        let local_changes = local_changes(project, &snapshot)?;
        Ok(Self {
            project: project.clone(),
            config,
            snapshot,
            current,
            local_changes,
        })
    }

    /// Return the old native identity, absent only for an empty scaffold.
    #[must_use]
    pub const fn current(&self) -> Option<&Identity> {
        self.current.as_ref()
    }

    /// Return the number of changed source paths; Git-less populated sources require confirmation.
    #[must_use]
    pub const fn local_changes(&self) -> usize {
        self.local_changes
    }

    /// Return the full number of source files that will be replaced or removed.
    #[must_use]
    pub const fn files(&self) -> usize {
        self.snapshot.files
    }

    /// Determine whether publication needs explicit consent, never permitting a type change.
    ///
    /// # Errors
    /// A different project type is always rejected, including forced operations.
    pub fn requires_confirmation(&self, incoming: &Identity) -> Result<bool, ArtifactError> {
        ensure_type(&self.project, incoming)?;
        Ok(self.local_changes != 0
            || self
                .current
                .as_ref()
                .is_some_and(|current| current != incoming))
    }

    /// Replace the source directory after rechecking the approved state; preserve Git and config.
    ///
    /// # Errors
    /// Refuses stale plans and unconfirmed warnings. On publication failure restores the old
    /// tree; if restoration itself fails, reports and retains the backup for recovery.
    pub fn apply(
        &self,
        artifact: &PreparedArtifact,
        confirmed: bool,
    ) -> Result<ImportResult, ArtifactError> {
        if self.requires_confirmation(artifact.identity())? && !confirmed {
            return Err(ArtifactError::ConfirmationRequired);
        }
        validate_location(&self.project)?;
        let config_path = self.project.root().join(crate::config::FILE_NAME);
        if fs::read(&config_path).map_err(|error| io_error(&config_path, error))? != self.config
            || snapshot::capture(self.project.source())? != self.snapshot
            || local_changes(&self.project, &self.snapshot)? != self.local_changes
        {
            return Err(ArtifactError::SourceChanged(
                self.project.source().to_owned(),
            ));
        }
        snapshot::capture(&artifact.sources())?;
        if inspect_identity(&artifact.sources())?.as_ref() != Some(artifact.identity()) {
            return Err(ArtifactError::SourceChanged(artifact.sources()));
        }
        replace_directory(&artifact.sources(), self.project.source())
    }
}

/// Restore the previous source tree if publishing the prepared directory fails.
fn replace_directory(
    sources: &std::path::Path,
    destination: &std::path::Path,
) -> Result<ImportResult, ArtifactError> {
    let mut backup = Staging::create(destination)?;
    let previous = backup.path.join("previous");
    fs::rename(destination, &previous).map_err(|error| io_error(destination, error))?;
    // From this point, automatic cleanup must never discard the user's only surviving tree.
    backup.preserve = true;
    if let Err(original) = fs::rename(sources, destination) {
        if let Err(restore) = fs::rename(&previous, destination) {
            return Err(ArtifactError::Rollback {
                backup: previous,
                original,
                restore,
            });
        }
        backup.preserve = false;
        return Err(io_error(destination, original));
    }
    let retained_backup = fs::remove_dir_all(&backup.path)
        .err()
        .map(|_| backup.path.clone());
    Ok(ImportResult { retained_backup })
}

/// Compare metadata types before offering any destructive override.
fn ensure_type(project: &Project, identity: &Identity) -> Result<(), ArtifactError> {
    let expected = project.configuration().project_type();
    if expected == identity.project_type {
        Ok(())
    } else {
        Err(ArtifactError::TypeMismatch {
            expected,
            actual: identity.project_type,
        })
    }
}

/// Check raw configured components because discovery canonicalizes and erases symlink paths.
fn validate_location(project: &Project) -> Result<(), ArtifactError> {
    let config = ProjectConfig::load(&project.root().join(crate::config::FILE_NAME))
        .map_err(ArtifactError::Config)?;
    let mut path = project.root().to_owned();
    for component in config.source().components() {
        path.push(component);
        if !fs::symlink_metadata(&path)
            .map_err(|error| io_error(&path, error))?
            .is_dir()
        {
            return Err(ArtifactError::UnsafePath(path));
        }
    }
    if path == project.root()
        || fs::canonicalize(&path).map_err(|error| io_error(&path, error))? != project.source()
    {
        return Err(ArtifactError::UnsafePath(path));
    }
    Ok(())
}

/// Count only changed paths inside this source tree, preserving unrelated workspace edits.
fn local_changes(project: &Project, snapshot: &Snapshot) -> Result<usize, ArtifactError> {
    let repository = match Repository::discover(project.root()) {
        Ok(repository) => repository,
        Err(RepositoryError::NotFound { .. }) => {
            return Ok(if snapshot.empty_scaffold {
                0
            } else {
                snapshot.files
            });
        }
        Err(error) => return Err(ArtifactError::Repository(error)),
    };
    let status = repository.status().map_err(ArtifactError::Repository)?;
    Ok(status
        .entries
        .iter()
        .filter(|entry| {
            repository
                .work_dir()
                .join(gix::path::from_bstr(entry.path.as_bstr()).as_ref())
                .starts_with(project.source())
                && !(snapshot.empty_scaffold && entry.path.ends_with(b"/.gitkeep"))
        })
        .count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    /// A failed directory publication restores the complete previous tree before returning.
    #[test]
    fn publication_failure_restores_previous_source_bytes() {
        let fixture = TestDir::new();
        let source = fixture.0.join("src");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/module.bsl"), b"\xef\xbb\xbfkept\r\n").unwrap();
        let result = replace_directory(&fixture.0.join("missing-candidate"), &source);
        assert!(matches!(result, Err(ArtifactError::Io { .. })));
        assert_eq!(
            fs::read(source.join("nested/module.bsl")).unwrap(),
            b"\xef\xbb\xbfkept\r\n"
        );
        assert_eq!(
            fs::read_dir(fixture.0.join(".eska/import"))
                .unwrap()
                .count(),
            1
        );
    }
}
