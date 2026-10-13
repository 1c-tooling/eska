//! Shared argument presentation and locale-independent tool envelopes; no business operations.

use super::localization::{LocalizationValue, Localizer};
use crate::project_core::source_format::{AcceptedRef, Error};
use clap::{ArgMatches, Command, error::ErrorKind};
use serde_json::{Value, json};
use std::{
    ffi::{OsStr, OsString},
    io::{self, Write},
    process::ExitCode,
};

/// The representation is selected at the process boundary, never in a source core.
#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

/// A presentation failure retains stable domain coordinates and optional diagnostic data.
pub struct Failure {
    pub code: &'static str,
    pub location: Vec<String>,
    pub diagnostics: Value,
}

impl Failure {
    /// Construct a stable failure without embedding a human-language message in JSON.
    #[must_use]
    pub const fn new(code: &'static str) -> Self {
        Self {
            code,
            location: Vec::new(),
            diagnostics: Value::Null,
        }
    }

    /// Attach a filesystem or selector coordinate without changing the core error code.
    #[must_use]
    pub fn at(mut self, location: impl Into<String>) -> Self {
        self.location.push(location.into());
        self
    }
}

impl From<Error> for Failure {
    /// Source failures already have stable codes and document coordinates.
    fn from(error: Error) -> Self {
        Self {
            code: error.code,
            location: error.location,
            diagnostics: Value::Null,
        }
    }
}

/// Read only a global bootstrap option before clap chooses a locale or output representation.
pub fn bootstrap_value<T: AsRef<OsStr>>(args: &[T], name: &str) -> Option<String> {
    let mut arguments = args.iter().skip(1);
    let prefix = format!("{name}=");
    while let Some(argument) = arguments.next() {
        let argument = argument.as_ref();
        if argument == "--" {
            break;
        }
        if argument == name {
            return arguments
                .next()
                .and_then(|value| value.as_ref().to_str())
                .map(str::to_owned);
        }
        if let Some(argument) = argument.to_str()
            && let Some(value) = argument.strip_prefix(&prefix)
        {
            return Some(value.to_owned());
        }
    }
    None
}

/// Apply the same translated help and argument metadata to the root and every subcommand.
pub fn localize(mut command: Command, localizer: &Localizer, prefix: &str) -> Command {
    let name = command.get_name().to_owned();
    let about = if name.starts_with("eska-") {
        format!("{prefix}-about")
    } else {
        format!("{prefix}-command-{name}")
    };
    let subcommands = if command.get_subcommands().next().is_some() {
        format!("\n{}:\n{{subcommands}}", localizer.text("cli-commands"))
    } else {
        String::new()
    };
    command = command.about(localizer.text(&about)).help_template(format!(
        "{{before-help}}{{name}} {{version}}\n{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{positionals}}{subcommands}\n{}:\n{{options}}{{after-help}}",
        localizer.text("cli-usage"), localizer.text("tool-arguments"), localizer.text("cli-options"),
    ));
    let arguments: Vec<_> = command
        .get_arguments()
        .map(|argument| argument.get_id().as_str().to_owned())
        .collect();
    for id in arguments {
        let key = match id.as_str() {
            "help" => "cli-help".to_owned(),
            "version" => "cli-version".to_owned(),
            "lang" => "cli-lang-help".to_owned(),
            "project_dir" => "cli-project-dir-help".to_owned(),
            _ => format!("tool-option-{}", id.replace('_', "-")),
        };
        command = command.mut_arg(&id, |arg| {
            let takes_values = arg.get_action().takes_values();
            let arg = arg.help(localizer.text(&key));
            if takes_values {
                arg.hide_default_value(true).hide_possible_values(true)
            } else {
                arg
            }
        });
    }
    command.mut_subcommands(|subcommand| localize(subcommand, localizer, prefix))
}

/// Parse safely: help/version succeed, other argument failures use the tool's stable envelope.
///
/// # Errors
/// Returns the already-presented exit code for help, version, invalid arguments or output failure.
pub fn parse(
    command: Command,
    args: Vec<OsString>,
    localizer: &Localizer,
    tool: &str,
) -> Result<ArgMatches, ExitCode> {
    let json = bootstrap_value(&args, "--format").as_deref() == Some("json");
    match command.try_get_matches_from(args) {
        Ok(matches) => Ok(matches),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Err(if error.print().is_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        Err(error) => {
            let failure = Failure {
                diagnostics: json!({"parser": format!("{:?}", error.kind())}),
                ..Failure::new("arguments_invalid")
            };
            let format = if json {
                OutputFormat::Json
            } else {
                OutputFormat::Human
            };
            Err(emit_failure(tool, None, format, &failure, localizer))
        }
    }
}

/// Resolve an independent caller pin; editable project TOML cannot approve its own baseline.
///
/// # Errors
/// Returns a missing pin or a value other than exactly 64 hexadecimal characters.
pub fn accepted_ref(explicit: Option<&str>) -> Result<AcceptedRef, Failure> {
    let environment = std::env::var("ESKA_ACCEPTED_BASE_SHA256").ok();
    let value = explicit
        .or(environment.as_deref())
        .ok_or_else(|| Failure::new("accepted_base_pin_required"))?;
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Failure::new("accepted_base_pin_invalid"));
    }
    Ok(AcceptedRef {
        sha256: value.to_ascii_lowercase(),
    })
}

