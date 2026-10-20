//! Persistent managed infobases shared by repeated builds of one project.

use std::{
    fmt::Write as _,
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::BuildPlan;

const OWNER_FILE: &str = "owner";
const OWNER_MARKER: &[u8] = b"eska-managed-infobase-v1\n";
const STATE_FILE: &str = "state.json";

#[derive(Debug)]
pub enum ManagedInfobaseError {
    Io { path: PathBuf, source: io::Error },
    Locked { path: PathBuf },
    Unowned { path: PathBuf },
    OutsideScope { root: PathBuf, scope: PathBuf },
    StateSerialize(serde_json::Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanOutcome {
    Removed,
    AlreadyAbsent,
}

pub(super) struct ManagedInfobase {
    root: PathBuf,
    data: PathBuf,
    state: StateDocument,
    needs_creation: bool,
    _lock: File,
}

impl ManagedInfobase {
    /// Lock and prepare the reusable infobase selected by the build plan.
    ///
    /// # Errors
    /// Returns a structured error for inaccessible, busy, or non-owned cache paths.
    pub(super) fn prepare(
        plan: &BuildPlan,
        base_configuration: Option<&Path>,
    ) -> Result<Self, ManagedInfobaseError> {
        let root = plan.infobase_root().to_owned();
        validate_scope(&root, plan.output_scope_root())?;
        let state = StateDocument::capture(plan, base_configuration)?;
        let lock = acquire_lock(&root)?;
        let reusable = validate_owned_root(&root)?
            && read_state(&root).is_some_and(|saved| saved == state)
            && root.join("data").is_dir();
        let needs_creation = plan.recreates_infobase() || !reusable;
        if needs_creation {
            reset_owned_root(&root)?;
        }
        Ok(Self {
            data: root.join("data"),
            root,
            state,
            needs_creation,
            _lock: lock,
        })
    }

    #[must_use]
    pub(super) fn data(&self) -> &Path {
        &self.data
    }

    #[must_use]
    pub(super) const fn needs_creation(&self) -> bool {
        self.needs_creation
    }

    /// Mark a successfully created infobase as reusable by later builds.
    ///
    /// # Errors
    /// Returns a structured error when the state document cannot be serialized or written.
    pub(super) fn mark_ready(&self) -> Result<(), ManagedInfobaseError> {
        let bytes =
            serde_json::to_vec_pretty(&self.state).map_err(ManagedInfobaseError::StateSerialize)?;
        let path = self.root.join(STATE_FILE);
        let mut file = File::create(&path).map_err(|source| ManagedInfobaseError::Io {
            path: path.clone(),
            source,
        })?;
        file.write_all(&bytes)
            .and_then(|()| file.write_all(b"\n"))
            .map_err(|source| ManagedInfobaseError::Io { path, source })
    }
}

/// Delete one reusable infobase after verifying ownership and acquiring its lock.
///
/// # Errors
/// Returns a structured error for inaccessible, busy, or non-owned paths.
pub fn clean(root: &Path, scope: &Path) -> Result<CleanOutcome, ManagedInfobaseError> {
    if !root.exists() {
        return Ok(CleanOutcome::AlreadyAbsent);
    }
    validate_scope(root, scope)?;
    let _lock = acquire_lock(root)?;
    if !validate_owned_root(root)? {
        return Ok(CleanOutcome::AlreadyAbsent);
    }
    fs::remove_dir_all(root).map_err(|source| ManagedInfobaseError::Io {
        path: root.to_owned(),
        source,
    })?;
    Ok(CleanOutcome::Removed)
}

/// Ensure existing symlinks cannot move a managed directory outside its project scope.
fn validate_scope(root: &Path, scope: &Path) -> Result<(), ManagedInfobaseError> {
    let scope = fs::canonicalize(scope).map_err(|source| ManagedInfobaseError::Io {
        path: scope.to_owned(),
        source,
    })?;
    let mut ancestor = root;
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or_else(|| ManagedInfobaseError::Io {
            path: root.to_owned(),
            source: io::Error::new(io::ErrorKind::InvalidInput, "infobase root has no ancestor"),
        })?;
    }
    let resolved = fs::canonicalize(ancestor).map_err(|source| ManagedInfobaseError::Io {
        path: ancestor.to_owned(),
        source,
    })?;
    if resolved.starts_with(&scope) {
        Ok(())
    } else {
        Err(ManagedInfobaseError::OutsideScope {
            root: resolved,
            scope,
        })
    }
}

/// Open and exclusively lock the stable sibling lock file.
fn acquire_lock(root: &Path) -> Result<File, ManagedInfobaseError> {
    let parent = root.parent().ok_or_else(|| ManagedInfobaseError::Io {
        path: root.to_owned(),
        source: io::Error::new(io::ErrorKind::InvalidInput, "infobase root has no parent"),
    })?;
    fs::create_dir_all(parent).map_err(|source| ManagedInfobaseError::Io {
        path: parent.to_owned(),
        source,
    })?;
    let lock_path = root.with_extension(format!(
        "{}.lock",
        root.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
    ));
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|source| ManagedInfobaseError::Io {
            path: lock_path.clone(),
            source,
        })?;
    match lock.try_lock() {
        Ok(()) => Ok(lock),
        Err(TryLockError::WouldBlock) => Err(ManagedInfobaseError::Locked { path: lock_path }),
        Err(TryLockError::Error(source)) => Err(ManagedInfobaseError::Io {
            path: lock_path,
            source,
        }),
    }
}

/// Return whether an existing path is an eska-owned cache root.
fn validate_owned_root(root: &Path) -> Result<bool, ManagedInfobaseError> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(source) => {
            return Err(ManagedInfobaseError::Io {
                path: root.to_owned(),
                source,
            });
        }
    };
    if !metadata.file_type().is_dir() {
        return Err(ManagedInfobaseError::Unowned {
            path: root.to_owned(),
        });
    }
    let owner = root.join(OWNER_FILE);
    match fs::read(&owner) {
        Ok(contents) if contents == OWNER_MARKER => Ok(true),
        Ok(_) => Err(ManagedInfobaseError::Unowned {
            path: root.to_owned(),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Err(ManagedInfobaseError::Unowned {
                path: root.to_owned(),
            })
        }
        Err(source) => Err(ManagedInfobaseError::Io {
            path: owner,
            source,
        }),
    }
}

