//! Creation of a new project directory; no CLI parsing or localized output.

use std::{
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use crate::{
    config::ProjectConfig,
    discovery::{self, DiscoveryError},
    project::{Project, ProjectType, WorkflowPreset},
    templates::Template,
};

/// Creates a project without modifying any pre-existing destination.
///
/// The parent directory must exist. Ordinary errors roll back only the newly
/// created directory; cleanup failures retain both errors and the affected path.
/// Process termination and concurrent external edits are not transactional.
///
/// # Errors
///
/// Returns structured path, I/O, Git, validation or rollback errors.
pub fn create(
    destination: &Path,
    project_type: ProjectType,
    workflow: WorkflowPreset,
    initialize_vcs: bool,
) -> Result<Project, CreationError> {
    let destination = resolve_destination(destination)?;
    let config = ProjectConfig::new(project_type).with_workflow(workflow);
    let template = Template::from_config(&config).map_err(CreationError::Template)?;
    in_new_directory(&destination, |root| {
        for directory in template.directories() {
            let path = root.join(directory);
            fs::create_dir(&path).map_err(|source| CreationError::Io { path, source })?;
        }
        for entry in template.files() {
            let path = root.join(entry.path());
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|source| CreationError::Io {
                    path: path.clone(),
                    source,
                })?;
            file.write_all(entry.contents().as_bytes())
                .map_err(|source| CreationError::Io { path, source })?;
        }
        if initialize_vcs {
            crate::vcs::initialize(root).map_err(CreationError::Git)?;
        }
        discovery::discover(root).map_err(|error| CreationError::Validation(Box::new(error)))
    })
}

/// Preflights a destination without creating it, useful before interactive prompts.
///
/// # Errors
///
/// Rejects empty/current/parent paths, missing parents, and existing destinations.
pub fn resolve_destination(destination: &Path) -> Result<PathBuf, CreationError> {
    if destination.as_os_str().is_empty()
        || destination
            .components()
            .any(|part| part == Component::ParentDir)
    {
        return Err(CreationError::InvalidDestination {
            path: destination.to_path_buf(),
        });
    }
    let name = destination
        .file_name()
        .ok_or_else(|| CreationError::InvalidDestination {
            path: destination.to_path_buf(),
        })?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|source| CreationError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(_) => Err(CreationError::AlreadyExists { path }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(path),
        Err(source) => Err(CreationError::Io { path, source }),
    }
}

fn in_new_directory(
    destination: &Path,
    operation: impl FnOnce(&Path) -> Result<Project, CreationError>,
) -> Result<Project, CreationError> {
    // create_dir, unlike create_dir_all, claims the destination exclusively.
    fs::create_dir(destination).map_err(|source| {
        if source.kind() == io::ErrorKind::AlreadyExists {
            CreationError::AlreadyExists {
                path: destination.to_path_buf(),
            }
        } else {
            CreationError::Io {
                path: destination.to_path_buf(),
                source,
            }
        }
    })?;
    match operation(destination) {
        Ok(project) => Ok(project),
        Err(original) => match fs::remove_dir_all(destination) {
            Ok(()) => Err(original),
            Err(source) => Err(CreationError::Rollback {
                path: destination.to_path_buf(),
                original: Box::new(original),
                source,
            }),
        },
    }
}

#[derive(Debug)]
pub enum CreationError {
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
    Template(toml::ser::Error),
    Git(Box<gix::init::Error>),
    Validation(Box<DiscoveryError>),
    Rollback {
        path: PathBuf,
        original: Box<Self>,
        source: io::Error,
    },
}

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_operation_removes_only_its_new_directory() {
        let fixture = test_support::TestDir::new();
        let sentinel = fixture.0.join("user-file");
        fs::write(&sentinel, "preserved").expect("sentinel");
        let destination = fixture.0.join("partial-project");
        let result = in_new_directory(&destination, |root| {
            fs::create_dir(root.join("src")).expect("partial source");
            fs::write(root.join("eska.toml"), "partial config").expect("partial config");
            crate::vcs::initialize(root).expect("partial Git repository");
            Err(CreationError::Io {
                path: root.join("injected-error"),
                source: io::Error::other("injected failure"),
            })
        });
        assert!(matches!(result, Err(CreationError::Io { .. })));
        assert!(!destination.exists());
        assert_eq!(
            fs::read_to_string(sentinel).expect("sentinel intact"),
            "preserved"
        );
    }

    #[test]
    fn existing_directory_is_not_owned_by_transaction() {
        let fixture = test_support::TestDir::new();
        let result = in_new_directory(&fixture.0, |_| panic!("must not enter existing directory"));
        assert!(matches!(result, Err(CreationError::AlreadyExists { .. })));
        assert!(fixture.0.is_dir());
    }

    #[test]
    fn cleanup_failure_preserves_original_error_and_reports_remaining_path() {
        let fixture = test_support::TestDir::new();
        let destination = fixture.0.join("cleanup-failure");
        let result = in_new_directory(&destination, |root| {
            // Inject a deterministic cleanup failure without relying on user privileges.
            fs::remove_dir(root).expect("remove owned empty directory");
            fs::write(root, "leftover").expect("replace owned directory with file");
            Err(CreationError::Io {
                path: root.to_path_buf(),
                source: io::Error::other("original failure"),
            })
        });
        let Err(CreationError::Rollback { path, original, .. }) = result else {
            panic!("expected structured rollback failure");
        };
        assert_eq!(path, destination);
        assert!(
            matches!(*original, CreationError::Io { source, .. } if source.to_string() == "original failure")
        );
        assert_eq!(
            fs::read_to_string(destination).expect("remaining file"),
            "leftover"
        );
    }
}
