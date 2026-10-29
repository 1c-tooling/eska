//! Derived analyzer config commands expose preview, freshness and explicit migration separately.

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use clap::{Args, Subcommand};
use eska_analyzer::{
    Error,
    configuration::{self, Plan, Status},
    migration,
    process::Analyzer,
};
use eska_presentation::localization::{LocalizationValue, Localizer};
use serde_json::{Value, json};

use crate::{cli::OutputFormat, response};

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    command: Action,
    #[arg(long, global = true)]
    application: Option<String>,
    #[arg(long, global = true)]
    analyzer: Option<PathBuf>,
    #[arg(long,global=true,default_value_t=60,value_parser=clap::value_parser!(u64).range(1..))]
    timeout: u64,
    #[arg(long,global=true,value_enum,default_value_t=OutputFormat::Human)]
    format: OutputFormat,
    #[arg(short,long,global=true,action=clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Subcommand)]
enum Action {
    #[command(disable_help_flag = true)]
    Status,
    #[command(disable_help_flag = true)]
    Prepare {
        #[arg(long)]
        switch_context: bool,
    },
    #[command(disable_help_flag = true)]
    Migrate {
        #[arg(long, requires = "preview_token")]
        apply: bool,
        #[arg(long, requires = "apply")]
        preview_token: Option<String>,
    },
}

