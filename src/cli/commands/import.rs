//! Update one project's sources from a native artifact after preview and consent.

use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Args, ValueEnum};
use serde_json::{Value, json};

use crate::{
    cli::{
        diagnostics,
        encoding::json_path,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        artifact::{Identity, ImportPlan, PreparedArtifact},
        discovery,
        selection::{self, SelectionIntent},
    },
};

use super::artifact::{self, Failure, PlatformArgs};

#[derive(Debug, Args)]
pub(in crate::cli) struct ImportArgs {
    file: PathBuf,
    #[arg(short = 'p', long)]
    project: Option<String>,
    #[arg(long)]
    force: bool,
    #[arg(long, conflicts_with = "force")]
    dry_run: bool,
    #[arg(long, value_enum, default_value_t = Format::Human)]
    format: Format,
    #[command(flatten)]
    platform: PlatformArgs,
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum Format {
    #[default]
    Human,
    Json,
}

impl ImportArgs {
    /// Emit one versioned JSON result on stdout and localized diagnostics on stderr.
    pub(super) fn run(&self, base: &Path, localizer: &Localizer) -> ExitCode {
        let (document, code) = match self.execute(base, localizer) {
            Ok(document) => (document, ExitCode::SUCCESS),
            Err(error) => {
                eprintln!("{}", error.message);
                (
                    json!({"schema_version":1,"kind":"import","error":{"code":error.code},"preview":error.details}),
                    ExitCode::FAILURE,
                )
            }
        };
        if self.format == Format::Json {
            let result = serde_json::to_writer(io::stdout().lock(), &document)
                .map_err(io::Error::other)
                .and_then(|()| writeln!(io::stdout().lock()));
            if result.is_err() {
                eprintln!("{}", localizer.text("artifact-output-error"));
                return ExitCode::FAILURE;
            }
        }
        code
    }

