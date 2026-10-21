//! Narrow process contracts; callers retain ownership of their surrounding file transactions.

use crate::vcs::{
    git,
    repository::{Error, Head, Repository},
};
use clap::{Args, Subcommand};
use eska_project_core::{
    project::{Project, discovery},
    provenance::GitProvenance,
};
use gix::bstr::ByteSlice;
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Debug, Args)]
pub struct BridgeArgs {
    #[command(subcommand)]
    operation: Operation,
}

#[derive(Debug, Subcommand)]
enum Operation {
    Doctor {
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long = "attribute-root")]
        attribute_roots: Vec<PathBuf>,
    },
    PatchPlan {
        #[arg(long)]
        base: Option<String>,
    },
    PreparePatch {
        #[arg(long)]
        base: Option<String>,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        expected: String,
    },
    Provenance,
    RepositoryExists,
    Initialize,
    InitializeReserved,
    SourceChanges {
        #[arg(long)]
        empty_scaffold: bool,
        #[arg(long)]
        files: usize,
    },
}

/// Preserve the same available/absent/unavailable semantics as the former in-process adapter.
#[must_use]
pub fn provenance(root: &Path) -> GitProvenance {
    let repository = match Repository::discover(root) {
        Ok(repository) => repository,
        Err(Error::NotFound { .. }) => return GitProvenance::absent(),
        Err(_) => return GitProvenance::unavailable(),
    };
    let commit = match repository.head() {
        Ok(Head::Unborn { .. }) => None,
        Ok(head) => head.id().map(|id| id.to_string()),
        Err(_) => return GitProvenance::unavailable(),
    };
    repository.status().map_or_else(
        |_| GitProvenance::unavailable(),
        |status| GitProvenance::available(commit, status.is_dirty()),
    )
}

/// Count only selected source paths; repository errors never become a clean source result.
///
/// # Errors
/// Returns unreadable or broken repository state while preserving Git-less confirmation rules.
pub fn source_changes(
    project: &Project,
    empty_scaffold: bool,
    files: usize,
) -> Result<usize, Error> {
    let repository = match Repository::discover(project.root()) {
        Ok(repository) => repository,
        Err(Error::NotFound { .. }) => return Ok(if empty_scaffold { 0 } else { files }),
        Err(error) => return Err(error),
    };
    Ok(repository
        .status()?
        .entries
        .iter()
        .filter(|entry| {
            repository
                .work_dir()
                .join(gix::path::from_bstr(entry.path.as_bstr()).as_ref())
                .starts_with(project.source())
                && !(empty_scaffold && entry.path.ends_with(b"/.gitkeep"))
        })
        .count())
}

impl BridgeArgs {
    /// Emit a complete machine response; mutators require an exclusively owned new namespace.
    #[must_use]
    pub fn run(&self, root: &Path) -> ExitCode {
        let (operation, result): (&str, Result<Value, Value>) = match &self.operation {
            Operation::Doctor {
                project,
                attribute_roots,
            } => ("doctor", doctor(root, project.as_deref(), attribute_roots)),
            Operation::PatchPlan { base } => (
                "patch_plan",
                crate::project::patch::plan_reply(root, base.as_deref())
                    .map_err(|error| json!(error)),
            ),
            Operation::PreparePatch {
                base,
                workspace,
                expected,
            } => (
                "prepare_patch",
                crate::project::patch::prepare_reply(root, base.as_deref(), workspace, expected)
                    .map_err(|error| json!(error)),
            ),
            Operation::Provenance => ("provenance", Ok(json!(provenance(root)))),
            Operation::RepositoryExists => (
                "repository_exists",
                git::exists(root)
                    .map(|exists| json!({"exists":exists}))
                    .map_err(|_| json!("repository")),
            ),
            Operation::Initialize => (
                "initialize",
                initialize(root, false).map_err(|error| json!(error)),
            ),
            Operation::InitializeReserved => (
                "initialize_reserved",
                initialize(root, true).map_err(|error| json!(error)),
            ),
            Operation::SourceChanges {
                empty_scaffold,
                files,
            } => (
                "source_changes",
                discovery::discover(root)
                    .map_err(|_| json!("project"))
                    .and_then(|project| {
                        source_changes(&project, *empty_scaffold, *files)
                            .map_err(|_| json!("repository"))
                    })
                    .map(|count| json!({"local_changes": count})),
            ),
        };
        let (data, error, code) = match result {
            Ok(data) => (Some(data), None, ExitCode::SUCCESS),
            Err(error) => (None, Some(error), ExitCode::FAILURE),
        };
        let document = json!({"schema_version":1,"operation":operation,"data":data,"error":error});
        let mut stdout = io::stdout().lock();
        if serde_json::to_writer(&mut stdout, &document).is_err() || writeln!(stdout).is_err() {
            return ExitCode::FAILURE;
        }
        code
    }
}

/// Refuse collisions before invoking the initializer; rollback remains the caller's responsibility.
fn initialize(root: &Path, reserved: bool) -> Result<serde_json::Value, &'static str> {
    let metadata = fs::symlink_metadata(root).map_err(|_| "io")?;
    if metadata.is_symlink() || !metadata.is_dir() {
        return Err("destination");
    }
    if reserved {
        if root.file_name().is_none_or(|name| name != ".git")
            || fs::read_dir(root).map_err(|_| "io")?.next().is_some()
        {
            return Err("git_collision");
        }
        git::initialize_reserved(root).map_err(|_| "git_init")?;
    } else {
        match fs::symlink_metadata(root.join(".git")) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            _ => return Err("git_collision"),
        }
        git::initialize(root).map_err(|_| "git_init")?;
    }
    Ok(json!({"initialized":true}))
}

/// A project failure cannot prevent the caller from requesting independent system Git checks.
fn doctor(root: &Path, project: Option<&Path>, attributes: &[PathBuf]) -> Result<Value, Value> {
    let checks = match project {
        Some(path) => {
            let project = discovery::discover(path).map_err(|_| json!("project"))?;
            crate::doctor::inspect_vcs(root, &project, attributes)
        }
        None => crate::doctor::inspect_system_git(root, attributes),
    };
    let details = vec![None::<String>; checks.len()];
    Ok(json!({"checks":checks,"details":details}))
}
