//! Source-only project creation and native unpacking are distinct companion operations.

use super::{
    super::{NewArgs, artifact::Failure},
    failure,
    policy::Plan,
    process,
    protocol::{self, Created, Operation},
    stage::NativeStage,
};
use crate::{
    cli::{
        companion::{Capability, Kind, Tool},
        localization::Localizer,
    },
    config::ProjectConfig,
    project_core::{native, native_version},
    tool_cli::OutputFormat,
};
use serde_json::Value;
use std::{path::Path, process::Command};

pub(super) struct Expected {
    pub(super) kind: crate::project_core::ProjectType,
    pub(super) metadata_name: String,
    pub(super) uuid: Option<String>,
    pub(super) version: native_version::ProjectVersion,
}

/// Both executables are verified before creating even the invocation's private native staging.
pub(super) fn tools(
    plan: &Plan,
    args: &NewArgs,
    localizer: &Localizer,
) -> Result<(Tool, Option<Tool>), Failure> {
    let project = tool(
        Kind::Project,
        if args.from.is_some() {
            Capability::ImportQueries
        } else {
            Capability::New(
                plan.project_type
                    .ok_or_else(|| failure("new-child-response-invalid", localizer))?,
            )
        },
        localizer,
    )?;
    let build = args
        .from
        .as_ref()
        .map(|_| tool(Kind::Build, Capability::Unpack, localizer))
        .transpose()?;
    Ok((project, build))
}

/// A resolved platform remains explicit at the build-tool boundary; no platform runs for empty new.
pub(super) fn platform(
    plan: &mut Plan,
    args: &NewArgs,
    localizer: &Localizer,
) -> Result<(), Failure> {
    if args.from.is_some() || args.platform.select_platform {
        let inherited = plan
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.build_settings().platform_version());
        let selected = args.platform.resolve(
            inherited,
            matches!(args.format, OutputFormat::Human),
            localizer,
        )?;
        if args.from.is_some() || plan.platform.is_some() || args.platform.select_platform {
            plan.platform = Some(selected.version().clone());
        }
    }
    Ok(())
}

/// Validate the native report against actual XML identity and version, independent of artifact stems.
pub(super) fn unpack(
    tool: &Tool,
    args: &NewArgs,
    base: &Path,
    plan: &Plan,
    stage: &mut NativeStage,
    localizer: &Localizer,
) -> Result<Expected, Failure> {
    let input = args
        .from
        .as_ref()
        .ok_or_else(|| failure("new-child-response-invalid", localizer))?;
    let version = plan.platform.as_ref().ok_or_else(|| {
        Failure::new(
            "platform-required",
            localizer.text("artifact-platform-required"),
        )
    })?;
    let native_path = stage.native();
    let mut command = command(tool, localizer);
    command
        .arg("unpack")
        .arg(base.join(input))
        .arg(&native_path)
        .args(["--platform-version", version.as_str()]);
    runner_options(&mut command, args);
    let captured = process::capture(&mut command, 16 * 1024 * 1024)
        .map_err(|_| failure("new-child-transport-failed", localizer))?;
    if !captured.status.success() {
        return Err(child_failure(&captured, localizer));
    }
    let report: Value = serde_json::from_slice(&captured.stdout)
        .map_err(|_| failure("new-child-response-invalid", localizer))?;
    let data = &report["data"];
    let identity = native::inspect_identity(&native_path)
        .map_err(|_| failure("new-child-response-invalid", localizer))?
        .ok_or_else(|| failure("new-child-response-invalid", localizer))?;
    if captured.oversized
        || report["schema_version"] != 1
        || report["tool"] != "eska-build"
        || report["command"] != "unpack"
        || report["status"] != "success"
        || data["destination"].as_str().map(Path::new) != Some(native_path.as_path())
        || data["project_type"] != identity.project_type.as_str()
        || data["name"] != identity.name
        || data["uuid"] != identity.uuid
        || data["source_format"] != "designer-xml"
        || data["project_created"] != false
        || data["platform_version"] != version.as_str()
    {
        return Err(failure("new-child-response-invalid", localizer));
    }
    let project = ProjectConfig::new(identity.project_type)
        .with_source("native".into())
        .map_err(|_| failure("new-child-response-invalid", localizer))?
        .into_project(stage.root().to_owned())
        .map_err(|_| failure("new-child-response-invalid", localizer))?;
    let version = native_version::inspect(&project)
        .map_err(|_| failure("new-native-version-invalid", localizer))?
        .version;
    if let Some(scheme) = plan.scheme {
        version
            .validate_scheme(scheme)
            .map_err(|_| failure("new-version-scheme-mismatch", localizer))?;
    }
    stage
        .seal()
        .map_err(|_| failure("new-native-staging-failed", localizer))?;
    Ok(Expected {
        kind: identity.project_type,
        metadata_name: identity.name,
        uuid: Some(identity.uuid),
        version,
    })
}

