//! Filesystem-backed project discovery, independent of CLI presentation.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::config::{
    FILE_NAME, ManifestConfig, ManifestConfigError, ProjectConfig, ProjectConfigError,
    WorkspaceConfig,
};

use super::{
    Project, ProjectConfiguration, ProjectName, Workspace, WorkspaceMember,
    build::{BuildSettings, BuildSettingsError},
};

/// The validated project context found from a starting directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryContext {
    Standalone(Project),
    Workspace {
        workspace: Workspace,
        current_member: Option<ProjectName>,
    },
}

/// Finds and validates either a standalone project or its containing workspace.
///
/// A workspace root validates every explicitly listed member. Starting within a
/// member additionally records that member as the current selection context.
///
/// # Errors
/// Returns a structured project, manifest, member layout, or inheritance error.
pub fn discover_context(start: &Path) -> Result<DiscoveryContext, ContextDiscoveryError> {
    let start = canonicalize(start).map_err(ContextDiscoveryError::Project)?;
    if !metadata(&start)
        .map_err(ContextDiscoveryError::Project)?
        .is_dir()
    {
        return Err(ContextDiscoveryError::Project(
            DiscoveryError::StartNotDirectory { path: start },
        ));
    }

    let (nearest_root, nearest_path, nearest) = find_nearest_manifest(&start)?;
    match nearest {
        ManifestConfig::Workspace(config) => {
            load_workspace(&nearest_root, &config).map(|workspace| DiscoveryContext::Workspace {
                workspace,
                current_member: None,
            })
        }
        ManifestConfig::Project(config) => {
            for ancestor in nearest_root.parent().into_iter().flat_map(Path::ancestors) {
                let path = ancestor.join(FILE_NAME);
                match fs::symlink_metadata(&path) {
                    Ok(_) => {
                        let manifest = load_manifest(&path)?;
                        return match manifest {
                            ManifestConfig::Project(_) => {
                                load_project_config(&nearest_root, &nearest_path, config)
                                    .map(DiscoveryContext::Standalone)
                                    .map_err(ContextDiscoveryError::Project)
                            }
                            ManifestConfig::Workspace(workspace_config) => {
                                let workspace = load_workspace(ancestor, &workspace_config)?;
                                let current_member = workspace
                                    .members()
                                    .iter()
                                    .find(|member| member.root() == nearest_root)
                                    .map(|member| member.name().clone())
                                    .ok_or_else(|| ContextDiscoveryError::UnlistedProject {
                                        project: nearest_root.clone(),
                                        workspace: ancestor.to_path_buf(),
                                    })?;
                                Ok(DiscoveryContext::Workspace {
                                    workspace,
                                    current_member: Some(current_member),
                                })
                            }
                        };
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(source) => {
                        return Err(ContextDiscoveryError::Project(DiscoveryError::Io {
                            path,
                            source,
                        }));
                    }
                }
            }
            load_project_config(&nearest_root, &nearest_path, config)
                .map(DiscoveryContext::Standalone)
                .map_err(ContextDiscoveryError::Project)
        }
    }
}

fn find_nearest_manifest(
    start: &Path,
) -> Result<(PathBuf, PathBuf, ManifestConfig), ContextDiscoveryError> {
    for root in start.ancestors() {
        let path = root.join(FILE_NAME);
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                let manifest = load_manifest(&path)?;
                return Ok((root.to_path_buf(), path, manifest));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(ContextDiscoveryError::Project(DiscoveryError::Io {
                    path,
                    source,
                }));
            }
        }
    }
    Err(ContextDiscoveryError::Project(DiscoveryError::NotFound {
        start: start.to_path_buf(),
    }))
}

fn load_manifest(path: &Path) -> Result<ManifestConfig, ContextDiscoveryError> {
    if !metadata(path)
        .map_err(ContextDiscoveryError::Project)?
        .is_file()
    {
        return Err(ContextDiscoveryError::Project(
            DiscoveryError::ConfigNotFile {
                path: path.to_path_buf(),
            },
        ));
    }
    ManifestConfig::load(path).map_err(|source| ContextDiscoveryError::Manifest {
        path: path.to_path_buf(),
        source,
    })
}

