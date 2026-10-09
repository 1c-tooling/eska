//! Standalone project CLI delegates every source operation to the shared offline core.

mod args;
mod operations;

use crate::tool_cli::{
    self, Failure,
    localization::{Localizer, resolve_locale_from_environment},
};
use clap::{CommandFactory, FromArgMatches};
use std::{env, process::ExitCode};

const TOOL: &str = "eska-project";

/// Start a single operation using RU/EN presentation and a stable, machine-only JSON envelope.
#[must_use]
pub fn run() -> ExitCode {
    let arguments: Vec<_> = env::args_os().collect();
    let locale = tool_cli::bootstrap_value(&arguments, "--lang");
    let locale = resolve_locale_from_environment(locale.as_deref());
    let localizer = match Localizer::try_new(locale) {
        Ok(localizer) => localizer,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let command = tool_cli::localize(
        args::Cli::command()
            .version(env!("CARGO_PKG_VERSION"))
            .propagate_version(true),
        &localizer,
        "tool-project",
    );
    let matches = match tool_cli::parse(command, arguments, &localizer, TOOL) {
        Ok(matches) => matches,
        Err(exit) => return exit,
    };
    let Ok(cli) = args::Cli::from_arg_matches(&matches) else {
        return tool_cli::emit_failure(
            TOOL,
            None,
            tool_cli::OutputFormat::Human,
            &Failure::new("arguments_invalid"),
            &localizer,
        );
    };
    let operation = cli.command.name();
    let result = operations::execute(&cli, &localizer).and_then(|(data, human)| {
        tool_cli::emit_success(TOOL, operation, cli.format, &data, &human)
    });
    match result {
        Ok(exit) => exit,
        Err(failure) => {
            tool_cli::emit_failure(TOOL, Some(operation), cli.format, &failure, &localizer)
        }
    }
}
