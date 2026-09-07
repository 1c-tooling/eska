//! Creation of a new project directory; no CLI parsing or localized output.

use std::{
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use crate::{
    config::ProjectConfig,
    vcs::{git, workflow::WorkflowPreset},
};

use super::{
    Project, ProjectName, ProjectType, Workspace,
    discovery::{self, ContextDiscoveryError, DiscoveryContext, DiscoveryError},
    onboarding::{self, WorkspaceEnrollmentError, WorkspaceMemberEnrollment},
    templates::Template,
};

/// A collision-free workspace destination prepared before interactive input.
#[derive(Debug)]
pub struct WorkspaceCreationPlan {
    destination: PathBuf,
    parent: PathBuf,
    create_parent: bool,
    name: ProjectName,
    enrollment: WorkspaceMemberEnrollment,
}

impl WorkspaceCreationPlan {
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }
}

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
            git::initialize(root).map_err(CreationError::Git)?;
        }
        discovery::discover(root).map_err(|error| CreationError::Validation(Box::new(error)))
    })
}

/// Preflights the conventional `src/<name>` destination of a workspace member.
///
/// # Errors
/// Returns structured name, path, collision, manifest, or workspace-layout errors.
pub fn inspect_workspace_member(
    workspace: &Workspace,
    name: ProjectName,
) -> Result<WorkspaceCreationPlan, CreationError> {
    let parent = workspace.root().join("src");
    let (resolved_parent, create_parent) = match fs::symlink_metadata(&parent) {
        Ok(_) => {
            let resolved = fs::canonicalize(&parent).map_err(|source| CreationError::Io {
                path: parent.clone(),
                source,
            })?;
            if !resolved.is_dir() || !resolved.starts_with(workspace.root()) {
                return Err(CreationError::InvalidDestination { path: parent });
            }
            (resolved, false)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (parent.clone(), true),
        Err(source) => {
            return Err(CreationError::Io {
                path: parent,
                source,
            });
        }
    };
    let destination = resolved_parent.join(name.as_str());
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            return Err(CreationError::AlreadyExists { path: destination });
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(CreationError::Io {
                path: destination,
                source,
            });
        }
    }
    let member_path = PathBuf::from("src").join(name.as_str());
    let enrollment = onboarding::prepare(workspace, destination.clone(), member_path, &name)
        .map_err(CreationError::Workspace)?;
    Ok(WorkspaceCreationPlan {
        destination,
        parent,
        create_parent,
        name,
        enrollment,
    })
}

/// Creates and enrolls one workspace member without nested Git or workflow files.
///
/// # Errors
/// Rolls back the new directory and exact root manifest bytes on ordinary failure.
pub fn create_workspace_member(
    plan: &WorkspaceCreationPlan,
    project_type: ProjectType,
) -> Result<Project, CreationError> {
    let template = Template::workspace_member(plan.name.clone(), project_type)
        .map_err(CreationError::Template)?;
    let owns_parent = if plan.create_parent {
        fs::create_dir(&plan.parent).map_err(|source| CreationError::Io {
            path: plan.parent.clone(),
            source,
        })?;
        true
    } else {
        false
    };
    if let Err(error) = fs::create_dir(&plan.destination) {
        if owns_parent {
            let _ = fs::remove_dir(&plan.parent);
        }
        return Err(if error.kind() == io::ErrorKind::AlreadyExists {
            CreationError::AlreadyExists {
                path: plan.destination.clone(),
            }
        } else {
            CreationError::Io {
                path: plan.destination.clone(),
                source: error,
            }
        });
    }
    let mut manifest_published = false;
    let result = (|| {
        write_template(&plan.destination, &template)?;
        plan.enrollment
            .publish()
            .map_err(CreationError::Workspace)?;
        manifest_published = true;
        let context = discovery::discover_context(plan.enrollment.workspace_root())
            .map_err(|error| CreationError::WorkspaceValidation(Box::new(error)))?;
        let DiscoveryContext::Workspace { workspace, .. } = context else {
            return Err(CreationError::WorkspaceMemberMissing {
                name: plan.name.clone(),
            });
        };
        workspace
            .member(&plan.name)
            .map(|member| member.project().clone())
            .ok_or_else(|| CreationError::WorkspaceMemberMissing {
                name: plan.name.clone(),
            })
    })();
    if let Ok(project) = result {
        return Ok(project);
    }
    let original = result.expect_err("handled successful result");
    let mut paths = Vec::new();
    if manifest_published && plan.enrollment.restore().is_err() {
        paths.push(
            plan.enrollment
                .workspace_root()
                .join(crate::config::FILE_NAME),
        );
    }
    if fs::remove_dir_all(&plan.destination).is_err() {
        paths.push(plan.destination.clone());
    }
    if owns_parent && fs::remove_dir(&plan.parent).is_err() {
        paths.push(plan.parent.clone());
    }
    if paths.is_empty() {
        Err(original)
    } else {
        Err(CreationError::WorkspaceRollback {
            paths,
            original: Box::new(original),
        })
    }
}

fn write_template(root: &Path, template: &Template) -> Result<(), CreationError> {
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
            .and_then(|()| file.sync_all())
            .map_err(|source| CreationError::Io { path, source })?;
    }
    Ok(())
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
    Workspace(WorkspaceEnrollmentError),
    WorkspaceValidation(Box<ContextDiscoveryError>),
    WorkspaceMemberMissing {
        name: ProjectName,
    },
    Rollback {
        path: PathBuf,
        original: Box<Self>,
        source: io::Error,
    },
    WorkspaceRollback {
        paths: Vec<PathBuf>,
        original: Box<Self>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    #[test]
    fn failed_operation_removes_only_its_new_directory() {
        let fixture = test_support::TestDir::new();
        let sentinel = fixture.0.join("user-file");
        fs::write(&sentinel, "preserved").expect("sentinel");
        let destination = fixture.0.join("partial-project");
        let result = in_new_directory(&destination, |root| {
            fs::create_dir(root.join("src")).expect("partial source");
            fs::write(root.join("eska.toml"), "partial config").expect("partial config");
            git::initialize(root).expect("partial Git repository");
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

    #[test]
    fn workspace_validation_failure_restores_manifest_and_removes_new_paths() {
        let fixture = test_support::TestDir::new();
        let existing = fixture.0.join("src/existing");
        fs::create_dir_all(existing.join("src")).unwrap();
        fs::write(
            existing.join("eska.toml"),
            "[project]\nname = \"existing\"\ntype = \"report\"\n",
        )
        .unwrap();
        let manifest = fixture.0.join("eska.toml");
        let original = b"# preserved\n[workspace]\nmembers = [\"src/existing\"]\n";
        fs::write(&manifest, original).unwrap();
        let super::DiscoveryContext::Workspace { workspace, .. } =
            super::discovery::discover_context(&fixture.0).unwrap()
        else {
            panic!("workspace");
        };
        let name = ProjectName::parse("new-member".to_owned()).unwrap();
        let plan = inspect_workspace_member(&workspace, name).unwrap();
        fs::remove_dir(existing.join("src")).unwrap();

        assert!(matches!(
            create_workspace_member(&plan, ProjectType::Processing),
            Err(CreationError::WorkspaceValidation(_))
        ));
        assert_eq!(fs::read(manifest).unwrap(), original);
        assert!(!fixture.0.join("src/new-member").exists());
        assert!(existing.join("eska.toml").is_file());
    }
}
