//! Creation of a new project directory; no CLI parsing or localized output.

use std::{
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use crate::config::ProjectConfig;
use std::convert::Infallible;

use super::{
    Project, ProjectName, ProjectType, Workspace,
    discovery::{self, ContextDiscoveryError, DiscoveryContext, DiscoveryError},
    onboarding::{self, WorkspaceEnrollmentError, WorkspaceMemberEnrollment},
    templates::Template,
};

/// A scoped proof that the creation transaction exclusively claimed this directory.
pub struct NewDirectory<'a> {
    path: &'a Path,
}

impl NewDirectory<'_> {
    /// Expose the owned namespace for a caller's preparation without allowing token construction.
    #[must_use]
    pub const fn path(&self) -> &Path {
        self.path
    }
}

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

/// Create a project without Git initialization or platform discovery.
///
/// # Errors
/// Returns structured filesystem, manifest, validation or rollback failures.
pub fn create(destination: &Path, config: &ProjectConfig) -> Result<Project, CreationError> {
    create_with(destination, config, |_| Ok::<(), Infallible>(()))
}

/// Keep caller-owned initialization inside the same exclusive creation transaction.
///
/// # Errors
/// Retains typed initialization errors and removes only the newly owned destination.
pub fn create_with<E>(
    destination: &Path,
    config: &ProjectConfig,
    initialize: impl FnOnce(&Path) -> Result<(), E>,
) -> Result<Project, CreationError<E>> {
    let destination = resolve_destination(destination)?;
    in_new_directory(&destination, |root| {
        publish_project(root, config, None, initialize)
    })
}

/// Finish files and caller-owned initialization only inside an exclusively owned directory.
///
/// # Errors
/// Returns source/template, initialization or final discovery failures.
pub fn publish_project<E>(
    root: &NewDirectory<'_>,
    config: &ProjectConfig,
    sources: Option<&Path>,
    initialize: impl FnOnce(&Path) -> Result<(), E>,
) -> Result<Project, CreationError<E>> {
    let root = root.path();
    let template = Template::from_config(config).map_err(CreationError::Template)?;
    write_template(root, &template)?;
    if let Some(sources) = sources {
        install_sources(root, config.source(), sources)?;
    }
    initialize(root).map_err(CreationError::Operation)?;
    discovery::discover(root).map_err(|error| CreationError::Validation(Box::new(error)))
}

/// Preflights the conventional `src/<name>` destination of a workspace member.
///
/// # Errors
/// Returns structured name, path, collision, manifest, or workspace-layout errors.
pub fn inspect_workspace_member<E>(
    workspace: &Workspace,
    name: ProjectName,
) -> Result<WorkspaceCreationPlan, CreationError<E>> {
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
pub fn create_workspace_member<E>(
    plan: &WorkspaceCreationPlan,
    project_type: ProjectType,
) -> Result<Project, CreationError<E>> {
    let config = ProjectConfig::new(project_type).with_name(plan.name.clone());
    create_member_configured(plan, &config, None, || Ok(()))
}

/// Create and enroll prepared XML sources with optional explicit platform overrides.
///
/// # Errors
/// Returns creation errors, restoring the workspace manifest on ordinary failure.
pub fn create_imported_member<E>(
    plan: &WorkspaceCreationPlan,
    config: &ProjectConfig,
    sources: &Path,
    verify: impl Fn() -> Result<(), E>,
) -> Result<Project, CreationError<E>> {
    let config = config.clone().with_name(plan.name.clone());
    create_member_configured(plan, &config, Some(sources), verify)
}

/// Publish member files and verify caller state before committing the shared workspace enrollment.
///
/// # Errors
/// Preserves unrelated files and restores the exact root manifest on ordinary failure.
pub fn create_member_configured<E>(
    plan: &WorkspaceCreationPlan,
    config: &ProjectConfig,
    sources: Option<&Path>,
    verify: impl Fn() -> Result<(), E>,
) -> Result<Project, CreationError<E>> {
    verify().map_err(CreationError::Operation)?;
    let config = config.clone().with_name(plan.name.clone());
    let config = &config;
    let template = Template::workspace_config(config).map_err(CreationError::Template)?;
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
        if let Some(sources) = sources {
            install_sources(&plan.destination, config.source(), sources)?;
        }
        verify().map_err(CreationError::Operation)?;
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
        let project = workspace
            .member(&plan.name)
            .map(|member| member.project().clone())
            .ok_or_else(|| CreationError::WorkspaceMemberMissing {
                name: plan.name.clone(),
            })?;
        verify().map_err(CreationError::Operation)?;
        Ok(project)
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

/// Replace only the empty, newly owned scaffold source directory.
fn install_sources<E>(
    root: &Path,
    relative: &Path,
    sources: &Path,
) -> Result<(), CreationError<E>> {
    let destination = root.join(relative);
    let placeholder = destination.join(".gitkeep");
    fs::remove_file(&placeholder).map_err(|source| CreationError::Io {
        path: placeholder,
        source,
    })?;
    fs::remove_dir(&destination).map_err(|source| CreationError::Io {
        path: destination.clone(),
        source,
    })?;
    fs::rename(sources, &destination).map_err(|source| CreationError::Io {
        path: destination,
        source,
    })
}

/// Materialize a template only inside a newly created directory.
fn write_template<E>(root: &Path, template: &Template) -> Result<(), CreationError<E>> {
    for directory in template.directories() {
        let path = root.join(directory);
        fs::create_dir_all(&path).map_err(|source| CreationError::Io { path, source })?;
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
pub fn resolve_destination<E>(destination: &Path) -> Result<PathBuf, CreationError<E>> {
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

/// Run preparation and publication under one exclusively claimed destination.
///
/// # Errors
/// Preserves operation errors and reports any failure to remove the owned directory.
pub fn in_new_directory<E, T>(
    destination: &Path,
    operation: impl FnOnce(&NewDirectory<'_>) -> Result<T, CreationError<E>>,
) -> Result<T, CreationError<E>> {
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
    match operation(&NewDirectory { path: destination }) {
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
pub enum CreationError<E = Infallible> {
    Operation(E),
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
