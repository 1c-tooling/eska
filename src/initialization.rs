//! Non-destructive attachment of an existing Designer XML export.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use crate::{
    config::{ProjectConfig, ProjectConfigError},
    designer,
    discovery::{self, DiscoveryError},
    project::{Project, ProjectType, WorkflowPreset},
    vcs,
};

const MAX_DESCRIPTOR_BYTES: u64 = 64 * 1024 * 1024;

/// Parameters detected without changing any project files.
#[derive(Debug, Eq, PartialEq)]
pub struct InitPlan {
    root: PathBuf,
    source: PathBuf,
    project_type: ProjectType,
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
pub enum InitError {
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
    Git(Box<gix::init::Error>),
    ExistingGit {
        path: PathBuf,
        source: Option<Box<gix::open::Error>>,
    },
    ChangedSource {
        path: PathBuf,
    },
    Validation(Box<DiscoveryError>),
    Rollback {
        paths: Vec<PathBuf>,
        original: Box<Self>,
    },
}

/// Detect a single root descriptor in `.` or `src`, or in an explicit source.
/// No recursive walk or full semantic validation is performed.
///
/// # Errors
/// Returns an error for collisions, unsafe paths, malformed/ambiguous XML or
/// missing source descriptors. All checks are read-only.
pub fn inspect(root: &Path, source: Option<&Path>) -> Result<InitPlan, InitError> {
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

fn ensure_no_config(root: &Path) -> Result<(), InitError> {
    let path = root.join("eska.toml");
    match fs::symlink_metadata(&path) {
        Ok(_) => Err(InitError::ExistingConfig { path }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(InitError::Io { path, source }),
    }
}

fn detect_directory(directory: &Path, root: &Path) -> Result<Option<ProjectType>, InitError> {
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
        let detected = designer::project_type(&input).map_err(|source| InitError::InvalidXml {
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

/// Write only a new config and, if requested and absent, a new Git repository.
/// Existing source files and repository metadata are never rewritten.
///
/// # Errors
/// Returns validation/I/O/VCS errors. Ordinary failures roll back only artifacts
/// exclusively created by this call; cleanup failures report remaining paths.
pub fn apply(
    plan: &InitPlan,
    workflow: WorkflowPreset,
    initialize_vcs: bool,
) -> Result<Project, InitError> {
    if inspect(&plan.root, Some(&plan.source))? != *plan {
        return Err(InitError::ChangedSource {
            path: plan.root.clone(),
        });
    }
    let create_git = initialize_vcs
        && !vcs::exists(&plan.root).map_err(|error| {
            // Keep the repository boundary's diagnostic attached to an inspectable
            // error without exposing its potentially unlocalized text in the CLI.
            match error {
                vcs::ExistingError::Io { path, source } => InitError::Io { path, source },
                vcs::ExistingError::Open(error) => InitError::ExistingGit {
                    path: plan.root.clone(),
                    source: Some(error),
                },
                vcs::ExistingError::Bare => InitError::ExistingGit {
                    path: plan.root.clone(),
                    source: None,
                },
            }
        })?;
    let contents = ProjectConfig::new(plan.project_type)
        .with_source(plan.source.clone())
        .map_err(InitError::Config)?
        .with_workflow(workflow)
        .to_toml()
        .map_err(InitError::Serialize)?;
    write_project(&plan.root, &contents, create_git, || {
        discovery::discover(&plan.root).map_err(|error| InitError::Validation(Box::new(error)))
    })
}

fn write_project<T>(
    root: &Path,
    contents: &str,
    create_git: bool,
    validate: impl FnOnce() -> Result<T, InitError>,
) -> Result<T, InitError> {
    let config = root.join("eska.toml");
    let mut file = OpenOptions::new()
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
    let result = (|| {
        file.write_all(contents.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|source| InitError::Io {
                path: config.clone(),
                source,
            })?;
        if create_git {
            fs::create_dir(&git).map_err(|source| InitError::Io {
                path: git.clone(),
                source,
            })?;
            owns_git = true;
            vcs::initialize_reserved(&git).map_err(|error| match error {
                vcs::InitializeError::Io { path, source } => InitError::Io { path, source },
                vcs::InitializeError::Git(error) => InitError::Git(error),
            })?;
        }
        validate()
    })();
    drop(file); // Windows cannot remove an open config during rollback.
    match result {
        Ok(value) => Ok(value),
        Err(original) => {
            let mut paths = Vec::new();
            if owns_git && fs::remove_dir_all(&git).is_err() {
                paths.push(git);
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

#[cfg(test)]
mod tests {
    use super::{InitError, write_project};
    use crate::test_support::TestDir;
    use std::fs;

    #[test]
    fn validation_failure_rolls_back_only_owned_config_and_git() {
        for with_git in [true, false] {
            let fixture = TestDir::new();
            fs::write(fixture.0.join("source.xml"), "unchanged").expect("source");
            let result = write_project::<()>(&fixture.0, "config", with_git, || {
                if with_git {
                    let repo = gix::open(&fixture.0).expect("initialized repo");
                    assert_eq!(repo.workdir(), Some(fixture.0.as_path()));
                }
                Err(InitError::ChangedSource {
                    path: fixture.0.clone(),
                })
            });
            assert!(matches!(result, Err(InitError::ChangedSource { .. })));
            assert_eq!(
                fs::read(fixture.0.join("source.xml")).expect("source"),
                b"unchanged"
            );
            assert_eq!(fs::read_dir(&fixture.0).expect("root preserved").count(), 1);
        }
    }

    #[test]
    fn late_git_collision_preserves_existing_metadata_and_removes_new_config() {
        let fixture = TestDir::new();
        fs::write(fixture.0.join(".git"), "owned by user").expect("existing metadata");
        let result = write_project(&fixture.0, "config", true, || Ok(()));
        assert!(matches!(result, Err(InitError::Io { .. })));
        assert_eq!(
            fs::read(fixture.0.join(".git")).expect("metadata"),
            b"owned by user"
        );
        assert!(!fixture.0.join("eska.toml").exists());
    }

    #[test]
    fn late_config_collision_is_not_overwritten() {
        let fixture = TestDir::new();
        fs::write(fixture.0.join("eska.toml"), "user config").expect("config");
        assert!(matches!(
            write_project(&fixture.0, "new", true, || Ok(())),
            Err(InitError::ExistingConfig { .. })
        ));
        assert_eq!(
            fs::read(fixture.0.join("eska.toml")).expect("config"),
            b"user config"
        );
        assert!(!fixture.0.join(".git").exists());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_failure_reports_remaining_artifact_and_original_error() {
        let fixture = TestDir::new();
        let config = fixture.0.join("eska.toml");
        let result = write_project::<()>(&fixture.0, "config", true, || {
            fs::remove_file(&config).expect("inject cleanup failure");
            fs::create_dir(&config).expect("block removal as a file");
            Err(InitError::ChangedSource {
                path: fixture.0.clone(),
            })
        });
        let Err(InitError::Rollback { paths, original }) = result else {
            panic!("cleanup must fail");
        };
        assert_eq!(paths, vec![config]);
        assert!(matches!(*original, InitError::ChangedSource { .. }));
        assert!(
            !fixture.0.join(".git").exists(),
            "independent cleanup still runs"
        );
    }
}
