//! Localized self-update command with a locale-independent JSON contract.

use crate::{
    cli::{
        encoding::json_path,
        localization::{LocalizationValue, Localizer},
    },
    update::{self, Status},
};
use clap::{Args, ValueEnum};
use serde_json::json;
use std::process::ExitCode;

#[derive(Debug, Args)]
pub(in crate::cli) struct UpdateArgs {
    #[arg(long)]
    check: bool,
    #[arg(long)]
    target_version: Option<String>,
    #[arg(long, value_enum, default_value_t = Format::Human)]
    format: Format,
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Format {
    Human,
    Json,
}

impl UpdateArgs {
    /// Updates the installed CLI independently of project discovery and Git state.
    pub(super) fn run(&self, localizer: &Localizer) -> ExitCode {
        match update::run(self.check, self.target_version.as_deref()) {
            Ok(result) => {
                let unsupported = matches!(result.status, Status::UnsupportedInstallation);
                match self.format {
                    Format::Json => {
                        let (value, encoding) = json_path(result.executable.as_os_str());
                        println!(
                            "{}",
                            json!({"schemaVersion":1,"status":result.status,"installedVersion":result.installed,
                            "availableVersion":result.available,"method":result.method,"executable":{"value":value,"encoding":encoding}})
                        );
                    }
                    Format::Human => {
                        let key = match result.status {
                            Status::UpToDate => "update-current",
                            Status::UpdateAvailable => "update-available",
                            Status::Updated => "update-updated",
                            Status::UnsupportedInstallation => "update-unsupported",
                        };
                        println!(
                            "{}",
                            localizer.format(
                                key,
                                &[
                                    ("installed", LocalizationValue::Text(&result.installed)),
                                    (
                                        "available",
                                        LocalizationValue::Text(
                                            result.available.as_deref().unwrap_or("")
                                        )
                                    )
                                ]
                            )
                        );
                    }
                }
                if unsupported {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(error) => {
                let code = error.code();
                match self.format {
                    Format::Json => println!(
                        "{}",
                        json!({"schemaVersion":1,"status":"error","code":code})
                    ),
                    Format::Human => {
                        eprintln!("{}", localizer.text(&format!("update-error-{code}")));
                    }
                }
                ExitCode::FAILURE
            }
        }
    }
}

/// Localize only presentation; command names, flags and JSON remain stable.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("update-about"))
        .mut_arg("check", |arg| arg.help(localizer.text("update-check-help")))
        .mut_arg("target_version", |arg| {
            arg.help(localizer.text("update-target-help"))
        })
        .mut_arg("format", |arg| {
            arg.help(localizer.text("update-format-help"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
