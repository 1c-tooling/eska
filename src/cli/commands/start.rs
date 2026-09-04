//! Localized CLI presentation for starting work according to workflow policy.

use std::{path::Path, process::ExitCode};

use clap::Args;

use crate::{
    cli::{diagnostics, localization::Localizer},
    project::{discovery, start},
    vcs::command,
};

use crate::cli::localization::LocalizationValue;

#[derive(Debug, Args)]
pub(in crate::cli) struct StartArgs {
    task: String,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl StartArgs {
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover(project_dir) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("{}", diagnostics::present_project_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let result = match start::execute(&project, &self.task) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("{}", present_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        println!(
            "{}",
            localizer.format(
                "start-created",
                &[
                    ("task", LocalizationValue::Text(&result.task)),
                    ("branch", LocalizationValue::Text(&result.branch)),
                    ("base", LocalizationValue::Text(&result.base_branch)),
                ],
            )
        );
        ExitCode::SUCCESS
    }
}

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("start-about"))
        .override_usage(localizer.text("start-usage"))
        .mut_arg("task", |argument| {
            argument
                .help(localizer.text("start-task-help"))
                .value_name(localizer.text("start-task-value"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}

fn present_error(error: &start::StartError, localizer: &Localizer) -> String {
    match error {
        start::StartError::WorkflowNotConfigured => localizer.text("start-workflow-missing"),
        start::StartError::Policy(error) => match error {
            crate::vcs::workflow::PolicyError::InvalidTask { .. }
            | crate::vcs::workflow::PolicyError::ProtectedTaskBranch { .. } => {
                localizer.text("start-task-invalid")
            }
            _ => localizer.text("start-policy-error"),
        },
        start::StartError::Repository(_) => localizer.text("start-repository-error"),
        start::StartError::ProjectOutsideRepository { .. } => {
            localizer.text("start-project-outside-repository")
        }
        start::StartError::DirtyWorkspace { files } => localizer.format(
            "start-dirty-workspace",
            &[(
                "files",
                LocalizationValue::Number(i64::try_from(*files).unwrap_or(i64::MAX)),
            )],
        ),
        start::StartError::DetachedHead => localizer.text("start-detached-head"),
        start::StartError::UnbornHead => localizer.text("start-unborn-head"),
        start::StartError::BaseBranchMissing { branch } => localizer.format(
            "start-base-missing",
            &[("branch", LocalizationValue::Text(branch))],
        ),
        start::StartError::RemoteBaseMissing { reference } => localizer.format(
            "start-remote-base-missing",
            &[("reference", LocalizationValue::Text(reference))],
        ),
        start::StartError::TaskBranchExists { branch } => localizer.format(
            "start-branch-exists",
            &[("branch", LocalizationValue::Text(branch))],
        ),
        start::StartError::BaseDiverged {
            branch,
            remote_reference,
        } => localizer.format(
            "start-base-diverged",
            &[
                ("branch", LocalizationValue::Text(branch)),
                ("reference", LocalizationValue::Text(remote_reference)),
            ],
        ),
        start::StartError::Command(error) => localizer.text(command_error_key(error)),
    }
}

const fn command_error_key(error: &command::Error) -> &'static str {
    let operation = match error {
        command::Error::Spawn { operation, .. } | command::Error::Failed { operation, .. } => {
            operation
        }
    };
    match operation {
        command::Operation::Fetch => "start-fetch-error",
        command::Operation::Ancestry => "start-ancestry-error",
        command::Operation::UpdateBase => "start-update-base-error",
        command::Operation::Switch => "start-switch-error",
    }
}