impl ConfigArgs {
    /// Freshness performs no writes; prepare and explicit migration install their own cancellation hook.
    pub fn run(&self, start: &Path, localizer: &Localizer) -> ExitCode {
        if matches!(self.command, Action::Status) {
            return match configuration::inspect(start, self.application.as_deref()) {
                Ok(plan) => {
                    report(&plan, false, self.format, localizer);
                    if plan.status == Status::Current {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(error) => failure(&error, self.format, localizer),
            };
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&cancelled);
        if let Err(error) = ctrlc::set_handler(move || signal.store(true, Ordering::Release)) {
            return failure(
                &Error::ProcessIo(std::io::Error::other(error)),
                self.format,
                localizer,
            );
        }
        let analyzer = Analyzer {
            binary: self
                .analyzer
                .clone()
                .or_else(|| std::env::var_os("ESKA_BSL_ANALYZER").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("bsl-analyzer-app")),
            timeout: Duration::from_secs(self.timeout),
            cancelled: &cancelled,
            connection_closed: None,
        };
        match self.execute(start, &analyzer, localizer) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => failure(&error, self.format, localizer),
        }
    }

    /// A token is recomputed from actual files before migration; it never authorizes different inputs.
    fn execute(
        &self,
        start: &Path,
        analyzer: &Analyzer<'_>,
        localizer: &Localizer,
    ) -> Result<(), Error> {
        match &self.command {
            Action::Status => {}
            Action::Prepare { switch_context } => {
                let plan = configuration::inspect(start, self.application.as_deref())?;
                let prepared = configuration::prepare(&plan, analyzer, *switch_context)?;
                report(
                    &prepared,
                    plan.status != Status::Current,
                    self.format,
                    localizer,
                );
            }
            Action::Migrate {
                apply,
                preview_token,
            } => {
                let preview = migration::preview(start, self.application.as_deref())?;
                if *apply {
                    let (plan, backup) = migration::apply(
                        &preview,
                        preview_token.as_deref().unwrap_or_default(),
                        analyzer,
                    )?;
                    if matches!(self.format, OutputFormat::Json) {
                        println!(
                            "{}",
                            json!({"schema_version":1,"kind":"analyzer-migration","status":"migrated","backup":response::path(&backup),"configuration":document(&plan,true)})
                        );
                    } else {
                        println!(
                            "{}",
                            response::path_message(
                                localizer,
                                "analyzer-migration-complete",
                                &backup
                            )
                        );
                    }
                } else {
                    migration::validate_preview(&preview, analyzer)?;
                    if matches!(self.format, OutputFormat::Json) {
                        println!(
                            "{}",
                            json!({"schema_version":1,"kind":"analyzer-migration","status":"preview","preview":preview})
                        );
                    } else {
                        println!("{}", localizer.text("analyzer-migration-preview"));
                        println!("{}", preview.proposed_manifest);
                        for field in &preview.conflicts {
                            println!(
                                "{}",
                                localizer.format(
                                    "analyzer-migration-decision",
                                    &[("field", LocalizationValue::Text(field))]
                                )
                            );
                        }
                        println!(
                            "{}",
                            localizer.format(
                                "analyzer-migration-token",
                                &[("token", LocalizationValue::Text(&preview.token))]
                            )
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

/// Use the same generated fingerprint and path envelope in CLI and the project server.
pub fn document(plan: &Plan, changed: bool) -> Value {
    json!({"schema_version":1,"kind":"analyzer-configuration","status":plan.status,"changed":changed,"path":response::path(&plan.path()),"ownership":plan.ownership,"preview_token":plan.token,"limited_context":plan.context.limited_context()})
}

/// A freshness result is actionable without implying successful BSL analysis.
fn report(plan: &Plan, changed: bool, format: OutputFormat, localizer: &Localizer) {
    if matches!(format, OutputFormat::Json) {
        println!("{}", document(plan, changed));
    } else {
        println!(
            "{}",
            response::path_message(
                localizer,
                &format!("analyzer-config-status-{}", status_key(plan.status)),
                &plan.path()
            )
        );
    }
}

/// Keep localization keys aligned with the fixed serialized status vocabulary.
const fn status_key(status: Status) -> &'static str {
    match status {
        Status::Current => "current",
        Status::Missing => "missing",
        Status::Stale => "stale",
        Status::MigrationRequired => "migration-required",
        Status::ManagedConflict => "managed-conflict",
        Status::ContextConflict => "context-conflict",
    }
}

/// Retain upstream diagnostics and reversible paths without putting translated text into JSON.
pub fn error_document(error: &Error) -> Value {
    let data = match error {
        Error::Io { path, source } => {
            json!({"path":response::path(path),"io_kind":source.kind().to_string()})
        }
        Error::InvalidToml { path, detail } => json!({"path":response::path(path),"detail":detail}),
        Error::InvalidPolicy { field } => json!({"field":field}),
        Error::NonPortablePath { path }
        | Error::InputTooLarge { path }
        | Error::ManagedConflict { path }
        | Error::MigrationRequired { path }
        | Error::Busy { path } => json!({"path":response::path(path)}),
        Error::ContextConflict { current, requested } => {
            json!({"current":current,"requested":requested})
        }
        Error::MigrationConflicts { fields } => json!({"fields":fields}),
        Error::ProcessStart { binary, source } => {
            json!({"binary":response::path(binary),"io_kind":source.kind().to_string()})
        }
        Error::AnalyzerVersion { actual } => {
            json!({"actual":actual})
        }
        Error::CheckConfig { exit_code, .. } => json!({"exit_code":exit_code}),
        Error::Context(error) => match error.as_ref() {
            eska_project_core::application::ContextError::UnknownApplication {
                requested,
                available,
            } => {
                json!({"reason":"unknown_application","requested":requested,"available":available})
            }
            eska_project_core::application::ContextError::AmbiguousApplication { available } => {
                json!({"reason":"ambiguous_application","available":available})
            }

            _ => json!({}),
        },
        Error::Rollback { paths, original } => {
            json!({"paths":paths.iter().map(|path|response::path(path)).collect::<Vec<_>>(),"original":error_document(original)})
        }
        _ => json!({}),
    };
    json!({"code":error.code(),"data":data})
}

/// Exit 130 distinguishes explicit cancellation from failed preparation.
fn failure(error: &Error, format: OutputFormat, localizer: &Localizer) -> ExitCode {
    if matches!(format, OutputFormat::Json) {
        println!(
            "{}",
            json!({"schema_version":1,"kind":"analyzer-configuration","status":"error","error":error_document(error)})
        );
    }
    if let Error::Context(error) = error {
        eprintln!(
            "{}",
            crate::context::failure(error, Path::new("."), localizer).message()
        );
    } else {
        eprintln!(
            "{}",
            localizer.text(&format!("analyzer-error-{}", error.code()))
        );
    }
    if let Error::CheckConfig { stdout, stderr, .. } = error {
        eprintln!("{stdout}{stderr}");
    }
    if matches!(error, Error::Cancelled) {
        ExitCode::from(130)
    } else {
        ExitCode::FAILURE
    }
}

/// Localize every operation and option while clap derives usage from the actual component name.
pub fn localize(mut command: clap::Command, localizer: &Localizer) -> clap::Command {
    let template = format!(
        "{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{subcommands}}\n{}:\n{{options}}",
        localizer.text("cli-usage"),
        localizer.text("cli-commands"),
        localizer.text("cli-options")
    );
    command = command
        .about(localizer.text("analyzer-config-about"))
        .help_template(template.clone());
    for (name, key) in [
        ("application", "project-application-help"),
        ("analyzer", "analyzer-binary-help"),
        ("timeout", "analyzer-timeout-help"),
        ("format", "version-format-help"),
        ("help", "cli-help"),
    ] {
        command = command.mut_arg(name, |argument| argument.help(localizer.text(key)));
    }
    command.mut_subcommands(|mut command| {
        let about = localizer.text(&format!("analyzer-config-{}-about", command.get_name()));
        command = command.about(about).help_template(template.clone());
        for (name, key) in [
            ("switch_context", "analyzer-switch-context-help"),
            ("apply", "analyzer-migration-apply-help"),
            ("preview_token", "analyzer-preview-token-help"),
        ] {
            if command
                .get_arguments()
                .any(|argument| argument.get_id() == name)
            {
                command = command.mut_arg(name, |argument| argument.help(localizer.text(key)));
            }
        }
        command
    })
}
