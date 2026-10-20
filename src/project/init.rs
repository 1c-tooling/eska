//! Existing Git-aware initialization delegates source and file transactions to project core.

#[cfg(test)]
use super::templates::TemplateFile;
use super::{Project, ProjectName, Workspace, build::BuildSettings};
use crate::{
    config::ProjectConfig,
    vcs::{git, workflow::WorkflowPreset},
};
use eska_project_core::project::init as core;
use std::path::{Path, PathBuf};

pub use core::{InitPlan, WorkspaceInitPlan};

/// Git failures remain outside the shared XML/filesystem implementation.
#[derive(Debug)]
pub enum InitFailure {
    Git(Box<gix::init::Error>),
    ExistingGit {
        path: PathBuf,
        source: Option<Box<gix::open::Error>>,
    },
}

/// The native CLI retains its Git errors inside the shared transaction error type.
pub type InitError = core::InitError<InitFailure>;

/// Discover the same bounded XML identity used by standalone project initialization.
///
/// # Errors
/// Returns source, path, type or manifest failures without writing files.
pub fn inspect(root: &Path, source: Option<&Path>) -> Result<InitPlan, InitError> {
    core::inspect(root, source)
}

/// Prepare root enrollment using the shared member transaction.
///
/// # Errors
/// Returns duplicate-name, path, or root-manifest failures.
pub fn prepare_workspace_member(
    plan: InitPlan,
    workspace: &Workspace,
    name: ProjectName,
) -> Result<WorkspaceInitPlan, InitError> {
    core::prepare_workspace_member(plan, workspace, name)
}

/// Attach a member without nested VCS metadata.
///
/// # Errors
/// Rolls back only this call's files and exact root enrollment after an ordinary failure.
pub fn apply_workspace_member(plan: &WorkspaceInitPlan) -> Result<Project, InitError> {
    core::apply_workspace_member(plan)
}

/// Keep repository preflight before writes and execute initialization inside the shared transaction.
///
/// # Errors
/// Returns source, Git, I/O, or rollback failures without overwriting user files.
pub fn apply(
    plan: &InitPlan,
    workflow: WorkflowPreset,
    build: BuildSettings,
    initialize_vcs: bool,
) -> Result<Project, InitError> {
    if inspect(plan.root(), Some(plan.source()))? != *plan {
        return Err(InitError::ChangedSource {
            path: plan.root().into(),
        });
    }
    let create_git = initialize_vcs
        && !git::exists(plan.root()).map_err(|error| match error {
            git::ExistingError::Io { path, source } => InitError::Io { path, source },
            git::ExistingError::Open(source) => InitError::Operation(InitFailure::ExistingGit {
                path: plan.root().into(),
                source: Some(source),
            }),
            git::ExistingError::Bare => InitError::Operation(InitFailure::ExistingGit {
                path: plan.root().into(),
                source: None,
            }),
        })?;
    let config = ProjectConfig::new(plan.project_type())
        .with_source(plan.source().into())
        .map_err(InitError::Config)?
        .with_workflow(workflow)
        .with_build_settings(build);
    core::apply_configured(plan, &config, create_git, initialize_reserved)
}

/// Only the VCS boundary knows how to populate the core's exclusively reserved repository directory.
fn initialize_reserved(path: &Path) -> Result<(), InitError> {
    git::initialize_reserved(path).map_err(|error| match error {
        git::InitializeError::Io { path, source } => InitError::Io { path, source },
        git::InitializeError::Git(error) => InitError::Operation(InitFailure::Git(error)),
    })
}

