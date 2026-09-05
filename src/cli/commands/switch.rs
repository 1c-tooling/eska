//! Localized CLI presentation for switching between existing workflow branches.

use std::{path::Path, process::ExitCode};

use clap::{ArgGroup, Args};

use crate::{
    cli::{diagnostics, localization::LocalizationValue, localization::Localizer},
    project::{discovery, switch},
};

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("target")
        .required(true)
        .args(["task", "base"])
))]
pub(in crate::cli) struct SwitchArgs {
    task: Option<String>,

    #[arg(long)]
    base: bool,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl SwitchArgs {
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover(project_dir) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("{}", diagnostics::present_project_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let target = self
            .task
            .as_deref()
            .map_or(switch::SwitchTarget::Base, switch::SwitchTarget::Task);
        let result = match switch::execute(&project, target) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("{}", present_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let key = if result.task.is_some() {
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

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("switch-about"))
        .override_usage(localizer.text("switch-usage"))
        .mut_arg("task", |argument| {
            argument
                .help(localizer.text("switch-task-help"))
                .value_name(localizer.text("switch-task-value"))
        })
        .mut_arg("base", |argument| {
            argument.help(localizer.text("switch-base-help"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}

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
        switch::SwitchError::DirtyWorkspace { files } => localizer.format(
            "switch-dirty-workspace",
            &[(
                "files",
                LocalizationValue::Number(i64::try_from(*files).unwrap_or(i64::MAX)),
            )],
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
