//! Filesystem failures exercise restoration of both destinations without a platform process.

use super::*;
use crate::test_support::TestDir;

struct Pair {
    directory: TestDir,
    artifact: PathBuf,
    output: PathBuf,
    staged_manifest: PathBuf,
    manifest: PathBuf,
}

impl Pair {
    /// Each scenario owns its staged bytes and every pre-existing destination.
    fn new(existing: [bool; 2]) -> Self {
        let directory = TestDir::new();
        let root = &directory.0;
        let pair = Self {
            artifact: root.join("staged.cf"),
            output: root.join("result.cf"),
            staged_manifest: root.join("staged.json"),
            manifest: root.join("result.cf.manifest.json"),
            directory,
        };
        fs::write(&pair.artifact, b"new artifact").unwrap();
        fs::write(&pair.staged_manifest, b"new manifest").unwrap();
        if existing[0] {
            fs::write(&pair.output, b"old artifact").unwrap();
        }
        if existing[1] {
            fs::write(&pair.manifest, b"old manifest").unwrap();
        }
        pair
    }

    /// Use the same publication boundary as the build pipeline.
    fn publish(&self) -> Result<(), BuildError> {
        publish_pair(
            &self.artifact,
            &self.output,
            &self.staged_manifest,
            &self.manifest,
        )
    }

    /// Successful rollback restores absence as well as bytes and leaves no owned backup links.
    fn assert_restored(&self, existing: [bool; 2]) {
        for (path, expected, present) in [
            (&self.output, b"old artifact".as_slice(), existing[0]),
            (&self.manifest, b"old manifest".as_slice(), existing[1]),
        ] {
            if present {
                assert_eq!(fs::read(path).unwrap(), expected);
            } else {
                assert!(!path.exists());
            }
        }
        self.assert_no_backups();
    }

    /// Backups created by this attempt must disappear after success or complete restoration.
    fn assert_no_backups(&self) {
        assert!(fs::read_dir(&self.directory.0).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".eska-backup-")
        }));
    }
}

/// Leave previous results untouched when the second destination cannot be backed up.
#[test]
fn pair_publication_preserves_previous_results_on_failure() {
    let pair = Pair::new([true, false]);
    fs::create_dir(&pair.manifest).unwrap();
    assert!(
        matches!(pair.publish(), Err(BuildError::Publish { path, .. }) if path == pair.manifest)
    );
    assert_eq!(fs::read(&pair.output).unwrap(), b"old artifact");
    assert!(pair.manifest.is_dir());
    assert_eq!(fs::read(&pair.artifact).unwrap(), b"new artifact");
    assert_eq!(fs::read(&pair.staged_manifest).unwrap(), b"new manifest");
    pair.assert_no_backups();
}

/// Failing either hard-link step restores every combination of old and initially absent outputs.
#[test]
fn failed_pair_links_restore_both_destinations_and_preserve_staging() {
    for existing in [[false, false], [true, false], [false, true], [true, true]] {
        for fail_artifact in [true, false] {
            let pair = Pair::new(existing);
            let missing = if fail_artifact {
                &pair.artifact
            } else {
                &pair.staged_manifest
            };
            let failed_output = if fail_artifact {
                &pair.output
            } else {
                &pair.manifest
            };
            fs::remove_file(missing).unwrap();
            assert!(
                matches!(pair.publish(), Err(BuildError::Publish { path, .. }) if &path == failed_output)
            );
            pair.assert_restored(existing);
            if fail_artifact {
                assert_eq!(fs::read(&pair.staged_manifest).unwrap(), b"new manifest");
            } else {
                assert_eq!(fs::read(&pair.artifact).unwrap(), b"new artifact");
            }
        }
    }
}

/// Publishing replaces only the requested pair and consumes staging after both links succeed.
#[test]
fn successful_pair_publication_cleans_staging_and_owned_backups() {
    for existing in [[false, false], [true, false], [false, true], [true, true]] {
        let pair = Pair::new(existing);
        pair.publish().unwrap();
        assert_eq!(fs::read(&pair.output).unwrap(), b"new artifact");
        assert_eq!(fs::read(&pair.manifest).unwrap(), b"new manifest");
        assert!(!pair.artifact.exists());
        assert!(!pair.staged_manifest.exists());
        pair.assert_no_backups();
    }
}

/// An obstructed destination retains its backup while its independent sibling still recovers.
#[test]
fn failed_restore_does_not_skip_the_other_destination() {
    for blocked_output in [true, false] {
        let pair = Pair::new([true, true]);
        let output_backup = create_backup_link(&pair.output).unwrap();
        let manifest_backup = create_backup_link(&pair.manifest).unwrap();
        fs::remove_file(&pair.output).unwrap();
        fs::remove_file(&pair.manifest).unwrap();
        let blocked = if blocked_output {
            &pair.output
        } else {
            &pair.manifest
        };
        fs::create_dir(blocked).unwrap();
        let result = restore_pair(
            &pair.output,
            Some(&output_backup),
            &pair.manifest,
            Some(&manifest_backup),
        );
        assert!(matches!(result, Err(BuildError::Restore { path, .. }) if &path == blocked));
        assert!(blocked.is_dir());
        let (restored, removed_backup, retained_backup, old) = if blocked_output {
            (
                &pair.manifest,
                &manifest_backup,
                &output_backup,
                b"old manifest".as_slice(),
            )
        } else {
            (
                &pair.output,
                &output_backup,
                &manifest_backup,
                b"old artifact".as_slice(),
            )
        };
        assert_eq!(fs::read(restored).unwrap(), old);
        assert!(!removed_backup.exists());
        assert!(retained_backup.is_file());
    }
}

/// A single artifact obeys the same preservation rule when its staged source disappears.
#[test]
fn single_publication_replaces_or_restores_existing_artifacts() {
    for existing in [false, true] {
        for missing in [false, true] {
            let pair = Pair::new([existing, false]);
            if missing {
                fs::remove_file(&pair.artifact).unwrap();
                assert!(
                    matches!(publish(&pair.artifact, &pair.output), Err(BuildError::Publish { path, .. }) if path == pair.output)
                );
                pair.assert_restored([existing, false]);
            } else {
                publish(&pair.artifact, &pair.output).unwrap();
                assert_eq!(fs::read(&pair.output).unwrap(), b"new artifact");
                assert!(!pair.artifact.exists());
                pair.assert_no_backups();
            }
        }
    }
}
