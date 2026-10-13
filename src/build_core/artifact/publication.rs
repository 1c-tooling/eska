//! Unpack publication owns a new directory and cannot replace an existing project or source tree.

use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use super::{PreparedArtifact, UnpackError, io_error, snapshot::check_cancelled};

/// Resolve an absent output below a real existing parent before starting the platform.
///
/// # Errors
/// Rejects existing destinations, missing parents, root paths and unsupported entries.
pub fn unpack_destination(destination: &Path) -> Result<PathBuf, UnpackError> {
    require_absent(destination)?;
    let name = destination
        .file_name()
        .ok_or_else(|| UnpackError::UnsafePath(destination.to_owned()))?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|error| io_error(parent, error))?;
    if !parent.is_dir() {
        return Err(UnpackError::UnsafePath(parent));
    }
    let path = parent.join(name);
    require_absent(&path)?;
    Ok(path)
}

/// Retain only the directory claimed by this call, removing incomplete output on normal failure.
struct ClaimedDirectory {
    path: PathBuf,
    complete: bool,
}

impl Drop for ClaimedDirectory {
    /// A failed operation never cleans up a destination that existed before its claim.
    fn drop(&mut self) {
        if !self.complete {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Recheck the prepared bytes before and after copying into exclusively claimed storage.
pub(super) fn publish(
    artifact: &PreparedArtifact,
    destination: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), UnpackError> {
    let path = unpack_destination(destination)?;
    artifact.verify_snapshot(&artifact.sources(), cancelled)?;
    fs::create_dir(&path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            UnpackError::DestinationExists(path.clone())
        } else {
            io_error(&path, error)
        }
    })?;
    let mut claimed = ClaimedDirectory {
        path,
        complete: false,
    };
    copy_directory(&artifact.sources(), &claimed.path, cancelled)?;
    artifact.verify_snapshot(&artifact.sources(), cancelled)?;
    artifact.verify_snapshot(&claimed.path, cancelled)?;
    check_cancelled(cancelled)?;
    claimed.complete = true;
    Ok(())
}

/// Reject aliases during traversal and create every output file without overwrite semantics.
fn copy_directory(
    source: &Path,
    destination: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), UnpackError> {
    check_cancelled(cancelled)?;
    for entry in fs::read_dir(source).map_err(|error| io_error(source, error))? {
        check_cancelled(cancelled)?;
        let entry = entry.map_err(|error| io_error(source, error))?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        let kind = entry.file_type().map_err(|error| io_error(&path, error))?;
        if kind.is_dir() {
            fs::create_dir(&target).map_err(|error| io_error(&target, error))?;
            copy_directory(&path, &target, cancelled)?;
        } else if kind.is_file() {
            let mut input = fs::File::open(&path).map_err(|error| io_error(&path, error))?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|error| io_error(&target, error))?;
            let mut bytes = [0_u8; 16 * 1024];
            loop {
                check_cancelled(cancelled)?;
                let count = input
                    .read(&mut bytes)
                    .map_err(|error| io_error(&path, error))?;
                if count == 0 {
                    break;
                }
                output
                    .write_all(&bytes[..count])
                    .map_err(|error| io_error(&target, error))?;
            }
        } else {
            return Err(UnpackError::UnsafePath(path));
        }
    }
    Ok(())
}

/// A dangling link is an existing destination too, never free space for this operation.
fn require_absent(path: &Path) -> Result<(), UnpackError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(UnpackError::DestinationExists(path.to_owned())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(path, error)),
    }
}
