//! Deterministic source inventory detects newly added references as well as changed files.

use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

pub const MAX_TEXT_BYTES: u64 = 64 * 1024 * 1024;
/// Spreadsheet payloads exceed the descriptor ceiling and use sequential XML validation.
pub const MAX_XML_BYTES: u64 = 256 * 1024 * 1024;

/// Source-only paths; symlinks and non-UTF-8 paths cannot authorize a rename transaction.
#[derive(Debug)]
pub struct Inventory {
    pub files: Vec<PathBuf>,
    pub directories: BTreeSet<PathBuf>,
}

impl Inventory {
    /// Traverse in stable path order without following aliases or special files.
    pub fn read(root: &Path, excluded: &[PathBuf]) -> io::Result<Self> {
        let mut inventory = Self {
            files: Vec::new(),
            directories: BTreeSet::new(),
        };
        let mut pending = vec![PathBuf::new()];
        while let Some(relative) = pending.pop() {
            for entry in fs::read_dir(root.join(&relative))? {
                let entry = entry?;
                let path = relative.join(entry.file_name());
                if excluded.iter().any(|excluded| path.starts_with(excluded)) {
                    continue;
                }
                let kind = entry.file_type()?;
                if path.to_str().is_none() || kind.is_symlink() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "unsupported_source_path",
                    ));
                }
                if kind.is_dir() {
                    inventory.directories.insert(path.clone());
                    pending.push(path);
                } else if kind.is_file() {
                    inventory.files.push(path);
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "unsupported_source_entry",
                    ));
                }
            }
        }
        inventory.files.sort();
        Ok(inventory)
    }
}

/// Hash binary payloads without retaining them in memory or depending on mtimes.
pub fn file_hash(path: &Path) -> io::Result<[u8; 32]> {
    let mut input = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = [0_u8; 16 * 1024];
    loop {
        let count = input.read(&mut bytes)?;
        if count == 0 {
            return Ok(digest.finalize().into());
        }
        digest.update(&bytes[..count]);
    }
}

/// Read text with the parser's byte ceiling even if another process grows the file concurrently.
pub fn read_text(path: &Path, max_bytes: u64) -> io::Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source_too_large",
        ));
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
