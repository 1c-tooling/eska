//! Isolated Git opening and initialization shared by project and repository operations.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn initialize(root: &Path) -> Result<(), Box<gix::init::Error>> {
    // Ignore user/system config and Git environment overrides: initialization
    // must affect only the directory owned by the creation transaction.
    gix::ThreadSafeRepository::init_opts(
        root,
        gix::create::Kind::WithWorktree,
        gix::create::Options::default(),
        gix::open::Options::isolated(),
    )
    .map(|_| ())
    .map_err(Box::new)
}

#[derive(Debug)]
pub enum ExistingError {
    Io { path: PathBuf, source: io::Error },
    Open(Box<gix::open::Error>),
    Bare,
}

pub enum InitializeError {
    Io { path: PathBuf, source: io::Error },
    Git(Box<gix::init::Error>),
}

/// Inspect only on-disk repository markers, ignoring redirecting environment.
pub fn exists(root: &Path) -> Result<bool, ExistingError> {
    open_existing(root).map(|repository| repository.is_some())
}

/// Stop at the nearest marker, including a broken one: never silently select a parent repository.
pub(super) fn open_existing(root: &Path) -> Result<Option<gix::Repository>, ExistingError> {
    for ancestor in root.ancestors() {
        let marker = ancestor.join(".git");
        let found = match fs::symlink_metadata(&marker) {
            Ok(_) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(source) => {
                return Err(ExistingError::Io {
                    path: marker,
                    source,
                });
            }
        };
        let bare = ancestor.join("HEAD").is_file() && ancestor.join("objects").is_dir();
        if found || bare {
            let repo = gix::open_opts(ancestor, gix::open::Options::isolated())
                .map_err(|error| ExistingError::Open(Box::new(error)))?;
            if repo.is_bare() {
                return Err(ExistingError::Bare);
            }
            return Ok(Some(repo));
        }
    }
    Ok(None)
}

/// Populate an empty `.git` directory exclusively owned by the caller.
/// Stage through the same initializer as `new`, without ever reinitializing
/// existing user metadata. The caller owns rollback of the reserved directory.
pub fn initialize_reserved(git_dir: &Path) -> Result<(), InitializeError> {
    let staging = git_dir.join("eska-init");
    fs::create_dir(&staging).map_err(|source| InitializeError::Io {
        path: staging.clone(),
        source,
    })?;
    initialize(&staging).map_err(InitializeError::Git)?;
    let staged_git = staging.join(".git");
    for entry in fs::read_dir(&staged_git).map_err(|source| InitializeError::Io {
        path: staged_git.clone(),
        source,
    })? {
        let entry = entry.map_err(|source| InitializeError::Io {
            path: staged_git.clone(),
            source,
        })?;
        let target = git_dir.join(entry.file_name());
        fs::rename(entry.path(), &target).map_err(|source| InitializeError::Io {
            path: target,
            source,
        })?;
    }
    fs::remove_dir(&staged_git)
        .and_then(|()| fs::remove_dir(&staging))
        .map_err(|source| InitializeError::Io {
            path: staging,
            source,
        })
}
