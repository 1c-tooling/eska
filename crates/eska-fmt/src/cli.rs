//! Native argument parsing and localized output wrap core-owned policy and checked source publication.

mod planning;
mod report;

use clap::{CommandFactory, FromArgMatches, Parser};
use eska_presentation::{
    arguments::{
        bootstrap_lang,
        formatting::{self, FmtArgs},
    },
    localization::{Localizer, resolve_locale_from_environment},
};
use eska_project_core::formatting::Error;
use std::{
    ffi::OsString,
    path::PathBuf,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug, Parser)]
#[command(
    name = "eska-fmt",
    disable_help_flag = true,
    disable_version_flag = true
)]
struct Cli {
    #[arg(long)]
    lang: Option<String>,
    #[arg(long, default_value = ".")]
    project_dir: PathBuf,
    #[command(flatten)]
    arguments: FmtArgs,
    #[arg(short='V',long,action=clap::ArgAction::Version)]
    version: Option<bool>,
}

/// The exact pure-text entry precedes locale/config discovery and never writes a source file.
pub fn run() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    if let Some(code) = eska_presentation::components::tool_contract(
        &args,
        "eska-fmt",
        env!("CARGO_PKG_VERSION"),
        None,
    ) {
        return code;
    }
    if args.len() == 2 && args[1] == "__format-text" {
        return report::pure();
    }
    let locale = resolve_locale_from_environment(bootstrap_lang(&args).as_deref());
    let Ok(localizer) = Localizer::try_new(locale) else {
        return ExitCode::from(2);
    };
    let command = formatting::localize(
        Cli::command().version(env!("CARGO_PKG_VERSION")),
        &localizer,
    )
    .mut_arg("lang", |argument| {
        argument.help(localizer.text("cli-lang-help"))
    })
    .mut_arg("project_dir", |argument| {
        argument.help(localizer.text("check-project-dir-help"))
    })
    .mut_arg("version", |argument| {
        argument.help(localizer.text("cli-version"))
    });
    let matches = command.get_matches_from(args);
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    if eska_presentation::components::verify_installation("eska-fmt", &localizer).is_err() {
        return ExitCode::from(2);
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancelled);
    let mut output = report::Report::new(&cli.arguments);
    let result = ctrlc::set_handler(move || signal.store(true, Ordering::Release))
        .map_err(|_| Error::new("signal_handler", "operation"))
        .and_then(|()| planning::run(&cli, &cancelled, &mut output));
    if let Err(error) = result {
        for file in &mut output.files {
            if file.status == "planned" {
                file.status = "not_applied";
            }
        }
        output.error = Some(error);
    }
    let code = if output
        .error
        .as_ref()
        .is_some_and(|error| error.code == "cancelled")
    {
        130
    } else if output.error.is_some() {
        2
    } else {
        u8::from(cli.arguments.check() && output.files.iter().any(|file| file.changed))
    };
    if report::write(&output, &cli.arguments, &localizer).is_err() {
        eprintln!("{}", localizer.text("fmt-output-error"));
        return ExitCode::from(2);
    }
    ExitCode::from(code)
}
