//! Non-destructive attachment of an existing Designer XML export.

use std::{
    convert::Infallible,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use crate::config::{FILE_NAME, ProjectConfig, ProjectConfigError};

use super::{
    Project, ProjectName, ProjectType, Workspace,
    build::BuildSettings,
    designer_xml,
    discovery::{self, ContextDiscoveryError, DiscoveryContext, DiscoveryError},
    onboarding::{self, WorkspaceEnrollmentError, WorkspaceMemberEnrollment},
    templates::{self, TemplateFile},
};

const MAX_DESCRIPTOR_BYTES: u64 = 64 * 1024 * 1024;

/// Parameters detected without changing any project files.
#[derive(Debug, Eq, PartialEq)]
pub struct InitPlan {
    root: PathBuf,
    source: PathBuf,
    project_type: ProjectType,
}

/// A fully preflighted attachment of an existing directory to a workspace.
#[derive(Debug)]
pub struct WorkspaceInitPlan {
    detected: InitPlan,
    name: ProjectName,
    config: ProjectConfig,
    enrollment: WorkspaceMemberEnrollment,
}

impl WorkspaceInitPlan {
    /// Save an explicit platform override while retaining root-owned workspace settings.
    #[must_use]
    pub fn with_build_settings(mut self, build: BuildSettings) -> Self {
        self.config = self.config.with_build_settings(build);
        self
    }
}

impl InitPlan {
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }
    #[must_use]
    pub const fn project_type(&self) -> ProjectType {
        self.project_type
    }
}

/// Structured failures; presentation belongs to the CLI.
#[derive(Debug)]
pub enum InitError<E = Infallible> {
    Operation(E),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    InvalidRoot {
        path: PathBuf,
    },
    ExistingConfig {
        path: PathBuf,
    },
    InvalidSource {
        path: PathBuf,
    },
    MissingSource {
        path: PathBuf,
    },
    AmbiguousSource {
        path: PathBuf,
    },
    MultipleDescriptors {
        path: PathBuf,
    },
    InvalidDescriptor {
        path: PathBuf,
    },
    InvalidXml {
        path: PathBuf,
        source: roxmltree::Error,
    },
    DescriptorTooLarge {
        path: PathBuf,
    },
    Config(ProjectConfigError),
    Serialize(toml::ser::Error),
    ChangedSource {
        path: PathBuf,
    },
    Validation(Box<DiscoveryError>),
    Workspace(WorkspaceEnrollmentError),
    WorkspaceValidation(Box<ContextDiscoveryError>),
    WorkspaceMemberMissing {
        name: ProjectName,
    },
    Rollback {
        paths: Vec<PathBuf>,
        original: Box<Self>,
    },
}

/// Prepares a member manifest and root enrollment without writing either file.
///
/// # Errors
/// Returns workspace-layout, duplicate-name, or manifest errors.
pub fn prepare_workspace_member<E>(
    plan: InitPlan,
    workspace: &Workspace,
    name: ProjectName,
) -> Result<WorkspaceInitPlan, InitError<E>> {
    let member_path = plan
        .root
        .strip_prefix(workspace.root())
        .map_err(|_| {
            InitError::Workspace(WorkspaceEnrollmentError::OutsideWorkspace {
                workspace: workspace.root().to_path_buf(),
                member: plan.root.clone(),
            })
        })?
        .to_path_buf();
    let config = ProjectConfig::new(plan.project_type)
        .with_name(name.clone())
        .with_source(plan.source.clone())
        .map_err(InitError::Config)?;
    let enrollment = onboarding::prepare(workspace, plan.root.clone(), member_path, &name)
        .map_err(InitError::Workspace)?;
    Ok(WorkspaceInitPlan {
        detected: plan,
        name,
        config,
        enrollment,
    })
}