fn load_workspace(
    root: &Path,
    config: &WorkspaceConfig,
) -> Result<Workspace, ContextDiscoveryError> {
    let mut members: Vec<WorkspaceMember> = Vec::with_capacity(config.members().len());
    for configured_path in config.members() {
        let member_root = validate_member_root(root, configured_path, &members)?;
        let member = load_workspace_member(config, member_root)?;
        if members
            .iter()
            .any(|existing| existing.name() == member.name())
        {
            return Err(ContextDiscoveryError::DuplicateName {
                name: member.name().clone(),
            });
        }
        members.push(member);
    }

    Ok(Workspace::new(
        root.to_path_buf(),
        config.build_settings().clone(),
        config.workflow_settings().cloned(),
        members,
    ))
}

fn validate_member_root(
    workspace_root: &Path,
    configured_path: &Path,
    existing_members: &[WorkspaceMember],
) -> Result<PathBuf, ContextDiscoveryError> {
    let unresolved_root = workspace_root.join(configured_path);
    if !metadata(&unresolved_root)
        .map_err(ContextDiscoveryError::Project)?
        .is_dir()
    {
        return Err(ContextDiscoveryError::MemberNotDirectory {
            path: unresolved_root,
        });
    }
    let member_root = canonicalize(&unresolved_root).map_err(ContextDiscoveryError::Project)?;
    if !member_root.starts_with(workspace_root) {
        return Err(ContextDiscoveryError::MemberOutsideWorkspace {
            workspace: workspace_root.to_path_buf(),
            member: member_root,
        });
    }
    for existing in existing_members {
        if existing.root() == member_root {
            return Err(ContextDiscoveryError::DuplicateMember { path: member_root });
        }
        if existing.root().starts_with(&member_root) || member_root.starts_with(existing.root()) {
            return Err(ContextDiscoveryError::NestedMembers {
                first: existing.root().to_path_buf(),
                second: member_root,
            });
        }
    }
    Ok(member_root)
}

fn load_workspace_member(
    workspace_config: &WorkspaceConfig,
    member_root: PathBuf,
) -> Result<WorkspaceMember, ContextDiscoveryError> {
    let manifest_path = member_root.join(FILE_NAME);
    let member_config = match load_manifest(&manifest_path)? {
        ManifestConfig::Project(config) => config,
        ManifestConfig::Workspace(_) => {
            return Err(ContextDiscoveryError::MemberManifestNotProject {
                path: manifest_path,
            });
        }
    };
    let name =
        member_config
            .name()
            .cloned()
            .ok_or_else(|| ContextDiscoveryError::MemberNameMissing {
                path: manifest_path.clone(),
            })?;
    if member_config.configuration().workflow_settings().is_some() {
        return Err(ContextDiscoveryError::MemberWorkflowUnsupported {
            path: manifest_path,
        });
    }
    if member_config.overrides_artifacts_directory() {
        return Err(ContextDiscoveryError::MemberArtifactsDirectoryUnsupported {
            path: manifest_path,
        });
    }

    let configuration = resolve_member_configuration(workspace_config, &member_config)?;
    let source = member_root.join(member_config.source());
    let project = Project::new(member_root.clone(), source, configuration)
        .map_err(ProjectConfigError::ProjectPath)
        .map_err(|source| {
            ContextDiscoveryError::Project(DiscoveryError::Config {
                path: member_root.join(FILE_NAME),
                source,
            })
        })?;
    if !metadata(project.source())
        .map_err(ContextDiscoveryError::Project)?
        .is_dir()
    {
        return Err(ContextDiscoveryError::Project(
            DiscoveryError::SourceNotDirectory {
                path: project.source().to_path_buf(),
            },
        ));
    }
    let source = canonicalize(project.source()).map_err(ContextDiscoveryError::Project)?;
    let project = Project::new(member_root, source, project.configuration().clone())
        .map_err(ProjectConfigError::ProjectPath)
        .map_err(|source| {
            ContextDiscoveryError::Project(DiscoveryError::Config {
                path: manifest_path,
                source,
            })
        })?;
    Ok(WorkspaceMember::new(name, project))
}

fn resolve_member_configuration(
    workspace: &WorkspaceConfig,
    member: &ProjectConfig,
) -> Result<ProjectConfiguration, ContextDiscoveryError> {
    let version = member
        .platform_version_override()
        .or_else(|| {
            workspace
                .build_settings()
                .platform_version()
                .map(crate::project::build::PlatformVersion::as_str)
        })
        .unwrap_or_default();
    let build = BuildSettings::new(
        version,
        workspace
            .build_settings()
            .artifacts_directory()
            .to_path_buf(),
    )
    .map_err(ContextDiscoveryError::ResolvedBuild)?;
    let mut configuration = ProjectConfiguration::new(
        member.configuration().project_type(),
        member.configuration().source_format(),
    )
    .with_build_settings(build);
    if let Some(workflow) = workspace.workflow_settings() {
        configuration = configuration.with_workflow_settings(workflow.clone());
    }
    Ok(configuration)
}

