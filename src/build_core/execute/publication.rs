//! Publish one artifact or its manifest pair with backups for ordinary I/O failures.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::{BuildError, workspace::unique_sibling};

/// Replace an existing regular artifact while restoring it if publication fails.
pub(super) fn publish(artifact: &Path, output: &Path) -> Result<(), BuildError> {
    if !output.exists() {
        fs::hard_link(artifact, output).map_err(|source| BuildError::Publish {
            path: output.to_owned(),
            source,
        })?;
        return fs::remove_file(artifact).map_err(|source| BuildError::Publish {
            path: artifact.to_owned(),
            source,
        });
    }
    let backup = create_backup_link(output)?;
    if let Err(source) = fs::remove_file(output) {
        let _ = fs::remove_file(&backup);
        return Err(BuildError::Publish {
            path: output.to_owned(),
            source,
        });
    }
    if let Err(source) = fs::hard_link(artifact, output) {
        restore_backup(&backup, output)?;
        return Err(BuildError::Publish {
            path: output.to_owned(),
            source,
        });
    }
    fs::remove_file(artifact).map_err(|source| BuildError::Publish {
        path: artifact.to_owned(),
        source,
    })?;
    fs::remove_file(&backup).map_err(|source| BuildError::Publish {
        path: backup,
        source,
    })
}

/// Publish an artifact and manifest together, restoring the previous pair on failure.
pub(super) fn publish_pair(
    artifact: &Path,
    output: &Path,
    staged_manifest: &Path,
    manifest: &Path,
) -> Result<(), BuildError> {
    let output_backup = optional_backup(output)?;
    let manifest_backup = match optional_backup(manifest) {
        Ok(backup) => backup,
        Err(error) => {
            remove_backup(output_backup.as_deref());
            return Err(error);
        }
    };
    if let Err(error) = remove_existing(output).and_then(|()| remove_existing(manifest)) {
        restore_pair(
            output,
            output_backup.as_deref(),
            manifest,
            manifest_backup.as_deref(),
        )?;
        return Err(error);
    }
    if let Err(source) = fs::hard_link(artifact, output) {
        restore_pair(
            output,
            output_backup.as_deref(),
            manifest,
            manifest_backup.as_deref(),
        )?;
        return Err(BuildError::Publish {
            path: output.to_owned(),
            source,
        });
    }
    if let Err(source) = fs::hard_link(staged_manifest, manifest) {
        restore_pair(
            output,
            output_backup.as_deref(),
            manifest,
            manifest_backup.as_deref(),
        )?;
        return Err(BuildError::Publish {
            path: manifest.to_owned(),
            source,
        });
    }
    let _ = fs::remove_file(artifact);
    let _ = fs::remove_file(staged_manifest);
    remove_backup(output_backup.as_deref());
    remove_backup(manifest_backup.as_deref());
    Ok(())
}

/// Back up a regular destination by hard link while leaving missing paths absent.
fn optional_backup(path: &Path) -> Result<Option<PathBuf>, BuildError> {
    if path.exists() {
        create_backup_link(path).map(Some)
    } else {
        Ok(None)
    }
}

/// Remove one destination that preflight established as regular or absent.
fn remove_existing(path: &Path) -> Result<(), BuildError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(BuildError::Publish {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Remove partial new files and restore every destination that existed before publication.
fn restore_pair(
    output: &Path,
    output_backup: Option<&Path>,
    manifest: &Path,
    manifest_backup: Option<&Path>,
) -> Result<(), BuildError> {
    let _ = fs::remove_file(output);
    let _ = fs::remove_file(manifest);
    restore_optional(output_backup, output)?;
    restore_optional(manifest_backup, manifest)
}

/// Restore an owned backup only when that destination existed before publication.
fn restore_optional(backup: Option<&Path>, destination: &Path) -> Result<(), BuildError> {
    let Some(backup) = backup else {
        return Ok(());
    };
    fs::hard_link(backup, destination).map_err(|source| BuildError::Restore {
        path: destination.to_owned(),
        source,
    })?;
    let _ = fs::remove_file(backup);
    Ok(())
}

/// Remove only a backup created by this publication attempt.
fn remove_backup(backup: Option<&Path>) {
    if let Some(backup) = backup {
        let _ = fs::remove_file(backup);
    }
}

/// Reserve a collision-free hard-link backup without replacing unrelated files.
fn create_backup_link(output: &Path) -> Result<PathBuf, BuildError> {
    for _ in 0..32 {
        let backup = unique_sibling(output, "backup");
        match fs::hard_link(output, &backup) {
            Ok(()) => return Ok(backup),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(BuildError::Publish {
                    path: output.to_owned(),
                    source,
                });
            }
        }
    }
    Err(BuildError::Publish {
        path: output.to_owned(),
        source: io::Error::new(io::ErrorKind::AlreadyExists, "backup name exhausted"),
    })
}

/// Restore an existing artifact from its owned hard-link backup.
fn restore_backup(backup: &Path, output: &Path) -> Result<(), BuildError> {
    fs::hard_link(backup, output).map_err(|source| BuildError::Restore {
        path: output.to_owned(),
        source,
    })?;
    fs::remove_file(backup).map_err(|source| BuildError::Restore {
        path: backup.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Leave both previous destinations untouched when the manifest cannot be backed up.
    fn pair_publication_preserves_previous_results_on_failure() {
        let fixture = crate::test_support::TestDir::new();
        let root = &fixture.0;
        let artifact = root.join("staged.cf");
        let staged_manifest = root.join("staged.json");
        let output = root.join("result.cf");
        let manifest = root.join("result.cf.manifest.json");
        fs::write(&artifact, b"new artifact").expect("staged artifact");
        fs::write(&staged_manifest, b"new manifest").expect("staged manifest");
        fs::write(&output, b"old artifact").expect("old artifact");
        fs::create_dir(&manifest).expect("invalid manifest target");

        assert!(publish_pair(&artifact, &output, &staged_manifest, &manifest).is_err());
        assert_eq!(
            fs::read(&output).expect("preserved artifact"),
            b"old artifact"
        );
        assert!(manifest.is_dir());
    }
}