/// Keep the existing native rollback scenarios on the same filesystem writer used by project core.
#[cfg(test)]
fn write_project<T>(
    root: &Path,
    contents: &str,
    files: &[TemplateFile],
    create_git: bool,
    validate: impl FnOnce() -> Result<T, InitError>,
) -> Result<T, InitError> {
    core::write_project(
        root,
        contents,
        files,
        create_git,
        initialize_reserved,
        validate,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        InitError, apply_workspace_member, inspect, prepare_workspace_member, write_project,
    };
    use crate::{
        project::{
            ProjectName,
            discovery::{DiscoveryContext, discover_context},
            templates,
        },
        test_support::TestDir,
    };
    use std::fs;

    #[test]
    fn validation_failure_rolls_back_only_owned_config_and_git() {
        for with_git in [true, false] {
            let fixture = TestDir::new();
            fs::write(fixture.0.join("source.xml"), "unchanged").expect("source");
            let project_files = templates::built_in_git_files();
            let result =
                write_project::<()>(&fixture.0, "config", &project_files, with_git, || {
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
        let project_files = templates::built_in_git_files();
        let result = write_project(&fixture.0, "config", &project_files, true, || Ok(()));
        assert!(matches!(result, Err(InitError::Io { .. })));
        assert_eq!(
            fs::read(fixture.0.join(".git")).expect("metadata"),
            b"owned by user"
        );
        assert!(!fixture.0.join("eska.toml").exists());
        assert!(!fixture.0.join(".gitattributes").exists());
        assert!(!fixture.0.join(".gitignore").exists());
    }

    #[test]
    fn late_config_collision_is_not_overwritten() {
        let fixture = TestDir::new();
        fs::write(fixture.0.join("eska.toml"), "user config").expect("config");
        assert!(matches!(
            write_project(
                &fixture.0,
                "new",
                &templates::built_in_git_files(),
                true,
                || Ok(())
            ),
            Err(InitError::ExistingConfig { .. })
        ));
        assert_eq!(
            fs::read(fixture.0.join("eska.toml")).expect("config"),
            b"user config"
        );
        assert!(!fixture.0.join(".git").exists());
    }

    #[test]
    fn workspace_validation_failure_restores_both_manifests_only() {
        let fixture = TestDir::new();
        let existing = fixture.0.join("src/existing");
        fs::create_dir_all(existing.join("src")).unwrap();
        fs::write(
            existing.join("eska.toml"),
            "[project]\nname = \"existing\"\ntype = \"report\"\n",
        )
        .unwrap();
        let target = fixture.0.join("src/copied");
        fs::create_dir_all(target.join("src")).unwrap();
        let descriptor = target.join("src/Обработка.xml");
        fs::write(
            &descriptor,
            "<?xml version=\"1.0\"?><MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><ExternalDataProcessor uuid=\"12345678-1234-1234-1234-123456789012\"><Properties><Name>Copied</Name></Properties><ChildObjects/></ExternalDataProcessor></MetaDataObject>",
        )
        .unwrap();
        let manifest = fixture.0.join("eska.toml");
        let original = b"# preserved\n[workspace]\nmembers = [\"src/existing\"]\n";
        fs::write(&manifest, original).unwrap();
        let DiscoveryContext::Workspace { workspace, .. } = discover_context(&fixture.0).unwrap()
        else {
            panic!("workspace");
        };
        let detected = inspect(&target, None).unwrap();
        let plan = prepare_workspace_member(
            detected,
            &workspace,
            ProjectName::parse("copied".to_owned()).unwrap(),
        )
        .unwrap();
        fs::remove_dir(existing.join("src")).unwrap();

        assert!(matches!(
            apply_workspace_member(&plan),
            Err(InitError::WorkspaceValidation(_))
        ));
        assert_eq!(fs::read(manifest).unwrap(), original);
        assert!(!target.join("eska.toml").exists());
        assert!(descriptor.is_file());
        assert!(existing.join("eska.toml").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_failure_reports_remaining_artifact_and_original_error() {
        let fixture = TestDir::new();
        let config = fixture.0.join("eska.toml");
        let project_files = templates::built_in_git_files();
        let result = write_project::<()>(&fixture.0, "config", &project_files, true, || {
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
        assert!(!fixture.0.join(".gitattributes").exists());
        assert!(!fixture.0.join(".gitignore").exists());
    }
}
