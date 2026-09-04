//! Localized presentation for saving the current project `ChangeSet`.

use std::{path::Path, process::ExitCode};

use clap::Args;

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{discovery, save},
    vcs::command,
};

#[derive(Debug, Args)]
pub(in crate::cli) struct SaveArgs {
    #[arg(short, long, value_name = "MESSAGE")]
    message: Option<String>,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl SaveArgs {
    /// Discover the project and save its complete current `ChangeSet`.
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover(project_dir) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("{}", diagnostics::present_project_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let result = match save::execute(&project, self.message.as_deref()) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("{}", present_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let commit = result.commit.to_string();
        println!(
            "{}",
            localizer.format(
                "save-created",
                &[
                    (
                        "files",
                        LocalizationValue::Number(i64::try_from(result.files).unwrap_or(i64::MAX)),
                    ),
                    ("commit", LocalizationValue::Text(short_id(&commit))),
                ],
            )
        );
        ExitCode::SUCCESS
    }
}

/// Apply localized help text after clap has parsed the bootstrap locale.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("save-about"))
        .override_usage(localizer.text("save-usage"))
        .mut_arg("message", |argument| {
            argument
                .help(localizer.text("save-message-help"))
                .value_name(localizer.text("save-message-value"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}

fn present_error(error: &save::SaveError, localizer: &Localizer) -> String {
    match error {
        save::SaveError::Repository(_) => localizer.text("save-repository-error"),
        save::SaveError::ProjectOutsideRepository { .. } => {
            localizer.text("save-project-outside-repository")
        }
        save::SaveError::DetachedHead => localizer.text("save-detached-head"),
        save::SaveError::NoChanges => localizer.text("save-no-changes"),
        save::SaveError::Conflicts { files } => localizer.format(
            "save-conflicts",
            &[(
                "files",
                LocalizationValue::Number(i64::try_from(*files).unwrap_or(i64::MAX)),
            )],
        ),
        save::SaveError::EmptyMessage => localizer.text("save-empty-message"),
        save::SaveError::IndexSnapshot { path, .. } => path_error(
            localizer,
            "save-index-snapshot-error",
            &path.to_string_lossy(),
        ),
        save::SaveError::Command(error) => present_command_error(error, localizer),
        save::SaveError::IndexRestore { path, original, .. } => localizer.format(
            "save-index-restore-error",
            &[
                (
                    "reason",
                    LocalizationValue::Text(&present_error(original, localizer)),
                ),
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
            ],
        ),
        save::SaveError::CommitNotCreated => localizer.text("save-commit-not-created"),
    }
}

fn present_command_error(error: &command::Error, localizer: &Localizer) -> String {
    let (operation, reason) = match error {
        command::Error::Spawn { operation, source } => (*operation, source.to_string()),
        command::Error::Failed {
            operation,
            status,
            stderr,
        } => {
            let stderr = String::from_utf8_lossy(stderr).trim().to_owned();
            let reason = if stderr.is_empty() {
                status.to_string()
            } else {
                stderr
            };
            (*operation, reason)
        }
    };
    let key = match operation {
        command::Operation::Stage => "save-stage-error",
        command::Operation::Commit => "save-commit-error",
        _ => "save-repository-error",
    };
    localizer.format(key, &[("reason", LocalizationValue::Text(&reason))])
}

fn path_error(localizer: &Localizer, key: &str, path: &str) -> String {
    localizer.format(key, &[("path", LocalizationValue::Text(path))])
}

fn short_id(id: &str) -> &str {
    &id[..id.len().min(7)]
}
