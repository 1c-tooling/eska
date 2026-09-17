//! Byte-exact shelf payloads and conservative worktree path handling.

use super::Error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct Image {
    kind: Kind,
    digest: String,
    permissions: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum Kind {
    Missing,
    File,
    Link,
}

/// Validate a stored relative path without depending on the currently checked-out branch.
pub(super) fn relative_path(bytes: &[u8]) -> Result<PathBuf, Error> {
    let relative =
        gix::path::try_from_bstr(gix::bstr::BStr::new(bytes)).map_err(|_| Error::InvalidShelf)?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || relative
            .components()
            .any(|part| part.as_os_str().eq_ignore_ascii_case(".git"))
    {
        return Err(Error::InvalidShelf);
    }
    Ok(relative.into_owned())
}

/// Reject paths traversing existing symlinks before touching a worktree destination.
pub(super) fn work_path(root: &Path, bytes: &[u8]) -> Result<PathBuf, Error> {
    let relative = relative_path(bytes)?;
    let path = root.join(relative);
    let mut parent = path.parent();
    while let Some(directory) = parent.filter(|p| *p != root) {
        match fs::symlink_metadata(directory) {
            Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                return Err(Error::Collision(path));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        parent = directory.parent();
    }
    Ok(path)
}

/// Capture one file without following its final symlink or reading unrelated directories.
pub(super) fn capture(path: &Path, payload: &Path) -> Result<Image, Error> {
    let image = inspect(path)?;
    match image.kind {
        Kind::Missing => {}
        Kind::File => {
            fs::copy(path, payload)?;
            File::open(payload)?.sync_all()?;
        }
        Kind::Link => write_new(payload, &link_bytes(path)?)?,
    }
    if inspect(path)? != image {
        return Err(Error::Collision(path.to_owned()));
    }
    if image.kind != Kind::Missing && checksum(payload)? != image.digest {
        return Err(Error::InvalidShelf);
    }
    Ok(image)
}

/// Fingerprint bytes and filesystem mode without traversing a directory or following a link.
pub(super) fn inspect(path: &Path) -> Result<Image, Error> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Image {
                kind: Kind::Missing,
                digest: String::new(),
                permissions: 0,
            });
        }
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Ok(Image {
            kind: Kind::Link,
            digest: hash(&link_bytes(path)?),
            permissions: 0,
        });
    }
    if !metadata.is_file() {
        return Err(Error::Collision(path.to_owned()));
    }
    Ok(Image {
        kind: Kind::File,
        digest: checksum(path)?,
        permissions: mode(&metadata),
    })
}

/// Validate every payload before changing any destination file.
pub(super) fn validate(image: &Image, payload: &Path) -> Result<(), Error> {
    if image.kind != Kind::Missing && checksum(payload)? != image.digest {
        return Err(Error::InvalidShelf);
    }
    Ok(())
}

/// Restore a preflighted path, retaining the durable payload until the whole restore is verified.
pub(super) fn restore(path: &Path, image: &Image, payload: &Path) -> Result<(), Error> {
    if inspect(path)? == *image {
        return Ok(());
    }
    if image.kind == Kind::Missing {
        fs::remove_file(path)?;
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| Error::InvalidShelf)?
        .as_nanos();
    let temporary = path.with_file_name(format!(".eska-restore-{}-{unique:x}", std::process::id()));
    let result = (|| {
        match image.kind {
            Kind::File => {
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temporary)?;
                let mut input = File::open(payload)?;
                std::io::copy(&mut input, &mut output)?;
                output.sync_all()?;
                set_mode(&temporary, image.permissions)?;
            }
            Kind::Link => create_link(&fs::read(payload)?, &temporary)?,
            Kind::Missing => {}
        }
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Write an exclusively owned durable file without overwriting another operation's data.
pub(super) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Hash large files incrementally rather than loading a configuration payload into memory.
pub(super) fn checksum(path: &Path) -> Result<String, Error> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 65536].into_boxed_slice();
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
    }
    Ok(hex(&digest.finalize()))
}

/// Hash a small native symlink target.
fn hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

#[cfg(unix)]
/// Preserve native Unix file permissions, including the executable bit.
fn mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o777
}
#[cfg(windows)]
/// Preserve the Windows read-only attribute.
fn mode(metadata: &fs::Metadata) -> u32 {
    u32::from(metadata.permissions().readonly())
}

#[cfg(unix)]
/// Reapply the captured Unix permissions after writing file bytes.
fn set_mode(path: &Path, value: u32) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(value))?;
    Ok(())
}
#[cfg(windows)]
/// Reapply the captured Windows read-only attribute.
fn set_mode(path: &Path, value: u32) -> Result<(), Error> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(value != 0);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(unix)]
/// Preserve arbitrary Unix symlink target bytes.
fn link_bytes(path: &Path) -> Result<Vec<u8>, Error> {
    use std::os::unix::ffi::OsStrExt;
    Ok(fs::read_link(path)?.as_os_str().as_bytes().to_vec())
}
#[cfg(windows)]
/// Preserve Windows symlink targets as UTF-16 code units.
fn link_bytes(path: &Path) -> Result<Vec<u8>, Error> {
    use std::os::windows::ffi::OsStrExt;
    Ok(fs::read_link(path)?
        .as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect())
}
#[cfg(unix)]
/// Recreate a file symlink without following its target.
fn create_link(bytes: &[u8], path: &Path) -> Result<(), Error> {
    use std::{
        ffi::OsStr,
        os::unix::{ffi::OsStrExt, fs::symlink},
    };
    symlink(OsStr::from_bytes(bytes), path)?;
    Ok(())
}
#[cfg(windows)]
/// Recreate a native file symlink after validating its stored code units.
fn create_link(bytes: &[u8], path: &Path) -> Result<(), Error> {
    use std::{
        ffi::OsString,
        os::windows::{ffi::OsStringExt, fs::symlink_file},
    };
    if bytes.len() % 2 != 0 {
        return Err(Error::InvalidShelf);
    }
    let units: Vec<_> = bytes
        .chunks_exact(2)
        .map(|v| u16::from_le_bytes([v[0], v[1]]))
        .collect();
    symlink_file(OsString::from_wide(&units), path)?;
    Ok(())
}

/// Render a SHA-256 digest with stable lowercase hexadecimal digits.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("writing to String cannot fail");
    }
    text
}
