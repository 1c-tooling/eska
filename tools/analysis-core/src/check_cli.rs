//! Standalone checker presentation; no transport, build, Git or IDE orchestration.

mod args;
mod presentation;
mod settings;

use std::{env, process::ExitCode, time::Duration};

use clap::{CommandFactory, FromArgMatches};
use eska::{
    project_core::eska_source::EskaSource,
    tool_cli::{
        self, Failure,
        localization::{Localizer, resolve_locale_from_environment},
    },
};

use crate::batch::{BatchOptions, check_syntax};

/// Select one ESKA project, run the explicit syntax profile and emit stable RU/EN results.
#[must_use]
pub fn run() -> ExitCode {
    let arguments = env::args_os().collect::<Vec<_>>();
    let lang = tool_cli::bootstrap_value(&arguments, "--lang");
    let locale = resolve_locale_from_environment(lang.as_deref());
    let localizer = match Localizer::try_new(locale) {
        Ok(localizer) => localizer,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(3);
        }
    };
    let command = tool_cli::localize(
        args::Cli::command().version(env!("CARGO_PKG_VERSION")),
        &localizer,
        "tool-check",
    );
    let matches = match tool_cli::parse(command, arguments, &localizer, "eska-check") {
        Ok(matches) => matches,
        Err(exit) => {
            return if exit == ExitCode::SUCCESS {
                exit
            } else {
                ExitCode::from(3)
            };
        }
    };
    let Ok(cli) = args::Cli::from_arg_matches(&matches) else {
        return failure(
            &Failure::new("arguments_invalid"),
            tool_cli::OutputFormat::Human,
            &localizer,
        );
    };
    match execute(&cli, &localizer) {
        Ok(exit) => exit,
        Err(error) => failure(&error, cli.format, &localizer),
    }
}

/// Compose the selected reader once, without validating or reading an accepted binary baseline.
fn execute(cli: &args::Cli, localizer: &Localizer) -> Result<ExitCode, Failure> {
    let selected = tool_cli::selected(&cli.project_dir, cli.project.as_deref())?;
    let settings = settings::load(
        selected.project.root(),
        cli.config.as_deref(),
        localizer.locale(),
    )?;
    let root = selected.project.root().to_owned();
    let scope = selected.scope.clone();
    let source = EskaSource::open(selected.project, selected.scope)?;
    let options = BatchOptions {
        timeout: Duration::from_secs(cli.timeout_seconds),
        max_document_bytes: cli.max_document_bytes,
        max_retained_source_bytes: cli.max_retained_source_bytes,
        ..BatchOptions::default()
    };
    let report = match cli.profile {
        args::Profile::Syntax => {
            check_syntax(source.into_reader(), &settings.diagnostics, &options)
        }
    }
    .map_err(presentation::batch_failure)?;
    presentation::emit(&root, &scope, &settings, &report, cli.format, localizer)
}

/// Fatal errors use the shared envelope; checker exit 3 differs from findings and incompleteness.
fn failure(error: &Failure, format: tool_cli::OutputFormat, localizer: &Localizer) -> ExitCode {
    let _ = tool_cli::emit_failure("eska-check", Some("check"), format, error, localizer);
    ExitCode::from(3)
}