/// Attaches an existing export and enrolls it without nested Git or workflow files.
///
/// # Errors
/// Rolls back the new member manifest and exact root manifest bytes on failure.
pub fn apply_workspace_member<E>(plan: &WorkspaceInitPlan) -> Result<Project, InitError<E>> {
    if inspect(&plan.detected.root, Some(&plan.detected.source))? != plan.detected {
        return Err(InitError::ChangedSource {
            path: plan.detected.root.clone(),
        });
    }
    let contents = plan
        .config
        .to_workspace_member_toml()
        .map_err(InitError::Serialize)?;
    let config_path = plan.detected.root.join(FILE_NAME);
    let mut config_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config_path)
        .map_err(|source| {
            if source.kind() == io::ErrorKind::AlreadyExists {
                InitError::ExistingConfig {
                    path: config_path.clone(),
                }
            } else {
                InitError::Io {
                    path: config_path.clone(),
                    source,
                }
            }
        })?;
    let mut manifest_published = false;
    let result = (|| {
        config_file
            .write_all(contents.as_bytes())
            .and_then(|()| config_file.sync_all())
            .map_err(|source| InitError::Io {
                path: config_path.clone(),
                source,
            })?;
        plan.enrollment.publish().map_err(InitError::Workspace)?;
        manifest_published = true;
        let context = discovery::discover_context(plan.enrollment.workspace_root())
            .map_err(|error| InitError::WorkspaceValidation(Box::new(error)))?;
        let DiscoveryContext::Workspace { workspace, .. } = context else {
            return Err(InitError::WorkspaceMemberMissing {
                name: plan.name.clone(),
            });
        };
        workspace
            .member(&plan.name)
            .map(|member| member.project().clone())
            .ok_or_else(|| InitError::WorkspaceMemberMissing {
                name: plan.name.clone(),
            })
    })();
    drop(config_file);
    if let Ok(project) = result {
        return Ok(project);
    }
    let original = result.expect_err("handled successful result");
    let mut paths = Vec::new();
    if manifest_published && plan.enrollment.restore().is_err() {
        paths.push(plan.enrollment.workspace_root().join(FILE_NAME));
    }
    if fs::remove_file(&config_path).is_err() {
        paths.push(config_path);
    }
    if paths.is_empty() {
        Err(original)
    } else {
        Err(InitError::Rollback {
            paths,
            original: Box::new(original),
        })
    }
}

/// Detect a single root descriptor in `.` or `src`, or in an explicit source.
/// No recursive walk or full semantic validation is performed.
///
/// # Errors
/// Returns an error for collisions, unsafe paths, malformed/ambiguous XML or
/// missing source descriptors. All checks are read-only.
pub fn inspect<E>(root: &Path, source: Option<&Path>) -> Result<InitPlan, InitError<E>> {
    let root = fs::canonicalize(root).map_err(|source| InitError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    if !root.is_dir() {
        return Err(InitError::InvalidRoot { path: root });
    }
    ensure_no_config(&root)?;
    let candidates = source.map_or_else(
        || vec![Path::new("."), Path::new("src")],
        |source| vec![source],
    );
    let mut found = None;
    for relative in candidates {
        // Reuse config path rules before canonicalization can erase traversal.
        ProjectConfig::new(ProjectType::Configuration)
            .with_source(relative.to_path_buf())
            .map_err(InitError::Config)?;
        let directory = root.join(relative);
        match fs::symlink_metadata(&directory) {
            Err(error) if source.is_none() && error.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(InitError::Io {
                    path: directory,
                    source,
                });
            }
            Ok(_) => {}
        }
        let directory = fs::canonicalize(&directory).map_err(|source| InitError::Io {
            path: directory,
            source,
        })?;
        if !directory.is_dir() || !directory.starts_with(&root) {
            return Err(InitError::InvalidSource { path: directory });
        }
        if let Some(project_type) = detect_directory(&directory, &root)? {
            let relative = directory
                .strip_prefix(&root)
                .map_err(|_| InitError::InvalidSource {
                    path: directory.clone(),
                })?;
            let relative = if relative.as_os_str().is_empty() {
                Path::new(".")
            } else {
                relative
            };
            let plan = InitPlan {
                root: root.clone(),
                source: relative.to_path_buf(),
                project_type,
            };
            if found.as_ref().is_some_and(|previous| previous != &plan) {
                return Err(InitError::AmbiguousSource { path: root });
            }
            found = Some(plan);
        }
    }
    found.ok_or(InitError::MissingSource { path: root })
}