/// Write a success envelope only after the owning operation has completed.
#[cfg(any(feature = "project-cli", feature = "build-cli"))]
pub fn emit_success(
    tool: &str,
    command: &str,
    format: OutputFormat,
    data: &Value,
    human: &str,
) -> Result<ExitCode, Failure> {
    let output = match format {
        OutputFormat::Human => format!("{human}\n").into_bytes(),
        OutputFormat::Json => json_bytes(&json!({"schema_version":1, "tool":tool,
            "command":command, "status":"success", "data":data}))?,
    };
    io::stdout()
        .lock()
        .write_all(&output)
        .map_err(|_| Failure::new("tool_io"))?;
    Ok(ExitCode::SUCCESS)
}

/// Keep error stdout machine-only while localizing the corresponding stderr message.
pub fn emit_failure(
    tool: &str,
    command: Option<&str>,
    format: OutputFormat,
    failure: &Failure,
    localizer: &Localizer,
) -> ExitCode {
    let location = failure.location.join(" / ");
    let message = localizer.format(
        "tool-failure",
        &[
            ("code", LocalizationValue::Text(failure.code)),
            ("location", LocalizationValue::Text(&location)),
        ],
    );
    let _ = writeln!(io::stderr().lock(), "{message}");
    if let Some(key) = failure_hint(failure.code) {
        let _ = writeln!(io::stderr().lock(), "{}", localizer.text(key));
    }
    if matches!(format, OutputFormat::Json)
        && let Ok(bytes) = json_bytes(&json!({"schema_version":1, "tool":tool, "command":command,
            "status":"error", "error":{"code":failure.code, "location":failure.location,
            "diagnostics":failure.diagnostics}}))
    {
        let _ = io::stdout().lock().write_all(&bytes);
    }
    ExitCode::FAILURE
}

/// Explain common actionable failures without placing human text in the error envelope.
fn failure_hint(code: &str) -> Option<&'static str> {
    match code {
        "arguments_invalid" => Some("tool-error-arguments"),
        "accepted_base_pin_required" => Some("tool-error-pin-required"),
        "accepted_base_pin_invalid" => Some("tool-error-pin-invalid"),
        "source_baseline_untrusted" => Some("tool-error-base-untrusted"),
        "source_baseline_corrupt" => Some("tool-error-base-corrupt"),
        "source_destination_exists" => Some("tool-error-destination-exists"),
        "source_format_mismatch" => Some("tool-error-format"),
        "source_migration_scope_unsupported" => Some("tool-error-migration-scope"),
        "source_migration_incomplete" => Some("tool-error-migration-incomplete"),
        "source_update_scope_unsupported" => Some("tool-error-update-scope"),
        "source_update_conflicts" => Some("tool-error-update-conflicts"),
        "source_update_profile_mismatch" => Some("tool-error-update-profile"),
        "source_update_members_mismatch" => Some("tool-error-update-members"),
        "source_update_declarations_changed" => Some("tool-error-update-declarations"),
        "source_update_output_mismatch" | "source_update_receipt_invalid" => {
            Some("tool-error-update-recovery")
        }
        "source_update_incomplete" => Some("tool-error-update-incomplete"),
        "source_workspace_pins_required" => Some("tool-error-workspace-pins-required"),
        "source_workspace_pins_invalid" => Some("tool-error-workspace-pins-invalid"),
        "source_workspace_pins_mismatch" => Some("tool-error-workspace-pins-mismatch"),
        "project_selection_failed" => Some("tool-error-selection"),
        "project_discovery_failed" => Some("tool-error-discovery"),
        "analysis_config_invalid" => Some("tool-error-analysis-config"),
        "analysis_scope_unsupported" => Some("tool-error-analysis-scope"),
        "source_changes_excluded" => Some("tool-error-exclusions"),
        "platform_version_missing" => Some("tool-error-platform-version-missing"),
        "project_version_policy_invalid" => Some("tool-error-project-version-policy"),
        "platform_not_found"
        | "platform_version_mismatch"
        | "platform_executable_invalid"
        | "platform_version_command_failed"
        | "platform_version_unreadable"
        | "platform_container_required"
        | "platform_container_invalid"
        | "platform_architecture_invalid" => Some("tool-error-platform"),
        "build_source_options_incompatible" => Some("tool-error-source-options"),
        "machine_config_invalid"
        | "machine_config_container_missing"
        | "machine_config_container_unexpected" => Some("tool-error-machine-config"),
        _ => None,
    }
}

/// Encode one line-delimited JSON value with stable keys and no localized text.
fn json_bytes(value: &Value) -> Result<Vec<u8>, Failure> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| Failure::new("tool_json"))?;
    bytes.push(b'\n');
    Ok(bytes)
}
