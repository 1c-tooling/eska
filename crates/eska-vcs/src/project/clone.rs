//! Clone planning and execution without CLI presentation or system Git.

use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Component, Path, PathBuf},
};

use gix::bstr::ByteSlice;

use super::{
    Project,
    discovery::{self, DiscoveryError},
};
use crate::vcs::network;

/// A validated clone request which has not changed the filesystem yet.
pub struct ClonePlan {
    repository: gix::Url,
    destination: PathBuf,
    remote_name: String,
}

impl ClonePlan {
    /// Return the absolute destination that will be created by this clone.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }
}

/// Validate clone inputs and resolve paths without creating the destination.
///
/// Relative local repositories and destinations are resolved from `base`.
///
/// # Errors
/// Returns structured URL, remote-name, destination and filesystem errors.
pub fn inspect(
    base: &Path,
    repository: &OsStr,
    directory: Option<&Path>,
    remote_name: &str,
) -> Result<ClonePlan, CloneError> {
    let base = fs::canonicalize(base).map_err(|source| CloneError::Io {
        path: base.to_owned(),
        source,
    })?;
    if !base.is_dir() {
        return Err(CloneError::InvalidBase { path: base });
    }

    let mut repository = gix::Url::try_from(repository).map_err(CloneError::RepositoryUrl)?;
    repository
        .canonicalize(&base)
        .map_err(|source| CloneError::LocalRepository {
            path: repository_path(&repository),
            source,
        })?;
    gix::remote::name::validated(remote_name.as_bytes()).map_err(CloneError::RemoteName)?;

    let directory = match directory {
        Some(directory) => directory.to_owned(),
        None => PathBuf::from(default_directory(&repository)?),
    };
    let destination = resolve_destination(&base.join(directory))?;
    Ok(ClonePlan {
        repository,
        destination,
        remote_name: remote_name.to_owned(),
    })
}

/// Clone, fetch and check out a planned repository through `gix`, then validate the project.
///
/// The destination is claimed exclusively. Any ordinary failure removes only that newly
/// created directory; a cleanup failure retains the original error and affected path.
///
/// # Errors
/// Returns structured clone, checkout, project-validation or rollback errors.
pub fn execute(plan: ClonePlan) -> Result<Project, CloneError> {
    claim_destination(&plan.destination)?;
    match clone_and_validate(&plan) {
        Ok(project) => Ok(project),
        Err(original) => match fs::remove_dir_all(&plan.destination) {
            Ok(()) => Err(original),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Err(original),
            Err(source) => Err(CloneError::Rollback {
                path: plan.destination,
                original: Box::new(original),
                source,
            }),
        },
    }
}

/// Perform the gix fetch and checkout, then apply normal eska discovery.
fn clone_and_validate(plan: &ClonePlan) -> Result<Project, CloneError> {
    let outcome = network::clone_checkout(
        plan.repository.clone(),
        &plan.destination,
        &plan.remote_name,
    )
    .map_err(CloneError::Network)?;
    if outcome.collisions != 0 || outcome.errors != 0 {
        return Err(CloneError::IncompleteCheckout {
            collisions: outcome.collisions,
            errors: outcome.errors,
        });
    }
    discovery::discover(&plan.destination)
        .map_err(|source| CloneError::Validation(Box::new(source)))
}

/// Derive the conventional destination name from a parsed repository address.
fn default_directory(repository: &gix::Url) -> Result<OsString, CloneError> {
    let mut path = repository.path.as_slice();
    if matches!(
        repository.scheme,
        gix::url::Scheme::Http | gix::url::Scheme::Https
    ) && let Some(end) = path.iter().position(|byte| matches!(byte, b'?' | b'#'))
    {
        path = &path[..end];
    }
    while path.last() == Some(&b'/') {
        path = &path[..path.len() - 1];
    }
    let name = path.rsplit(|byte| *byte == b'/').next().unwrap_or_default();
    let name = name.strip_suffix(b".git").unwrap_or(name);
    if name.is_empty() || matches!(name, b"." | b"..") {
        return Err(CloneError::MissingDirectoryName);
    }
    Ok(gix::path::from_bstr(name.as_bstr())
        .into_owned()
        .into_os_string())
}

