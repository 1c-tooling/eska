//! Narrow read-only process contracts for build provenance and source replacement preflight.

use crate::vcs::repository::{Error, Head, Repository};
use clap::{Args, Subcommand};
use eska_project_core::{
    project::{Project, discovery},
    provenance::GitProvenance,
};
use gix::bstr::ByteSlice;
use serde_json::json;
use std::{path::Path, process::ExitCode};

#[derive(Debug, Args)]
pub struct BridgeArgs {
    #[command(subcommand)]
    operation: Operation,
}

#[derive(Debug, Subcommand)]
enum Operation {
    Provenance,
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
    /// Emit one locale-independent response; hidden bridge operations do not write project state.
    #[must_use]
    pub fn run(&self, root: &Path) -> ExitCode {
        let (operation, result) = match self.operation {
            Operation::Provenance => ("provenance", Ok(json!(provenance(root)))),
            Operation::SourceChanges {
                empty_scaffold,
                files,
            } => (
                "source_changes",
                discovery::discover(root)
                    .map_err(|_| "project")
                    .and_then(|project| {
                        source_changes(&project, empty_scaffold, files).map_err(|_| "repository")
                    })
                    .map(|count| json!({"local_changes": count})),
            ),
        };
        let (data, error, code) = match result {
            Ok(data) => (Some(data), None, ExitCode::SUCCESS),
            Err(error) => (None, Some(error), ExitCode::FAILURE),
        };
        println!(
            "{}",
            json!({"schema_version":1,"operation":operation,"data":data,"error":error})
        );
        code
    }
}
