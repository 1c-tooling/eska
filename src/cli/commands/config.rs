//! Global machine configuration initialization and transactional editing.

use std::{
    env,
    path::Path,
    process::{Command, ExitCode},
};

use clap::{Args, Subcommand};

use crate::{
    cli::localization::{LocalizationValue, Localizer},
    config::{
        GlobalConfigEditOutcome, GlobalConfigError, GlobalConfigInitOutcome, config_path,
        edit_global_at, init_global_at,
    },
};

#[derive(Debug, Args)]
pub(in crate::cli) struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    #[command(disable_help_flag = true)]
    Init(ConfigInitArgs),
    #[command(disable_help_flag = true)]
    Edit(ConfigEditArgs),
}

#[derive(Debug, Args)]
struct ConfigInitArgs {
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Args)]
struct ConfigEditArgs {
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl ConfigArgs {
    /// Create or edit the global machine configuration without project discovery.
    pub(super) fn run(&self, localizer: &Localizer) -> ExitCode {
        let path = match config_path() {
            Ok(path) => path,
            Err(error) => return fail(&error, localizer),
        };
        match &self.command {
            ConfigCommand::Init(_) => match init_global_at(&path) {
                Ok(GlobalConfigInitOutcome::Created(path)) => {
                    println!("{}", path_message("config-created", &path, localizer));
                    ExitCode::SUCCESS
                }
                Ok(GlobalConfigInitOutcome::Existing(path)) => {
                    println!("{}", path_message("config-existing", &path, localizer));
                    ExitCode::SUCCESS
                }
                Err(error) => fail(&error, localizer),
            },
            ConfigCommand::Edit(_) => match edit_global_at(&path, open_editor) {
                Ok(GlobalConfigEditOutcome::Unchanged(path)) => {
                    println!("{}", path_message("config-unchanged", &path, localizer));
                    ExitCode::SUCCESS
                }
                Ok(GlobalConfigEditOutcome::Changed { path, backup }) => {
                    println!(
                        "{}",
                        localizer.format(
                            "config-updated",
                            &[
                                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                                ("backup", LocalizationValue::Text(&backup.to_string_lossy()),),
                            ],
                        )
                    );
                    ExitCode::SUCCESS
                }
                Err(error) => fail(&error, localizer),
            },
        }
    }
}

/// Open a blocking editor selected from `ESKA_EDITOR`, `VISUAL`, `EDITOR` or OS defaults.
fn open_editor(path: &Path) -> Result<(), GlobalConfigError> {
    let editor = env::var_os("ESKA_EDITOR")
        .or_else(|| env::var_os("VISUAL"))
        .or_else(|| env::var_os("EDITOR"))
        .unwrap_or_else(default_editor);
    let status = Command::new(editor)
        .arg(path)
        .status()
        .map_err(|source| GlobalConfigError::Editor { source })?;
    if status.success() {
        Ok(())
    } else {
        Err(GlobalConfigError::EditorFailed)
    }
}

#[cfg(windows)]
fn default_editor() -> std::ffi::OsString {
    "notepad.exe".into()
}

#[cfg(not(windows))]
fn default_editor() -> std::ffi::OsString {
    "vi".into()
}

fn path_message(key: &str, path: &Path, localizer: &Localizer) -> String {
    localizer.format(
        key,
        &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
    )
}

fn fail(error: &GlobalConfigError, localizer: &Localizer) -> ExitCode {
    eprintln!("{}", present_error(error, localizer));
    ExitCode::FAILURE
}

pub(super) fn present_error(error: &GlobalConfigError, localizer: &Localizer) -> String {
    match error {
        GlobalConfigError::LocationUnavailable => localizer.text("config-location-error"),
        GlobalConfigError::Io { path, source } | GlobalConfigError::Replace { path, source } => {
            localizer.format(
                "config-io-error",
                &[
                    ("path", LocalizationValue::Text(&path.to_string_lossy())),
                    ("reason", LocalizationValue::Text(&source.to_string())),
                ],
            )
        }
        GlobalConfigError::Invalid { path, source } => localizer.format(
            "config-invalid",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        GlobalConfigError::DistroboxContainerMissing { path } => {
            path_message("config-distrobox-container-missing", path, localizer)
        }
        GlobalConfigError::HostContainerUnexpected { path } => {
            path_message("config-host-container-unexpected", path, localizer)
        }
        GlobalConfigError::Editor { source } => localizer.format(
            "config-editor-error",
            &[("reason", LocalizationValue::Text(&source.to_string()))],
        ),
        GlobalConfigError::EditorFailed => localizer.text("config-editor-failed"),
    }
}

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("config-about"))
        .override_usage(localizer.text("config-usage"))
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        .mut_subcommand("init", |command| {
            command
                .about(localizer.text("config-init-about"))
                .override_usage(localizer.text("config-init-usage"))
                .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        })
        .mut_subcommand("edit", |command| {
            command
                .about(localizer.text("config-edit-about"))
                .override_usage(localizer.text("config-edit-usage"))
                .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        })
}
