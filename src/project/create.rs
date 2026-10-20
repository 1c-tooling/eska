//! Native creation orchestration; filesystem transactions belong to the shared project core.

use super::{
    Project, ProjectName, ProjectType, Workspace,
    artifact::{ArtifactError, PreparedArtifact},
};
use crate::{config::ProjectConfig, vcs::workflow::WorkflowPreset};
use eska_project_core::project::create as core;
use std::path::{Path, PathBuf};

pub use core::WorkspaceCreationPlan;

/// Native failures remain typed in the caller without adding Git/platform dependencies to core.
#[derive(Debug)]
pub enum CreationFailure {
    Artifact(Box<ArtifactError>),
    Vcs(eska_tool_runtime::Error),
}

/// The existing CLI adds native failures to the shared filesystem transaction's errors.
pub type CreationError = core::CreationError<CreationFailure>;

/// Keep legacy workflow settings and optional Git initialization on the shared creation path.
///
/// # Errors
/// Returns configuration, Git, I/O or rollback failures.
pub fn create(
    destination: &Path,
    project_type: ProjectType,
    workflow: WorkflowPreset,
    initialize_vcs: bool,
) -> Result<Project, CreationError> {
    let destination = resolve_destination(destination)?;
    let vcs = preflight_vcs(initialize_vcs)?;
    let config = ProjectConfig::new(project_type).with_workflow(workflow);
    core::create_with(&destination, &config, |root| {
        initialize(root, vcs.as_deref())
    })
}

/// Claim the destination before native unpacking and preserve the prepared source owner until publication.
///
/// # Errors
/// Retains preparation failures and rolls back only the newly claimed directory.
pub fn create_imported(
    destination: &Path,
    initialize_vcs: bool,
    prepare: impl FnOnce(&Path) -> Result<(ProjectConfig, PreparedArtifact), CreationError>,
) -> Result<Project, CreationError> {
    let destination = resolve_destination(destination)?;
    let vcs = preflight_vcs(initialize_vcs)?;
    in_new_directory(&destination, |root| {
        let (config, artifact) = prepare(root.path())?;
        publish_project(root, &config, vcs.as_deref(), Some(&artifact.sources()))
    })
}

/// Preflight the same member destination used by the standalone project tool.
///
/// # Errors
/// Returns shared path, name or workspace validation failures.
pub fn inspect_workspace_member(
    workspace: &Workspace,
    name: ProjectName,
) -> Result<WorkspaceCreationPlan, CreationError> {
    core::inspect_workspace_member(workspace, name)
}

/// Create a member without introducing Git or platform execution into its filesystem writer.
///
/// # Errors
/// Rolls back both owned files and root enrollment after ordinary failure.
pub fn create_workspace_member(
    plan: &WorkspaceCreationPlan,
    project_type: ProjectType,
) -> Result<Project, CreationError> {
    core::create_workspace_member(plan, project_type)
}

/// Publish native XML using the same member transaction as ordinary project creation.
///
/// # Errors
/// Returns native source publication or enrollment failures.
pub fn create_imported_member(
    plan: &WorkspaceCreationPlan,
    config: &ProjectConfig,
    artifact: &PreparedArtifact,
) -> Result<Project, CreationError> {
    core::create_imported_member(plan, config, &artifact.sources())
}

/// Preserve legacy preflight before interactive prompts.
///
/// # Errors
/// Rejects invalid, missing-parent or existing destinations.
pub fn resolve_destination(destination: &Path) -> Result<PathBuf, CreationError> {
    core::resolve_destination(destination)
}

/// Resolve every required component before claiming or writing the destination.
fn preflight_vcs(enabled: bool) -> Result<Option<PathBuf>, CreationError> {
    enabled
        .then(|| eska_tool_runtime::resolve(eska_tool_runtime::Component::Vcs))
        .transpose()
        .map_err(|error| CreationError::Operation(CreationFailure::Vcs(error)))
}

/// Keep the VCS process inside the core's exclusive directory transaction.
fn initialize(root: &Path, binary: Option<&Path>) -> Result<(), CreationFailure> {
    binary.map_or(Ok(()), |binary| {
        eska_tool_runtime::vcs::initialize_new(binary, root).map_err(CreationFailure::Vcs)
    })
}

/// Share publication with imported creation and native rollback tests.
fn publish_project(
    root: &core::NewDirectory<'_>,
    config: &ProjectConfig,
    vcs: Option<&Path>,
    sources: Option<&Path>,
) -> Result<Project, CreationError> {
    core::publish_project(root, config, sources, |root| initialize(root, vcs))
}

/// Share exclusive directory ownership with preparation and native rollback tests.
fn in_new_directory(
    destination: &Path,
    operation: impl FnOnce(&core::NewDirectory<'_>) -> Result<Project, CreationError>,
) -> Result<Project, CreationError> {
    core::in_new_directory(destination, operation)
}

#[cfg(test)]
use super::discovery::{self, DiscoveryContext};
#[cfg(test)]
use std::fs;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    /// Preparation owns the new root before unpacking and its failure removes all auxiliaries.
    #[test]
    fn imported_preparation_failure_removes_the_claimed_project() {
        let fixture = test_support::TestDir::new();
        let sentinel = fixture.0.join("user-file");
        fs::write(&sentinel, "preserved").unwrap();
        let destination = fixture.0.join("new-project");
        let result = create_imported(&destination, false, |root| {
            assert!(root.is_dir());
            assert_eq!(root, destination);
            fs::create_dir_all(root.join(".eska/import/partial")).unwrap();
            fs::write(root.join(".eska/import/partial/input.epf"), "partial").unwrap();
            Err(CreationError::Operation(CreationFailure::Artifact(
                Box::new(ArtifactError::UnsupportedFile(sentinel.clone())),
            )))
        });
        assert!(matches!(
            result,
            Err(CreationError::Operation(CreationFailure::Artifact(_)))
        ));
        assert!(!destination.exists());
        assert!(!fixture.0.join(".eska").exists());
        assert_eq!(fs::read(sentinel).unwrap(), b"preserved");
    }

    #[test]
    fn failed_operation_removes_only_its_new_directory() {
        let fixture = test_support::TestDir::new();
        let sentinel = fixture.0.join("user-file");
        fs::write(&sentinel, "preserved").expect("sentinel");
        let destination = fixture.0.join("partial-project");
        let result = in_new_directory(&destination, |root| {
            fs::create_dir(root.path().join("src")).expect("partial source");
            fs::write(root.path().join("eska.toml"), "partial config").expect("partial config");
            let binary = crate::test_component(eska_tool_runtime::Component::Vcs);
            eska_tool_runtime::vcs::initialize_new(&binary, root.path())
                .expect("partial Git repository");
            Err(CreationError::Io {
                path: root.path().join("injected-error"),
                source: std::io::Error::other("injected failure"),
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
            fs::remove_dir(root.path()).expect("remove owned empty directory");
            fs::write(root.path(), "leftover").expect("replace owned directory with file");
            Err(CreationError::Io {
                path: root.path().to_path_buf(),
                source: std::io::Error::other("original failure"),
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