/// Replace only a verified owned root and immediately mark the new directory.
fn reset_owned_root(root: &Path) -> Result<(), ManagedInfobaseError> {
    if validate_owned_root(root)? {
        fs::remove_dir_all(root).map_err(|source| ManagedInfobaseError::Io {
            path: root.to_owned(),
            source,
        })?;
    }
    fs::create_dir(root).map_err(|source| ManagedInfobaseError::Io {
        path: root.to_owned(),
        source,
    })?;
    let owner = root.join(OWNER_FILE);
    fs::write(&owner, OWNER_MARKER).map_err(|source| ManagedInfobaseError::Io {
        path: owner,
        source,
    })
}

fn read_state(root: &Path) -> Option<StateDocument> {
    let contents = fs::read(root.join(STATE_FILE)).ok()?;
    serde_json::from_slice(&contents).ok()
}

#[derive(Deserialize, Eq, PartialEq, Serialize)]
struct StateDocument {
    schema_version: u8,
    platform_version: String,
    base_configuration_sha256: Option<String>,
}

impl StateDocument {
    /// Capture every input that changes the reusable infobase contents.
    fn capture(
        plan: &BuildPlan,
        base_configuration: Option<&Path>,
    ) -> Result<Self, ManagedInfobaseError> {
        Ok(Self {
            schema_version: 1,
            platform_version: plan.platform_version().as_str().to_owned(),
            base_configuration_sha256: base_configuration.map(file_checksum).transpose()?,
        })
    }
}

/// Hash the exact base configuration bytes without loading the file into memory.
fn file_checksum(path: &Path) -> Result<String, ManagedInfobaseError> {
    let mut file = File::open(path).map_err(|source| ManagedInfobaseError::Io {
        path: path.to_owned(),
        source,
    })?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| ManagedInfobaseError::Io {
                path: path.to_owned(),
                source,
            })?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    let digest = hash.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(encoded)
}