/// Convert a local repository URL path into an operating-system path.
fn repository_path(repository: &gix::Url) -> PathBuf {
    gix::path::from_bstr(repository.path.as_bstr()).into_owned()
}

/// Resolve a missing destination while rejecting traversal and collisions.
fn resolve_destination(destination: &Path) -> Result<PathBuf, CloneError> {
    if destination.as_os_str().is_empty()
        || destination
            .components()
            .any(|part| part == Component::ParentDir)
    {
        return Err(CloneError::InvalidDestination {
            path: destination.to_owned(),
        });
    }
    let name = destination
        .file_name()
        .ok_or_else(|| CloneError::InvalidDestination {
            path: destination.to_owned(),
        })?;
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|source| CloneError::Io {
        path: parent.to_owned(),
        source,
    })?;
    let destination = parent.join(name);
    match fs::symlink_metadata(&destination) {
        Ok(_) => Err(CloneError::AlreadyExists { path: destination }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(destination),
        Err(source) => Err(CloneError::Io {
            path: destination,
            source,
        }),
    }
}

/// Claim ownership of the exact destination before clone writes any files.
fn claim_destination(destination: &Path) -> Result<(), CloneError> {
    fs::create_dir(destination).map_err(|source| {
        if source.kind() == io::ErrorKind::AlreadyExists {
            CloneError::AlreadyExists {
                path: destination.to_owned(),
            }
        } else {
            CloneError::Io {
                path: destination.to_owned(),
                source,
            }
        }
    })
}

#[derive(Debug)]
pub enum CloneError {
    RepositoryUrl(gix::url::parse::Error),
    LocalRepository {
        path: PathBuf,
        source: gix::path::realpath::Error,
    },
    RemoteName(gix::remote::name::Error),
    MissingDirectoryName,
    InvalidBase {
        path: PathBuf,
    },
    InvalidDestination {
        path: PathBuf,
    },
    AlreadyExists {
        path: PathBuf,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Network(network::CloneError),
    IncompleteCheckout {
        collisions: usize,
        errors: usize,
    },
    Validation(Box<DiscoveryError>),
    Rollback {
        path: PathBuf,
        original: Box<Self>,
        source: io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    /// Parse a URL and derive its default checkout directory.
    fn derived_name(repository: &str) -> OsString {
        let repository = gix::Url::try_from(repository).expect("valid repository URL");
        default_directory(&repository).expect("directory name")
    }

    #[test]
    fn derives_destination_from_common_repository_addresses() {
        assert_eq!(
            derived_name("https://example.test/team/project.git"),
            "project"
        );
        assert_eq!(
            derived_name("https://example.test/team/project.git?token=value"),
            "project"
        );
        assert_eq!(derived_name("git@example.test:team/project.git"), "project");
        assert_eq!(derived_name("relative/project.git"), "project");
    }

    #[test]
    fn inspect_rejects_an_existing_destination_without_changing_it() {
        let fixture = TestDir::new();
        let source = fixture.0.join("source.git");
        fs::create_dir(&source).expect("source");
        let destination = fixture.0.join("existing");
        fs::create_dir(&destination).expect("destination");
        fs::write(destination.join("user-file"), "preserved").expect("user file");

        let error = inspect(
            &fixture.0,
            OsStr::new("source.git"),
            Some(Path::new("existing")),
            "origin",
        )
        .err()
        .expect("collision");

        assert!(matches!(error, CloneError::AlreadyExists { .. }));
        assert_eq!(
            fs::read_to_string(destination.join("user-file")).expect("user file"),
            "preserved"
        );
    }
}
