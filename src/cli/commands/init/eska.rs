//! Explicit ESKA attachment delegates source ownership and keeps parent onboarding failures inspectable.

mod policy;
mod publication;

use super::{
    InitArgs,
    artifact::{self, Failure},
};
use crate::{
    cli::{
        commands::new::eska::{
            process,
            protocol::{self, Created, Operation},
        },
        companion::{Capability, Kind, Tool},
        localization::{LocalizationValue, Localizer},
        path_output,
    },
    project_core::source_format::Error,
    tool_cli::OutputFormat,
};
use serde_json::{Value, json};
use std::{
    io::{self, IsTerminal},
    path::Path,
    process::ExitCode,
};

/// Attachment returns one final success after child integrity and ordinary parent onboarding complete.
pub(super) fn run(args: &InitArgs, base: &Path, localizer: &Localizer) -> ExitCode {
    match execute(args, base, localizer) {
        Ok(data) => {
            if matches!(args.format, OutputFormat::Json) {
                println!(
                    "{}",
                    json!({"schema_version":1,"tool":"eska","command":"init","status":"success","data":data})
                );
            } else {
                artifact::success(&localizer.format(
                    "init-eska-created",
                    &[(
                        "path",
                        LocalizationValue::Text(&path_output::render(
                            Path::new(data["root"].as_str().unwrap_or_default()),
                            io::stdout().is_terminal(),
                        )),
                    )],
                ));
                pins(&data, localizer);
            }
            ExitCode::SUCCESS
        }
        Err(error) => report_failure(args, &error, localizer),
    }
}

/// Existing input is never a cleanup target, including failed or malformed child acknowledgments.
fn execute(args: &InitArgs, base: &Path, localizer: &Localizer) -> Result<Value, Failure> {
    let plan = policy::prepare(args, base, localizer)?;
    let tool =
        Tool::discover(Kind::Project).map_err(|error| companion_failure(error, localizer))?;
    tool.verify(localizer.locale().as_str(), Capability::Init)
        .map_err(|error| companion_failure(error, localizer))?;
    plan.verify_workspace(localizer)?;
    let mut created = initialize(&tool, &plan, localizer).map_err(|mut error| {
        let details = error.details.get_or_insert_with(|| json!({}));
        details["root"] = json!(plan.core.root());
        details["existing_root"] = json!(true);
        details["project_initialized"] = Value::Null;
        details["onboarding_complete"] = json!(false);
        details["source_verified"] = json!(false);
        details["support_approved"] = json!(false);
        details["destination_removed"] = json!(false);
        details["recovery"] = json!("inspect_retained_existing_directory_and_child_error");
        error
    })?;
    let result = publication::finish(&plan, args, &mut created, localizer);
    result.map_err(|error| {
        let mut details = protocol::unfinished(&created, error.code, false);
        details["existing_root"] = json!(true);
        details["project_created"] = json!(false);
        details["project_initialized"] = json!(true);
        details["recovery"] = json!("inspect_retained_project_and_complete_onboarding");
        if let Some(extra) = error.details {
            details["detail"] = extra;
        }
        Failure {
            details: Some(details),
            ..error
        }
    })?;
    Ok(
        json!({"root":created.root,"name":created.name.as_str(),"source":created.root.join(&created.source_relative),
        "type":plan.core.project().configuration().project_type().as_str(),"metadata_name":plan.core.metadata_name(),
        "version":plan.core.version().to_string(),"version_scheme":plan.core.version().scheme().as_str(),
        "source_format":"eska","source_layout":"queries","source_layout_version":plan.core.source_layout_version(),
        "accepted_base":".eska/accepted","accepted_base_sha256":created.accepted.sha256,"ancestor_sha256":created.ancestor.sha256(),
        "manifest_sha256":created.manifest_sha256,"accepted_base_created":plan.core.accepted_base_created(),"ancestry_preserved":plan.core.ancestry_preserved(),
        "existing_root":true,"project_initialized":true,"onboarding_complete":true,"source_verified":true,"support_approved":false,"workspace_member":plan.enrollment.is_some()}),
    )
}

