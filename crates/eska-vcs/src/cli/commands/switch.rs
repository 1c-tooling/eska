//! Localized CLI presentation for switching between existing workflow branches.

pub use eska_presentation::arguments::vcs::switch::{SwitchArgs, localize};

use std::{path::Path, process::ExitCode};

use crate::{
    cli::{
        diagnostics,
        localization::LocalizationValue,
        localization::Localizer,
        shelves::{self, OutputFormat, ShelfDocument},
    },
    project::{discovery, switch},
};

struct SwitchHandler<'a> {
    args: &'a SwitchArgs,
}

impl SwitchHandler<'_> {
    /// Present a repository-wide switch or its read-only preview.
    #[must_use]
    pub fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover_context(project_dir) {
            Ok(project) => project,
            Err(error) => {
                return shelves::fail(
                    self.args.format,
                    "project-discovery",
                    &diagnostics::present_context_error(&error, localizer),
                );
            }
        };
        let target = self
            .args
            .task
            .as_deref()
            .map_or(switch::SwitchTarget::Base, switch::SwitchTarget::Task);
        let result = match switch::execute_context(&project, target, self.args.dry_run) {
            Ok(result) => result,
            Err(error) => {
                if let switch::SwitchError::Shelf(error) = &error {
                    return shelves::write_error(self.args.format, error, localizer);
                }
                return shelves::fail(
                    self.args.format,
                    error_code(&error),
                    &present_error(&error, localizer),
                );
            }
        };
        if matches!(self.args.format, OutputFormat::Json) {
            return shelves::write_json(
                &serde_json::json!({
                    "schema_version": 1, "operation": "switch", "dry_run": result.dry_run,
                    "task": result.task, "branch": result.branch,
                    "saved": result.saved.as_ref().map(ShelfDocument::from),
                    "restored": result.restored.as_ref().map(ShelfDocument::from),
                }),
                localizer,
            );
        }
        if let Some(saved) = &result.saved {
            shelves::write_human(saved, false, self.args.dry_run, localizer);
        }
        if let Some(restored) = &result.restored {
            shelves::write_human(restored, true, self.args.dry_run, localizer);
        }
        let key = if self.args.dry_run {
            "switch-preview"
        } else if result.task.is_some() {
            "switch-task-activated"
        } else {
            "switch-base-activated"
        };
        let task = result.task.as_deref().unwrap_or_default();
        println!(
            "{}",
            localizer.format(
                key,
                &[
                    ("task", LocalizationValue::Text(task)),
                    ("branch", LocalizationValue::Text(&result.branch)),
                ],
            )
        );
        ExitCode::SUCCESS
    }
}

/// Translate domain failures at the presentation boundary.
fn present_error(error: &switch::SwitchError, localizer: &Localizer) -> String {
    match error {
        switch::SwitchError::WorkflowNotConfigured => localizer.text("switch-workflow-missing"),
        switch::SwitchError::Policy(error) => match error {
            crate::vcs::workflow::PolicyError::InvalidTask { .. }
            | crate::vcs::workflow::PolicyError::ProtectedTaskBranch { .. } => {
                localizer.text("switch-task-invalid")
            }
            _ => localizer.text("switch-policy-error"),
        },
        switch::SwitchError::Repository(_) => localizer.text("switch-repository-error"),
        switch::SwitchError::ProjectOutsideRepository { .. } => {
            localizer.text("switch-project-outside-repository")
        }
        switch::SwitchError::Shelf(error) => {
            localizer.text(&format!("shelf-error-{}", shelves::error_code(error)))
        }
        switch::SwitchError::TargetCheckedOut { branch } => localizer.format(
            "switch-target-checked-out",
            &[("branch", LocalizationValue::Text(branch))],
        ),
        switch::SwitchError::TaskBranchMissing { branch } => localizer.format(
            "switch-task-branch-missing",
            &[("branch", LocalizationValue::Text(branch))],
        ),
        switch::SwitchError::BaseBranchMissing { branch } => localizer.format(
            "switch-base-branch-missing",
            &[("branch", LocalizationValue::Text(branch))],
        ),
        switch::SwitchError::Command(_) => localizer.text("switch-command-error"),
    }
}

/// Preserve locale-independent switch failure codes.
const fn error_code(error: &switch::SwitchError) -> &'static str {
    match error {
        switch::SwitchError::WorkflowNotConfigured => "workflow-missing",
        switch::SwitchError::Policy(_) => "policy",
        switch::SwitchError::Repository(_) => "repository",
        switch::SwitchError::ProjectOutsideRepository { .. } => "project-outside-repository",
        switch::SwitchError::TaskBranchMissing { .. } => "task-branch-missing",
        switch::SwitchError::BaseBranchMissing { .. } => "base-branch-missing",
        switch::SwitchError::TargetCheckedOut { .. } => "target-checked-out",
        switch::SwitchError::Shelf(error) => shelves::error_code(error),
        switch::SwitchError::Command(_) => "command",
    }
}

/// Execute parsed VCS options in the repository owner.
#[must_use]
pub fn run(args: &SwitchArgs, directory: &Path, localizer: &Localizer) -> ExitCode {
    SwitchHandler { args }.run(directory, localizer)
}
