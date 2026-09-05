//! Localized CLI presentation for completing the active workflow task.

use std::{path::Path, process::ExitCode};

use clap::Args;

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{discovery, finish},
};

#[derive(Debug, Args)]
pub(in crate::cli) struct FinishArgs {
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl FinishArgs {
    pub(super) fn run(project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover(project_dir) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("{}", diagnostics::present_project_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let result = match finish::execute(&project) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("{}", present_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let key = if result.branch_deleted {
            "finish-completed-deleted"
        } else {
            "finish-completed-preserved"
        };
        println!(
            "{}",
            localizer.format(
                key,
                &[
                    ("task", LocalizationValue::Text(&result.task)),
                    ("task_branch", LocalizationValue::Text(&result.task_branch)),
                    ("base", LocalizationValue::Text(&result.base_branch)),
                ],
            )
        );
        ExitCode::SUCCESS
    }
}

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("finish-about"))
        .override_usage(localizer.text("finish-usage"))
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}

fn present_error(error: &finish::FinishError, localizer: &Localizer) -> String {
    match error {
        finish::FinishError::WorkflowNotConfigured => localizer.text("finish-workflow-missing"),
        finish::FinishError::Policy(_) => localizer.text("finish-policy-error"),
        finish::FinishError::Repository(_) => localizer.text("finish-repository-error"),
        finish::FinishError::ProjectOutsideRepository { .. } => {
            localizer.text("finish-project-outside-repository")
        }
        finish::FinishError::OperationInProgress => localizer.text("finish-operation-in-progress"),
        finish::FinishError::DirtyWorkspace { files } => localizer.format(
            "finish-dirty-workspace",
            &[(
                "files",
                LocalizationValue::Number(i64::try_from(*files).unwrap_or(i64::MAX)),
            )],
        ),
        finish::FinishError::DetachedHead => localizer.text("finish-detached-head"),
        finish::FinishError::UnbornHead => localizer.text("finish-unborn-head"),
        finish::FinishError::NotTaskBranch => localizer.text("finish-not-task-branch"),
        finish::FinishError::BaseBranchMissing { branch } => localizer.format(
            "finish-base-missing",
            &[("branch", LocalizationValue::Text(branch))],
        ),
        finish::FinishError::RemoteBaseMissing {
            remote,
            url,
            reference,
        } => localizer.format(
            "finish-remote-base-missing",
            &[
                ("remote", LocalizationValue::Text(remote)),
                ("url", LocalizationValue::Text(url)),
                ("reference", LocalizationValue::Text(reference)),
            ],
        ),
        finish::FinishError::RemoteRequired { remote } => localizer.format(
            "finish-remote-required",
            &[("remote", LocalizationValue::Text(remote))],
        ),
        finish::FinishError::RequirementReferenceMissing { reference } => localizer.format(
            "finish-reference-missing",
            &[("reference", LocalizationValue::Text(reference))],
        ),
        finish::FinishError::NotPublished { reference } => localizer.format(
            "finish-not-published",
            &[("reference", LocalizationValue::Text(reference))],
        ),
        finish::FinishError::NotIntegrated { reference } => localizer.format(
            "finish-not-integrated",
            &[("reference", LocalizationValue::Text(reference))],
        ),
        finish::FinishError::BaseDiverged {
            branch,
            remote_reference,
        } => localizer.format(
            "finish-base-diverged",
            &[
                ("branch", LocalizationValue::Text(branch)),
                ("reference", LocalizationValue::Text(remote_reference)),
            ],
        ),
        finish::FinishError::Fetch {
            remote,
            url,
            reason,
        } => localizer.format(
            "finish-fetch-error",
            &[
                ("remote", LocalizationValue::Text(remote)),
                ("url", LocalizationValue::Text(url)),
                ("reason", LocalizationValue::Text(reason)),
            ],
        ),
        finish::FinishError::Ancestry(_) => localizer.text("finish-ancestry-error"),
        finish::FinishError::UpdateBase(_) => localizer.text("finish-update-base-error"),
        finish::FinishError::Switch(_) => localizer.text("finish-switch-error"),
        finish::FinishError::DeleteBranch(_) => localizer.text("finish-delete-error"),
    }
}