/// The direct tool receives only source-init inputs; workflow and Git remain parent-owned policy.
fn initialize(tool: &Tool, plan: &policy::Plan, localizer: &Localizer) -> Result<Created, Failure> {
    let mut command = tool.command();
    command
        .args([
            "--lang",
            localizer.locale().as_str(),
            "--format",
            "json",
            "init",
        ])
        .arg(plan.core.root())
        .args([
            "--name",
            plan.name.as_str(),
            "--source",
            plan.source.as_str(),
        ]);
    if let Some(platform) = &plan.platform {
        command.args(["--platform-version", platform.as_str()]);
    }
    if let Some(approved) = &plan.approved {
        command.args(["--accepted-base-sha256", &approved.sha256]);
    }
    let captured = process::capture(&mut command, 128 * 1024)
        .map_err(|_| failure("init-eska-child-transport-failed", localizer))?;
    if !captured.status.success() {
        let child = serde_json::from_slice::<Value>(&captured.stdout).ok();
        return Err(
            failure("init-eska-child-failed", localizer).with_details(json!({"child_error":child}))
        );
    }
    protocol::created(
        &captured,
        Operation::Init,
        plan.core.root(),
        &plan.name,
        plan.source.as_path(),
    )
    .map_err(|_| failure("init-eska-child-response-invalid", localizer))
}

/// Source failures retain their stable code and coordinates while human presentation is localized.
fn source_failure(error: &Error, localizer: &Localizer) -> Failure {
    Failure::new(
        error.code,
        localizer.format(
            "init-eska-source-error",
            &[
                ("code", LocalizationValue::Text(error.code)),
                (
                    "location",
                    LocalizationValue::Text(&error.location.join(" / ")),
                ),
            ],
        ),
    )
    .with_details(json!({"location":error.location}))
}

/// Companion errors share the existing creation boundary and never retry an incompatible executable.
fn companion_failure(error: crate::cli::companion::Error, localizer: &Localizer) -> Failure {
    use crate::cli::companion::Error;
    let code = match error {
        Error::Missing => "new-companion-missing",
        Error::Unavailable => "new-companion-unavailable",
        Error::ProbeFailed => "new-companion-probe-failed",
        Error::ProbeTimeout => "new-companion-probe-timeout",
        Error::ProbeOversized => "new-companion-probe-oversized",
        Error::Incompatible => "new-companion-incompatible",
    };
    Failure::new(
        code,
        localizer.format(
            code,
            &[("tool", LocalizationValue::Text(Kind::Project.name()))],
        ),
    )
}

/// Candidate output identifies preserved/unapproved evidence without choosing source or support trust.
fn pins(data: &Value, localizer: &Localizer) {
    if let (Some(accepted), Some(ancestor)) = (
        data["accepted_base_sha256"].as_str(),
        data["ancestor_sha256"].as_str(),
    ) {
        eprintln!(
            "{}",
            localizer.format(
                "new-eska-candidates",
                &[
                    ("accepted", LocalizationValue::Text(accepted)),
                    ("ancestor", LocalizationValue::Text(ancestor))
                ]
            )
        );
    }
}

/// Stable option errors use exit 2; post-child failures report retained initialized data explicitly.
pub(super) fn report_failure(args: &InitArgs, error: &Failure, localizer: &Localizer) -> ExitCode {
    if matches!(args.format, OutputFormat::Json) {
        println!(
            "{}",
            json!({"schema_version":1,"tool":"eska","command":"init","status":"error","error":{"code":error.code},"diagnostics":error.details})
        );
    } else {
        error.report();
        if let Some(details) = &error.details {
            if let Some(root) = details["root"].as_str() {
                artifact::report(
                    "!",
                    &localizer.format(
                        "init-eska-unfinished",
                        &[("path", LocalizationValue::Text(root))],
                    ),
                    "33",
                );
            }
            pins(details, localizer);
        }
    }
    if matches!(
        error.code,
        "init-eska-source-options-incompatible"
            | "init-eska-runner-options"
            | "init-eska-source-invalid"
            | "init-eska-name-invalid"
            | "init-eska-pin-invalid"
            | "init-eska-version-scheme"
            | "init-eska-workflow-required"
            | "workflow-invalid"
            | "workspace-workflow"
            | "platform-required"
            | "platform-version-invalid"
    ) {
        ExitCode::from(2)
    } else {
        ExitCode::FAILURE
    }
}

/// Parent failures use translated strings whose keys preserve their stable compound-operation codes.
fn failure(code: &'static str, localizer: &Localizer) -> Failure {
    Failure::new(code, localizer.text(code))
}
