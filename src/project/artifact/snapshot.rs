//! Source fingerprints prevent a slow platform export from overwriting intervening edits.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use super::{ArtifactError, io_error};

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Snapshot {
    digest: [u8; 32],
    pub files: usize,
    pub empty_scaffold: bool,
}

/// Hash byte-exact files and directory names without following symlinks or special files.
pub(super) fn capture(root: &Path) -> Result<Snapshot, ArtifactError> {
    let mut hasher = Sha256::new();
    let mut files = Vec::new();
    walk(root, root, &mut hasher, &mut files)?;
    Ok(Snapshot {
        digest: hasher.finalize().into(),
        files: files.len(),
        empty_scaffold: files.is_empty()
            || (files.len() == 1
                && files[0] == Path::new(".gitkeep")
                && fs::metadata(root.join(".gitkeep"))
                    .map_err(|error| io_error(root, error))?
                    .len()
                    == 0),
    })
}

/// Keep the traversal deterministic and retain no source buffers between files.
fn walk(
    root: &Path,
    directory: &Path,
    hasher: &mut Sha256,
    files: &mut Vec<PathBuf>,
) -> Result<(), ArtifactError> {
    let mut paths = fs::read_dir(directory)
        .map_err(|error| io_error(directory, error))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| io_error(directory, error))?;
    paths.sort();
    for path in paths {
        let metadata = fs::symlink_metadata(&path).map_err(|error| io_error(&path, error))?;
        if (!metadata.is_dir() && !metadata.is_file())
            || path
                .file_name()
                .is_some_and(|name| name == ".git" || name == "eska.toml")
        {
            return Err(ArtifactError::UnsafePath(path));
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| ArtifactError::UnsafePath(path.clone()))?;
        let encoded = gix::path::into_bstr(relative);
        hasher.update((encoded.len() as u64).to_le_bytes());
        hasher.update(encoded.as_ref());
        hasher.update([u8::from(metadata.is_dir())]);
        if metadata.is_dir() {
            walk(root, &path, hasher, files)?;
        } else {
            hasher.update(metadata.len().to_le_bytes());
            let mut file = fs::File::open(&path).map_err(|error| io_error(&path, error))?;
            let mut buffer = [0; 8192];
            loop {
                let length = file
                    .read(&mut buffer)
                    .map_err(|error| io_error(&path, error))?;
                if length == 0 {
                    break;
                }
                hasher.update(&buffer[..length]);
            }
            files.push(relative.to_owned());
        }
    }
    Ok(())
}
