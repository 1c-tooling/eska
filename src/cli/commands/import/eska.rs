//! ESKA import delegates native projection, exact plan, source publication and recovery to one child.

mod arguments;

use super::{
    super::artifact::{Failure, PlatformArgs},
    Format, ImportArgs,
};
use crate::{
    cli::{
        companion::{self, Capability, Kind, Tool, inherited},
        localization::Localizer,
    },
    project::{build::PlatformVersion, selection::SelectedProject},
    project_core::source_format::{ImportAncestorRef, ProjectSourceImportRef},
    tool_cli,
};
use serde_json::Value;
use std::path::Path;

/// The selected type and caller authority precede companion/platform admission without source writes.
pub(super) fn execute(
    args: &ImportArgs,
    selected: SelectedProject<'_>,
    base: &Path,
    localizer: &Localizer,
) -> Result<Value, Failure> {
    validate(args, localizer)?;
    let tool = Tool::discover(Kind::Build).map_err(|error| companion_error(error, localizer))?;
    tool.verify(
        localizer.locale().as_str(),
        Capability::SourceImport(selected.project().configuration().project_type()),
    )
    .map_err(|error| companion_error(error, localizer))?;
    let version = platform_override(&args.platform, args.format, localizer)?;
    let arguments = arguments::arguments(args, selected, base, version.as_ref(), localizer)
        .map_err(|_| failure("import-path-invalid", localizer))?;
    let mut command = tool.command();
    command.args(arguments);
    // Neither this shell nor its discovery/selection objects own publication/staging resources.
    // The child alone initializes interrupt handling and retains journal/receipt recovery state.
    inherited::execute(&mut command)
        .map_err(|_| failure("import-tool-execution-failed", localizer))
        .map(|never| match never {})
}

/// Syntax checks select no trust from local state; actual source/base/history checks remain child-owned.
fn validate(args: &ImportArgs, localizer: &Localizer) -> Result<(), Failure> {
    if args.force {
        return Err(failure("import-eska-force-incompatible", localizer));
    }
    tool_cli::accepted_ref(args.source.accepted_base_sha256.as_deref()).map_err(|error| {
        failure(
            if error.code == "accepted_base_pin_required" {
                "import-accepted-required"
            } else {
                "import-accepted-invalid"
            },
            localizer,
        )
    })?;
    let ancestor = args
        .source
        .ancestor_sha256
        .as_deref()
        .ok_or_else(|| failure("import-ancestor-required", localizer))?;
    ImportAncestorRef::from_sha256(ancestor)
        .map_err(|_| failure("import-ancestor-invalid", localizer))?;
    if let Some(plan) = &args.source.expected_plan_sha256 {
        ProjectSourceImportRef::from_sha256(plan)
            .map_err(|_| failure("import-expected-plan-invalid", localizer))?;
    }
    Ok(())
}

/// Ordinary installed-platform discovery stays in the child; only an explicit selector needs parent UI.
fn platform_override(
    args: &PlatformArgs,
    format: Format,
    localizer: &Localizer,
) -> Result<Option<PlatformVersion>, Failure> {
    if let Some(version) = &args.platform_version {
        return PlatformVersion::parse(version).map(Some).map_err(|_| {
            Failure::new(
                "platform-version-invalid",
                localizer.text("artifact-version-invalid"),
            )
        });
    }
    if args.select_platform {
        return args
            .resolve(None, format == Format::Human, localizer)
            .map(|tool| Some(tool.version().clone()));
    }
    Ok(None)
}

/// The shell localizes only transport faults; the child returns unchanged domain diagnostics.
fn companion_error(error: companion::Error, localizer: &Localizer) -> Failure {
    let code = match error {
        companion::Error::Missing => "import-tool-missing",
        companion::Error::Unavailable => "import-tool-unavailable",
        companion::Error::ProbeFailed => "import-tool-probe-failed",
        companion::Error::ProbeTimeout => "import-tool-probe-timeout",
        companion::Error::ProbeOversized => "import-tool-probe-oversized",
        companion::Error::Incompatible => "import-tool-incompatible",
    };
    failure(code, localizer)
}

/// Stable shell error codes use the same bilingual presentation key.
fn failure(code: &'static str, localizer: &Localizer) -> Failure {
    Failure::new(code, localizer.text(code))
}