/// Finds and validates the nearest project starting from a directory.
///
/// Relative paths are resolved against the current working directory. Symlinks
/// are resolved before walking ancestors and before checking source containment.
/// The first configuration encountered is authoritative, even when invalid.
///
/// # Errors
///
/// Returns a structured error for an inaccessible or invalid directory, a missing
/// project, invalid configuration, or sources outside the project root.
pub fn discover(start: &Path) -> Result<Project, DiscoveryError> {
    let start = canonicalize(start)?;
    if !metadata(&start)?.is_dir() {
        return Err(DiscoveryError::StartNotDirectory { path: start });
    }

    for root in start.ancestors() {
        let path = root.join(FILE_NAME);
        // A dangling link is a broken nearest config, not permission to fall back
        // to another project higher in the directory tree.
        match fs::symlink_metadata(&path) {
            Ok(_) => return load_project(root, &path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(DiscoveryError::Io { path, source }),
        }
    }

    Err(DiscoveryError::NotFound { start })
}

fn load_project(root: &Path, path: &Path) -> Result<Project, DiscoveryError> {
    // Reject directories and special files before reading them (e.g. FIFOs).
    if !metadata(path)?.is_file() {
        return Err(DiscoveryError::ConfigNotFile {
            path: path.to_path_buf(),
        });
    }

    let config_error = |source| DiscoveryError::Config {
        path: path.to_path_buf(),
        source,
    };
    let config = ProjectConfig::load(path).map_err(config_error)?;
    load_project_config(root, path, config)
}

fn load_project_config(
    root: &Path,
    path: &Path,
    config: ProjectConfig,
) -> Result<Project, DiscoveryError> {
    let config_error = |source| DiscoveryError::Config {
        path: path.to_path_buf(),
        source,
    };
    let project = config
        .into_project(root.to_path_buf())
        .map_err(config_error)?;

    if !metadata(project.source())?.is_dir() {
        return Err(DiscoveryError::SourceNotDirectory {
            path: project.source().to_path_buf(),
        });
    }
    let source = canonicalize(project.source())?;
    Project::new(root.to_path_buf(), source, project.configuration().clone())
        .map_err(ProjectConfigError::ProjectPath)
        .map_err(config_error)
}

fn canonicalize(path: &Path) -> Result<PathBuf, DiscoveryError> {
    fs::canonicalize(path).map_err(|source| DiscoveryError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn metadata(path: &Path) -> Result<fs::Metadata, DiscoveryError> {
    fs::metadata(path).map_err(|source| DiscoveryError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Structured discovery failures; localized text belongs to the CLI layer.
#[derive(Debug)]
pub enum DiscoveryError {
    NotFound {
        start: PathBuf,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    StartNotDirectory {
        path: PathBuf,
    },
    ConfigNotFile {
        path: PathBuf,
    },
    SourceNotDirectory {
        path: PathBuf,
    },
    Config {
        path: PathBuf,
        source: ProjectConfigError,
    },
}

/// Structured discovery failures specific to project workspaces.
#[derive(Debug)]
pub enum ContextDiscoveryError {
    Project(DiscoveryError),
    Manifest {
        path: PathBuf,
        source: ManifestConfigError,
    },
    MemberNotDirectory {
        path: PathBuf,
    },
    MemberOutsideWorkspace {
        workspace: PathBuf,
        member: PathBuf,
    },
    DuplicateMember {
        path: PathBuf,
    },
    NestedMembers {
        first: PathBuf,
        second: PathBuf,
    },
    MemberManifestNotProject {
        path: PathBuf,
    },
    MemberNameMissing {
        path: PathBuf,
    },
    DuplicateName {
        name: ProjectName,
    },
    MemberWorkflowUnsupported {
        path: PathBuf,
    },
    MemberArtifactsDirectoryUnsupported {
        path: PathBuf,
    },
    UnlistedProject {
        project: PathBuf,
        workspace: PathBuf,
    },
    ResolvedBuild(BuildSettingsError),
}
