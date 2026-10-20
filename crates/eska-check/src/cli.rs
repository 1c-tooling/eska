//! CLI parsing and presentation are separate from the shared complete-analysis adapter.

use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use clap::{ArgAction, CommandFactory, FromArgMatches, Parser, ValueEnum};
use eska_analyzer::{
    check::{self, Failure, Report},
    configuration,
    process::Analyzer,
};
use eska_presentation::{
    arguments::bootstrap_lang,
    localization::{Localizer, resolve_locale_from_environment},
};
use serde_json::json;

use crate::report;

#[derive(Debug, Parser)]
#[command(
    name = "eska-check",
    disable_help_flag = true,
    disable_version_flag = true
)]
struct Cli {
    #[arg(long)]
    lang: Option<String>,
    #[arg(long, default_value = ".")]
    project_dir: PathBuf,
    #[arg(long)]
    application: Option<String>,
    #[arg(long)]
    analyzer: Option<PathBuf>,
    #[arg(long, default_value_t=600, value_parser=clap::value_parser!(u64).range(1..))]
    timeout: u64,
    #[arg(long, value_enum, default_value_t=Format::Human)]
    format: Format,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(short, long, action=ArgAction::Help)]
    help: Option<bool>,
    #[arg(short='V', long, action=ArgAction::Version)]
    version: Option<bool>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Format {
    Human,
    Json,
}

/// Parse help before any project discovery; every real run has one owned cancellation flag.
pub fn run() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    let locale = resolve_locale_from_environment(bootstrap_lang(&args).as_deref());
    let localizer = match Localizer::try_new(locale) {
        Ok(localizer) => localizer,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(3);
        }
    };
    let matches = localized(&localizer).get_matches_from(args);
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    let initial =
        configuration::inspect(&cli.project_dir, cli.application.as_deref()).map_err(Failure::from);
    let mut report_file = match initial
        .as_ref()
        .ok()
        .zip(cli.report.as_deref())
        .map(|(plan, path)| open_report(path, plan))
        .transpose()
    {
        Ok(file) => file,
        Err(source) => return emit_report_error(&cli, &localizer, &source),
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancelled);
    let result = ctrlc::set_handler(move || signal.store(true, Ordering::Release))
        .map_err(|error| {
            Failure::Configuration(eska_analyzer::Error::ProcessIo(io::Error::other(error)))
        })
        .and(initial)
        .and_then(|plan| execute(&cli, &plan, &cancelled));
    let (document, code) = match &result {
        Ok(report) => (
            report::document(report),
            u8::from(report.counts.exceeds(&report.policy)),
        ),
        Err(error) => (
            report::failure(error),
            if error.code() == "cancelled" { 130 } else { 3 },
        ),
    };
    let Ok(mut output) = serde_json::to_vec(&document) else {
        eprintln!("{}", localizer.text("check-output-error"));
        return ExitCode::from(3);
    };
    output.push(b'\n');
    if let Some(file) = &mut report_file
        && let Err(source) = file.write_all(&output).and_then(|()| file.sync_all())
    {
        return emit_report_error(&cli, &localizer, &source);
    }
    let written = match cli.format {
        Format::Json => io::stdout().lock().write_all(&output),
        Format::Human => result.as_ref().map_or(Ok(()), |report| {
            report::human(&mut io::stdout().lock(), report, &localizer)
        }),
    };
    if written.is_err() {
        eprintln!("{}", localizer.text("check-output-error"));
        return ExitCode::from(3);
    }
    if let Err(error) = &result {
        report::human_error(error, &localizer);
    }
    ExitCode::from(code)
}

/// Prepare only owned derived data, then run the same complete context without a project subprocess.
fn execute(
    cli: &Cli,
    initial: &configuration::Plan,
    cancelled: &AtomicBool,
) -> Result<Report, Failure> {
    let analyzer = Analyzer {
        binary: cli
            .analyzer
            .clone()
            .or_else(|| std::env::var_os("ESKA_BSL_ANALYZER").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("bsl-analyzer-app")),
        timeout: Duration::from_secs(cli.timeout),
        cancelled,
        connection_closed: None,
    };
    let plan = configuration::prepare(initial, &analyzer, false)?;
    check::analyze(&plan, &analyzer)
}

/// An explicit report path creates a new file; an existing source or earlier report is preserved.
fn open_report(path: &Path, plan: &configuration::Plan) -> io::Result<File> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent.canonicalize()?;
    if plan
        .context
        .sources()
        .iter()
        .any(|source| parent.starts_with(source.project().source()))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "report inside project source",
        ));
    }
    OpenOptions::new().write(true).create_new(true).open(path)
}

/// A report write failure takes precedence over a clean analysis because CI received no durable result.
fn emit_report_error(cli: &Cli, localizer: &Localizer, source: &io::Error) -> ExitCode {
    if matches!(cli.format, Format::Json) {
        let document = json!({"schema_version":1,"kind":"check","status":"incomplete","complete":false,"error":{"code":"report_write","data":{"path":cli.report.as_deref().map(report::path),"io_kind":source.kind().to_string()}}});
        let _ = writeln!(io::stdout().lock(), "{document}");
    }
    eprintln!("{}", localizer.text("check-report-error"));
    ExitCode::from(3)
}

/// Localize the entire command help before clap handles early help or argument failures.
fn localized(localizer: &Localizer) -> clap::Command {
    let mut command = Cli::command()
        .version(env!("CARGO_PKG_VERSION"))
        .about(localizer.text("check-tool-about"))
        .help_template(format!(
            "{{name}} {{version}}\n{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{options}}",
            localizer.text("cli-usage"),
            localizer.text("cli-options")
        ));
    for (name, key) in [
        ("lang", "cli-lang-help"),
        ("project_dir", "check-project-dir-help"),
        ("application", "project-application-help"),
        ("analyzer", "analyzer-binary-help"),
        ("timeout", "analyzer-timeout-help"),
        ("format", "version-format-help"),
        ("report", "check-report-help"),
        ("help", "cli-help"),
        ("version", "cli-version"),
    ] {
        command = command.mut_arg(name, |arg| arg.help(localizer.text(key)));
    }
    command
}
