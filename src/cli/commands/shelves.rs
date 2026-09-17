//! Explicit repository-wide shelf commands and their localized arguments.

use crate::{
    cli::{
        localization::Localizer,
        shelves::{self as presentation, OutputFormat},
    },
    vcs::shelves::{self, Session},
};
use clap::Args;
use std::{path::Path, process::ExitCode};

#[derive(Debug, Args)]
pub(in crate::cli) struct ShelveArgs {
    #[command(flatten)]
    options: MutationOptions,
}
#[derive(Debug, Args)]
pub(in crate::cli) struct UnshelveArgs {
    id: Option<String>,
    #[command(flatten)]
    options: MutationOptions,
}
#[derive(Debug, Args)]
struct MutationOptions {
    #[arg(long)]
    dry_run: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}
#[derive(Debug, Args)]
pub(in crate::cli) struct ShelvesArgs {
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl ShelveArgs {
    /// Save the current branch's repository-wide state or display its exact preview.
    pub(super) fn run(&self, start: &Path, localizer: &Localizer) -> ExitCode {
        self.options.run(start, false, None, localizer)
    }
}
impl UnshelveArgs {
    /// Restore the current branch's matching shelf without overwriting conflicts.
    pub(super) fn run(&self, start: &Path, localizer: &Localizer) -> ExitCode {
        self.options.run(start, true, self.id.as_deref(), localizer)
    }
}
impl MutationOptions {
    /// Dispatch one preflighted mutation with a shared read-only preview and error contract.
    fn run(
        &self,
        start: &Path,
        restore: bool,
        id: Option<&str>,
        localizer: &Localizer,
    ) -> ExitCode {
        let repository = match presentation::repository(start, self.format, localizer) {
            Ok(repo) => repo,
            Err(code) => return code,
        };
        let result = if self.dry_run {
            if restore {
                shelves::restore_plan(&repository, id)
            } else {
                shelves::plan(&repository)
            }
        } else {
            Session::acquire(&repository).and_then(|session| {
                if restore {
                    session.restore(id)
                } else {
                    session.capture()
                }
            })
        };
        match result {
            Ok(saved) => {
                presentation::write_result(self.format, &saved, restore, self.dry_run, localizer)
            }
            Err(error) => presentation::write_error(self.format, &error, localizer),
        }
    }
}
impl ShelvesArgs {
    /// List retained shelves without modifying the repository or filesystem.
    pub(super) fn run(&self, start: &Path, localizer: &Localizer) -> ExitCode {
        let repository = match presentation::repository(start, self.format, localizer) {
            Ok(repo) => repo,
            Err(code) => return code,
        };
        match shelves::list(&repository) {
            Ok(saved) => presentation::write_list(self.format, &saved, localizer),
            Err(error) => presentation::write_error(self.format, &error, localizer),
        }
    }
}

/// Attach localized help only to the arguments supported by each shelf command.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    let name = command.get_name().to_owned();
    let mut command = command
        .about(localizer.text(&format!("{name}-about")))
        .mut_arg("format", |arg| {
            arg.help(localizer.text("shelf-format-help"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")));
    if name != "shelves" {
        command = command.mut_arg("dry_run", |arg| {
            arg.help(localizer.text("shelf-dry-run-help"))
        });
    }
    if name == "unshelve" {
        command = command.mut_arg("id", |arg| arg.help(localizer.text("shelf-id-help")));
    }
    command
}
