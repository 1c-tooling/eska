//! CLI startup, command dispatch and human-facing presentation.

use std::{env, process::ExitCode};

mod args;
mod commands;
mod diagnostics;
mod interactive;
pub mod localization;

pub use args::Cli;

use localization::{Localizer, resolve_locale_from_environment};

/// Starts the CLI with the process arguments and selected UI locale.
#[must_use]
pub fn run() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    let cli_locale = args::bootstrap_lang(&args);
    let locale = resolve_locale_from_environment(cli_locale.as_deref());
    let localizer = match Localizer::try_new(locale) {
        Ok(localizer) => localizer,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    Cli::parse_localized(args, &localizer).run(&localizer)
}

impl Cli {
    /// Runs the selected command, or validates a project, in the UI locale.
    pub fn run(&self, localizer: &Localizer) -> ExitCode {
        commands::run(self.command.as_ref(), &self.project_dir, localizer)
    }
}