    /// Keep source selection, validation, confirmation and publication strictly ordered.
    fn execute(&self, base: &Path, localizer: &Localizer) -> Result<Value, Failure> {
        let context = discovery::discover_context(base).map_err(|error| {
            Failure::new(
                "discovery",
                diagnostics::present_context_error(&error, localizer),
            )
        })?;
        let names = self.project.iter().cloned().collect::<Vec<_>>();
        let selected =
            selection::select_projects(&context, &names, false, SelectionIntent::SingleMutation)
                .map_err(|error| {
                    Failure::new(
                        "project-selection",
                        diagnostics::present_selection_error(&error, localizer),
                    )
                })?;
        let selected = selected.projects().first().ok_or_else(|| {
            Failure::new(
                "project-selection",
                localizer.text("project-selector-single-required"),
            )
        })?;
        let project = selected.project();
        let plan =
            ImportPlan::inspect(project).map_err(|error| artifact::present(&error, localizer))?;
        let tool = self.platform.resolve(
            project.configuration().build_settings().platform_version(),
            self.format == Format::Human,
            localizer,
        )?;
        let prepared = PreparedArtifact::unpack(
            &base.join(&self.file),
            project.source(),
            &tool,
            |_, line| {
                eprintln!("{}", String::from_utf8_lossy(line));
            },
        )
        .map_err(|error| artifact::present(&error, localizer))?;
        let mut document = preview(
            &plan,
            &prepared,
            selected.name().map(crate::project::ProjectName::as_str),
            project.source(),
        );
        let confirmation = plan
            .requires_confirmation(prepared.identity())
            .map_err(|error| artifact::present(&error, localizer).with_details(document.clone()))?;
        if self.format == Format::Human {
            present_preview(&plan, &prepared, project.source(), localizer);
        }
        if self.dry_run {
            return Ok(document);
        }
        if confirmation && !self.force {
            confirm(self.format, localizer, &tool)
                .map_err(|error| error.with_details(document.clone()))?;
        }
        if tool.was_interrupted() {
            return Err(Failure::new(
                "cancelled",
                localizer.text("artifact-cancelled"),
            ));
        }
        let result = plan
            .apply(&prepared, confirmation)
            .map_err(|error| artifact::present(&error, localizer))?;
        document["applied"] = json!(true);
        if let Some(backup) = result.retained_backup {
            eprintln!(
                "{}",
                localizer.format(
                    "artifact-backup-retained",
                    &[("path", LocalizationValue::Text(&backup.to_string_lossy()))]
                )
            );
            document["retained_backup"] = path_document(&backup);
        }
        if self.format == Format::Human {
            println!("{}", localizer.text("artifact-imported"));
        }
        Ok(document)
    }
}

/// Render identity values with locale-independent keys and stable project-type names.
fn identity_document(identity: &Identity) -> Value {
    json!({"type":identity.project_type.as_str(),"uuid":identity.uuid,"name":identity.name})
}

/// Use the shared reversible encoding for non-Unicode filesystem paths.
fn path_document(path: &Path) -> Value {
    let (path, encoding) = json_path(path.as_os_str());
    json!({"path":path,"path_encoding":encoding})
}

/// Describe the complete replacement scope before any filesystem mutation.
fn preview(
    plan: &ImportPlan,
    artifact: &PreparedArtifact,
    member: Option<&str>,
    source: &Path,
) -> Value {
    json!({"schema_version":1,"kind":"import","applied":false,"project":member,
        "source":path_document(source),"current":plan.current().map(identity_document),
        "incoming":identity_document(artifact.identity()),"existing_files":plan.files(),
        "local_changes":plan.local_changes(),"identity_changed":plan.current().is_some_and(|old| old != artifact.identity()),
        "retained_backup":null})
}

/// Keep the destructive scope and both identities visible while the user answers.
fn present_preview(
    plan: &ImportPlan,
    artifact: &PreparedArtifact,
    source: &Path,
    localizer: &Localizer,
) {
    eprintln!(
        "{}",
        localizer.format(
            "artifact-replace-preview",
            &[
                ("path", LocalizationValue::Text(&source.to_string_lossy())),
                ("count", LocalizationValue::Text(&plan.files().to_string()))
            ]
        )
    );
    if let Some(current) = plan.current() {
        eprintln!(
            "{}",
            localizer.format(
                "artifact-current",
                &[
                    ("name", LocalizationValue::Text(&current.name)),
                    ("uuid", LocalizationValue::Text(&current.uuid))
                ]
            )
        );
    }
    let incoming = artifact.identity();
    eprintln!(
        "{}",
        localizer.format(
            "artifact-incoming",
            &[
                ("name", LocalizationValue::Text(&incoming.name)),
                ("uuid", LocalizationValue::Text(&incoming.uuid))
            ]
        )
    );
    if plan.current().is_some_and(|current| current != incoming) {
        eprintln!("{}", localizer.text("artifact-identity-warning"));
    }
    if plan.local_changes() != 0 {
        eprintln!("{}", localizer.text("artifact-local-warning"));
    }
}

/// Require a deliberate overwrite answer; Enter, EOF and every other answer cancel.
fn confirm(
    format: Format,
    localizer: &Localizer,
    tool: &crate::project::build::Ibcmd,
) -> Result<(), Failure> {
    if format == Format::Json || !artifact::terminal() {
        return Err(Failure::new(
            "confirmation-required",
            localizer.text("artifact-confirmation-required"),
        ));
    }
    eprintln!("{}", localizer.text("artifact-confirm"));
    let confirmed = crate::cli::interactive::confirm_overwrite(|| tool.was_interrupted())
        .map_err(|error| artifact::prompt_error(error, localizer))?;
    if confirmed {
        Ok(())
    } else {
        Err(Failure::new(
            "cancelled",
            localizer.text("artifact-cancelled"),
        ))
    }
}

/// Localize every public option while retaining machine-facing flag names.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    artifact::localize(command, localizer)
        .about(localizer.text("artifact-import-about"))
        .override_usage(localizer.text("artifact-import-usage"))
        .help_template(format!(
            "{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{positionals}}\n\n{}:\n{{options}}",
            localizer.text("cli-usage"),
            localizer.text("cli-arguments"),
            localizer.text("cli-options")
        ))
        .mut_arg("file", |arg| {
            arg.help(localizer.text("artifact-file-help"))
                .value_name(localizer.text("artifact-file-value"))
        })
        .mut_arg("project", |arg| {
            arg.help(localizer.text("artifact-project-help"))
                .value_name(localizer.text("version-project-value"))
        })
        .mut_arg("force", |arg| {
            arg.help(localizer.text("artifact-force-help"))
        })
        .mut_arg("dry_run", |arg| {
            arg.help(localizer.text("artifact-dry-run-help"))
        })
        .mut_arg("format", |arg| {
            arg.help(localizer.text("build-format-help"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
