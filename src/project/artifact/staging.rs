//! Exclusively owned temporary directories inside a self-ignored service directory.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{ArtifactError, io_error};

pub(super) struct Staging {
    pub path: PathBuf,
    pub preserve: bool,
}

impl Staging {
    /// Claim ignored storage near the destination, including before workspace parents exist.
    pub fn create(destination: &Path) -> Result<Self, ArtifactError> {
        let parent = destination
            .ancestors()
            .skip(1)
            .find(|path| path.is_dir())
            .ok_or_else(|| ArtifactError::UnsafePath(destination.to_owned()))?;
        let service = parent.join(".eska");
        let storage = service.join("import");
        if storage.starts_with(destination) {
            return Err(ArtifactError::UnsafePath(destination.to_owned()));
        }
        ensure_directory(&service)?;
        ensure_directory(&storage)?;
        ensure_ignored(&storage)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for attempt in 0..1024 {
            let path = storage.join(format!(
                ".eska-import-{}-{stamp}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        preserve: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io_error(&path, error)),
            }
        }
        Err(io_error(
            &storage,
            io::Error::from(io::ErrorKind::AlreadyExists),
        ))
    }
}

/// Refuse links and files instead of redirecting temporary data or retained backups.
fn ensure_directory(path: &Path) -> Result<(), ArtifactError> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if fs::symlink_metadata(path)
                .map_err(|error| io_error(path, error))?
                .is_dir()
            {
                Ok(())
            } else {
                Err(ArtifactError::UnsafePath(path.to_owned()))
            }
        }
        Err(error) => Err(io_error(path, error)),
    }
}

/// Ignore the entire service directory, including this rule, without editing project files.
fn ensure_ignored(storage: &Path) -> Result<(), ArtifactError> {
    let path = storage.join(".gitignore");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => file
            .write_all(b"*\n")
            .map_err(|error| io_error(&path, error)),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(&path).map_err(|error| io_error(&path, error))?;
            if !metadata.is_file() || metadata.len() != 2 {
                return Err(ArtifactError::UnsafePath(path));
            }
            if fs::read(&path).map_err(|error| io_error(&path, error))? != b"*\n" {
                return Err(ArtifactError::UnsafePath(path));
            }
            Ok(())
        }
        Err(error) => Err(io_error(&path, error)),
    }
}

impl Drop for Staging {
    /// Remove only the directory exclusively claimed by this operation.
    fn drop(&mut self) {
        if !self.preserve {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    /// Both an active staging tree and retained recovery data remain excluded from Git.
    #[test]
    fn storage_ignores_itself_and_preserves_only_requested_backups() {
        let fixture = TestDir::new();
        let destination = fixture.0.join("src/member");
        let path = {
            let staging = Staging::create(&destination).unwrap();
            fs::write(staging.path.join("payload"), "data").unwrap();
            staging.path.clone()
        };
        assert!(!path.exists());
        assert_eq!(
            fs::read(fixture.0.join(".eska/import/.gitignore")).unwrap(),
            b"*\n"
        );
        let path = {
            let mut backup = Staging::create(&destination).unwrap();
            backup.preserve = true;
            fs::write(backup.path.join("previous"), "keep").unwrap();
            backup.path.clone()
        };
        assert_eq!(fs::read(path.join("previous")).unwrap(), b"keep");
    }

    /// Reserved storage cannot overlap the source tree being replaced.
    #[test]
    fn refuses_storage_inside_destination() {
        let fixture = TestDir::new();
        let destination = fixture.0.join(".eska");
        assert!(matches!(
            Staging::create(&destination),
            Err(ArtifactError::UnsafePath(_))
        ));
        assert!(!destination.exists());
    }

    /// Never overwrite a user's different ignore rule to prepare temporary data.
    #[test]
    fn preserves_foreign_ignore_rules() {
        let fixture = TestDir::new();
        let storage = fixture.0.join(".eska/import");
        fs::create_dir_all(&storage).unwrap();
        fs::write(storage.join(".gitignore"), "!keep\n").unwrap();
        assert!(matches!(
            Staging::create(&fixture.0.join("src")),
            Err(ArtifactError::UnsafePath(_))
        ));
        assert_eq!(fs::read(storage.join(".gitignore")).unwrap(), b"!keep\n");
    }

    /// Check every managed path component before writing through a possible symlink.
    #[cfg(unix)]
    #[test]
    fn refuses_links_in_storage_and_ignore_file() {
        for relative in [".eska", ".eska/import", ".eska/import/.gitignore"] {
            let fixture = TestDir::new();
            let outside = fixture.0.join("outside");
            fs::create_dir(&outside).unwrap();
            let link = fixture.0.join(relative);
            fs::create_dir_all(link.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            assert!(matches!(
                Staging::create(&fixture.0.join("src")),
                Err(ArtifactError::UnsafePath(_))
            ));
            assert!(outside.read_dir().unwrap().next().is_none());
        }
    }
}
