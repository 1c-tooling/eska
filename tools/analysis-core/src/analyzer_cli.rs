//! IDE transport owns protocol and presentation; metadata and analysis remain in the shared session.

mod args;
mod capabilities;
mod completion;
mod definitions;
mod diagnostics;
mod document_symbols;
mod documents;
mod dto;
mod edit_authority;
mod external_context;
mod hover;
mod input_declarations;
mod kernel_memory;
mod language;
mod markup;
mod metadata_locations;
mod navigation;
mod permissions;
mod positions;
mod project_changes;
mod property_edits;
mod protocol;
mod query_analysis;
mod query_consumers;
mod query_syntax;
mod read_permissions;
mod references;
mod rename;
mod request_control;
mod resource_budget;
mod server;
mod signature_help;
mod source_saves;
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

/// Start bounded stdio or answer an explicit project-free compatibility query.
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
    if cli.capabilities {
        return match capabilities::emit(cli.format, &localizer) {
            Ok(exit) => exit,
            Err(error) => {
                let _ = tool_cli::emit_failure(
                    "eska-analyzer",
                    Some("capabilities"),
                    cli.format,
                    &error,
                    &localizer,
                );
                ExitCode::from(3)
            }
        };
    }
    let stdin = io::stdin();
    let stdout = io::stdout();
    ExitCode::from(server::run(
        &cli,
        &localizer,
        io::BufReader::new(stdin),
        &mut stdout.lock(),
    ))
}