fn ensure_no_config<E>(root: &Path) -> Result<(), InitError<E>> {
    let path = root.join(FILE_NAME);
    match fs::symlink_metadata(&path) {
        Ok(_) => Err(InitError::ExistingConfig { path }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(InitError::Io { path, source }),
    }
}

fn detect_directory<E>(directory: &Path, root: &Path) -> Result<Option<ProjectType>, InitError<E>> {
    let mut found = None;
    for entry in fs::read_dir(directory).map_err(|source| InitError::Io {
        path: directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| InitError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|value| value.eq_ignore_ascii_case("xml"))
            || entry.file_name() == "ConfigDumpInfo.xml"
        {
            continue;
        }
        let resolved = fs::canonicalize(&path).map_err(|source| InitError::Io {
            path: path.clone(),
            source,
        })?;
        if !resolved.starts_with(root) || !resolved.is_file() {
            return Err(InitError::InvalidSource { path });
        }
        let mut input = String::new();
        File::open(&resolved)
            .and_then(|file| {
                file.take(MAX_DESCRIPTOR_BYTES + 1)
                    .read_to_string(&mut input)
            })
            .map_err(|source| InitError::Io {
                path: path.clone(),
                source,
            })?;
        if input.len() as u64 > MAX_DESCRIPTOR_BYTES {
            return Err(InitError::DescriptorTooLarge { path });
        }
        let detected =
            designer_xml::project_type(&input).map_err(|source| InitError::InvalidXml {
                path: path.clone(),
                source,
            })?;
        if let Some(kind) = detected {
            if found.replace(kind).is_some() {
                return Err(InitError::MultipleDescriptors {
                    path: directory.to_path_buf(),
                });
            }
        } else if entry.file_name() == "Configuration.xml" {
            return Err(InitError::InvalidDescriptor { path });
        }
    }
    Ok(found)
}

/// Attach existing XML without requiring Git or platform discovery.
///
/// # Errors
/// Refuses stale plans or mismatched settings and rolls back only exclusively owned files.
pub fn apply(plan: &InitPlan, config: &ProjectConfig) -> Result<Project, InitError> {
    apply_configured(plan, config, false, |_| Ok(()))
}

/// Keep a caller-owned repository initializer inside the same filesystem transaction.
///
/// # Errors
/// Returns validation, serialization, initialization or exact rollback failures.
pub fn apply_configured<E>(
    plan: &InitPlan,
    config: &ProjectConfig,
    reserve_repository: bool,
    initialize: impl FnOnce(&Path) -> Result<(), InitError<E>>,
) -> Result<Project, InitError<E>> {
    if inspect(&plan.root, Some(&plan.source))? != *plan
        || config.configuration().project_type() != plan.project_type
        || config.source() != plan.source
    {
        return Err(InitError::ChangedSource {
            path: plan.root.clone(),
        });
    }
    let contents = config.to_toml().map_err(InitError::Serialize)?;
    let project_files = templates::built_in_git_files();
    write_project(
        &plan.root,
        &contents,
        &project_files,
        reserve_repository,
        initialize,
        || discovery::discover(&plan.root).map_err(|error| InitError::Validation(Box::new(error))),
    )
}

/// Publish only new control files, with optional caller-owned initialization of a reserved directory.
///
/// # Errors
/// Preserves every pre-existing path and retains the original failure if cleanup fails.
pub fn write_project<T, E>(
    root: &Path,
    contents: &str,
    project_files: &[TemplateFile],
    create_git: bool,
    initialize: impl FnOnce(&Path) -> Result<(), InitError<E>>,
    validate: impl FnOnce() -> Result<T, InitError<E>>,
) -> Result<T, InitError<E>> {
    let config = root.join(FILE_NAME);
    let mut config_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config)
        .map_err(|source| {
            if source.kind() == io::ErrorKind::AlreadyExists {
                InitError::ExistingConfig {
                    path: config.clone(),
                }
            } else {
                InitError::Io {
                    path: config.clone(),
                    source,
                }
            }
        })?;
    let git = root.join(".git");
    let mut owns_git = false;
    let mut owned_project_files = Vec::new();
    let result = (|| {
        config_file
            .write_all(contents.as_bytes())
            .and_then(|()| config_file.sync_all())
            .map_err(|source| InitError::Io {
                path: config.clone(),
                source,
            })?;
        write_missing_project_files(root, project_files, &mut owned_project_files)?;
        if create_git {
            fs::create_dir(&git).map_err(|source| InitError::Io {
                path: git.clone(),
                source,
            })?;
            owns_git = true;
            initialize(&git)?;
        }
        validate()
    })();
    drop(config_file); // Windows cannot remove an open config during rollback.
    match result {
        Ok(value) => Ok(value),
        Err(original) => {
            let mut paths = Vec::new();
            if owns_git && fs::remove_dir_all(&git).is_err() {
                paths.push(git);
            }
            for path in owned_project_files.into_iter().rev() {
                if fs::remove_file(&path).is_err() {
                    paths.push(path);
                }
            }
            if fs::remove_file(&config).is_err() {
                paths.push(config);
            }
            if paths.is_empty() {
                Err(original)
            } else {
                Err(InitError::Rollback {
                    paths,
                    original: Box::new(original),
                })
            }
        }
    }
}

/// Creates absent built-in project files while preserving every existing path.
fn write_missing_project_files<E>(
    root: &Path,
    project_files: &[TemplateFile],
    owned_paths: &mut Vec<PathBuf>,
) -> Result<(), InitError<E>> {
    for entry in project_files {
        let path = root.join(entry.path());
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(InitError::Io { path, source }),
        };
        owned_paths.push(path.clone());
        file.write_all(entry.contents().as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|source| InitError::Io { path, source })?;
    }
    Ok(())
}
