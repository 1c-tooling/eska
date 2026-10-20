//! Deterministic source inventory detects newly added references as well as changed files.

use std::{
    collections::{BTreeMap, BTreeSet},
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

/// Recheck the full preview snapshot after staging without reparsing every source file.
pub fn snapshot(root: &Path, excluded: &[PathBuf], id: &str, name: &str) -> io::Result<String> {
    let inventory = Inventory::read(root, excluded)?;
    let files = inventory
        .files
        .into_iter()
        .map(|path| Ok((path.clone(), file_hash(&root.join(path))?)))
        .collect::<io::Result<_>>()?;
    Ok(fingerprint(id, name, &files, &inventory.directories))
}

/// Predict the exact post-rename inventory before publication; never bless a later external edit.
pub fn transition(
    root: &Path,
    excluded: &[PathBuf],
    plan: &super::RenamePlan,
    staged: &BTreeMap<PathBuf, [u8; 32]>,
) -> io::Result<(String, String)> {
    let inventory = Inventory::read(root, excluded)?;
    let files: BTreeMap<_, _> = inventory
        .files
        .into_iter()
        .map(|path| Ok((path.clone(), file_hash(&root.join(path))?)))
        .collect::<io::Result<_>>()?;
    let before = fingerprint(
        plan.object_id.as_str(),
        &plan.new_name,
        &files,
        &inventory.directories,
    );
    let after_files = files
        .iter()
        .map(|(path, hash)| {
            (
                super::history::moved_path(path, &plan.moves),
                *staged.get(path).unwrap_or(hash),
            )
        })
        .collect();
    let after_directories = inventory
        .directories
        .iter()
        .map(|path| super::history::moved_path(path, &plan.moves))
        .collect();
    Ok((
        before,
        fingerprint(
            plan.new_object_id.as_str(),
            &plan.old_name,
            &after_files,
            &after_directories,
        ),
    ))
}

/// Identical fingerprint framing is used for current sources and projected undo snapshots.
fn fingerprint(
    id: &str,
    name: &str,
    files: &BTreeMap<PathBuf, [u8; 32]>,
    directories: &BTreeSet<PathBuf>,
) -> String {
    use std::fmt::Write;
    let mut digest = Sha256::new();
    digest.update(id);
    digest.update([0]);
    digest.update(name);
    for (path, hash) in files {
        digest.update([0]);
        digest.update((path.as_os_str().as_encoded_bytes().len() as u64).to_le_bytes());
        digest.update(path.as_os_str().as_encoded_bytes());
        digest.update(hash);
    }
    for path in directories {
        digest.update([1]);
        digest.update((path.as_os_str().as_encoded_bytes().len() as u64).to_le_bytes());
        digest.update(path.as_os_str().as_encoded_bytes());
    }
    digest
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        })
}
