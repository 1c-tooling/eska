//! IDE transport owns protocol and presentation; metadata and analysis remain in the shared session.

mod args;
mod completion;
mod definitions;
mod diagnostics;
mod document_symbols;
mod documents;
mod dto;
mod hover;
mod language;
mod metadata_locations;
mod positions;
mod project_changes;
mod property_edits;
mod protocol;
mod request_control;
mod server;
mod stream;
mod text_edits;
mod watched_files;
mod watcher_registration;

use std::{env, io, process::ExitCode};

use clap::{CommandFactory, FromArgMatches};
use eska::tool_cli::{
    self,
    localization::{Localizer, resolve_locale_from_environment},
};

/// Start a bounded stdio session; only explicit help/version may print human text to stdout.
#[must_use]
pub fn run() -> ExitCode {
    let arguments = env::args_os().collect::<Vec<_>>();
    let lang = tool_cli::bootstrap_value(&arguments, "--lang");
    let locale = resolve_locale_from_environment(lang.as_deref());
    let Ok(localizer) = Localizer::try_new(locale) else {
        return ExitCode::from(3);
    };
    let command = tool_cli::localize(
        args::Cli::command().version(env!("CARGO_PKG_VERSION")),
        &localizer,
        "tool-analyzer",
    );
    let matches = match tool_cli::parse(command, arguments, &localizer, "eska-analyzer") {
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
        return ExitCode::from(3);
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    ExitCode::from(server::run(
        &cli,
        &localizer,
        io::BufReader::new(stdin),
        &mut stdout.lock(),
    ))
}