/// One child owns final-root publication; the parent receives its original manifest/pin receipt.
pub(super) fn create(
    tool: &Tool,
    args: &NewArgs,
    plan: &Plan,
    stage: Option<&NativeStage>,
    localizer: &Localizer,
) -> Result<Created, Failure> {
    let mut command = command(tool, localizer);
    let operation = if let Some(stage) = stage {
        command
            .arg("import")
            .arg(stage.native())
            .arg(&plan.destination)
            .args(["--layout", "queries"]);
        Operation::Import
    } else {
        let kind = plan
            .project_type
            .ok_or_else(|| failure("new-child-response-invalid", localizer))?;
        let version = plan
            .version
            .as_ref()
            .ok_or_else(|| failure("new-child-response-invalid", localizer))?;
        command.arg("new").arg(&plan.destination).args([
            "--type",
            kind.as_str(),
            "--initial-version",
            &version.to_string(),
        ]);
        if let Some(metadata_name) = &args.metadata_name {
            command.args(["--metadata-name", metadata_name]);
        }
        Operation::New
    };
    command.args(["--name", plan.name.as_str()]);
    // Preserve member inheritance unless the user selected an override or the workspace lacks one.
    let inherited = plan
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.build_settings().platform_version());
    if let Some(platform) = &plan.platform
        && (plan.member.is_none()
            || args.platform.platform_version.is_some()
            || args.platform.select_platform
            || inherited.is_none())
    {
        command.args(["--platform-version", platform.as_str()]);
    }
    let captured = process::capture(&mut command, 128 * 1024)
        .map_err(|_| failure("new-child-transport-failed", localizer))?;
    if !captured.status.success() {
        return Err(child_failure(&captured, localizer));
    }
    protocol::created(
        &captured,
        operation,
        &plan.destination,
        &plan.name,
        Path::new("src"),
    )
    .map_err(|_| failure("new-child-response-invalid", localizer))
}

/// Exact protocol/version mismatches refuse the discovered tool rather than trying another binary.
fn tool(kind: Kind, capability: Capability, localizer: &Localizer) -> Result<Tool, Failure> {
    let tool = Tool::discover(kind).map_err(|error| companion_error(error, kind, localizer))?;
    tool.verify(localizer.locale().as_str(), capability)
        .map_err(|error| companion_error(error, kind, localizer))?;
    Ok(tool)
}

/// Localization belongs to the shell; stable failure codes retain the transport reason.
fn companion_error(
    error: crate::cli::companion::Error,
    kind: Kind,
    localizer: &Localizer,
) -> Failure {
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
            &[(
                "tool",
                crate::cli::localization::LocalizationValue::Text(kind.name()),
            )],
        ),
    )
}

/// Capture a machine document while preserving the exact selected locale for child diagnostics.
fn command(tool: &Tool, localizer: &Localizer) -> Command {
    let mut command = tool.command();
    command.args(["--lang", localizer.locale().as_str(), "--format", "json"]);
    command
}

/// Runner overrides retain OS-native paths and are interpreted relative to the same caller cwd.
fn runner_options(command: &mut Command, args: &NewArgs) {
    if let Some(path) = &args.platform.ibcmd {
        command.arg("--ibcmd").arg(path);
    }
    if let Some(arch) = &args.platform.platform_arch {
        command.args(["--platform-arch", arch]);
    }
    if let Some(container) = &args.platform.distrobox {
        command.args(["--distrobox", container]);
    }
}

/// A failed acknowledgment cannot prove ownership of a destination that may already be present.
fn child_failure(captured: &process::Captured, localizer: &Localizer) -> Failure {
    let report: Option<Value> = (!captured.oversized)
        .then(|| serde_json::from_slice(&captured.stdout).ok())
        .flatten();
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        captured.status.signal()
    };
    #[cfg(not(unix))]
    let signal: Option<i32> = None;
    failure("new-child-failed", localizer).with_details(serde_json::json!({"child_exit_code":captured.status.code(),"child_signal":signal,"child_error":report.map(|report| report["error"].clone())}))
}
