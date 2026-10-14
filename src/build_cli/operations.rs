//! Only the build core starts platform processes; the CLI supplies selected inputs and reports.

mod source_import;
mod unpack;

use std::io::{self, Write};

use serde::Serialize;
use serde_json::{Value, json};

use super::{
    args::{Build, Cli, Command},
    errors,
};
use crate::{
    build_core::{
        self, BuildPlan, BuildProvenance, GitProvenance, Ibcmd, PlatformVersion, ProcessStream,
        machine,
    },
    project_core::{
        ProjectName, SourceFormat,
        creation::DEFAULT_GENERATIONS_DIRECTORY,
        eska_source::EskaSource,
        source_format::{MappedFile, PreparedSource},
    },
    tool_cli::{
        self, Failure, OutputFormat, Selected,
        localization::{LocalizationValue, Localizer},
        path_text, selected,
    },
};

/// Exact line bytes and optional origins remain available on both success and platform failure.
#[derive(Serialize)]
struct PlatformEvent {
    stage: &'static str,
    stream: &'static str,
    bytes: Vec<u8>,
    origin: Option<MappedFile>,
}

/// Dispatch native operations without loading project or analysis state for unpack.
pub(super) fn execute(cli: &Cli, localizer: &Localizer) -> Result<(Value, String), Failure> {
    match &cli.command {
        Command::Build(args) => build(cli, args, localizer),
        Command::Unpack(args) => unpack::execute(cli, args, localizer),
        Command::Import(args) => source_import::execute(cli, args, localizer),
        Command::Capabilities => Ok((
            json!({"version":env!("CARGO_PKG_VERSION"),"protocol_version":1,
            "commands":["build","unpack","import","capabilities"],
            "source_formats":["designer-xml","eska"],
            "designer_artifact_types":["configuration","extension","processing","report"],
            "eska_artifact_types":["configuration","extension","processing","report"], "manifest":true,
            "platform_required_for_build":true,"git_required":false,"analyzer_required":false,
            "unpack":true,"designer_check":false,
            "source_import_formats":["eska"],"source_import_mode":"preview_then_explicit_write",
            "source_import_ancestor":"independently_confirmed_sha256",
            "source_import_recovery":"eska-project recover-import"}),
            localizer.text("tool-build-capabilities"),
        )),
    }
}

/// Prepare saved inputs before discovering ibcmd, then build and publish the verified artifact pair.
fn build(cli: &Cli, args: &Build, localizer: &Localizer) -> Result<(Value, String), Failure> {
    let selection = selected(&cli.project_dir, args.project.as_deref())?;
    let project = &selection.project;
    let plan = build_plan(&selection, args)?;
    let prepared = prepare_source(&selection, args)?;
    let native_plan = prepared.as_ref().map_or_else(
        || plan.clone(),
        |source| plan.with_snapshot_source(source.xml().to_owned()),
    );
    build_core::preflight_manifest(&native_plan).map_err(errors::build)?;
    let options = machine::tool_options(
        args.ibcmd.clone(),
        args.platform_arch.clone(),
        args.distrobox.clone(),
    )
    .map_err(errors::machine)?;
    let ibcmd = Ibcmd::discover(plan.platform_version(), &options).map_err(errors::tool)?;
    let name = selection
        .member
        .as_ref()
        .map(ProjectName::as_str)
        .or_else(|| project.root().file_name().and_then(|name| name.to_str()));
    let provenance = BuildProvenance::new(name, GitProvenance::Unavailable);
    let mut events = Vec::new();
    let mut observe = |stage, stream, line: &[u8], origin: Option<&MappedFile>| {
        if matches!(cli.format, OutputFormat::Human) {
            let _ = io::stderr().lock().write_all(line);
        }
        events.push(PlatformEvent {
            stage: errors::stage_name(stage),
            stream: match stream {
                ProcessStream::Stdout => "stdout",
                ProcessStream::Stderr => "stderr",
            },
            bytes: line.to_vec(),
            origin: origin.cloned(),
        });
    };
    let result = if let Some(prepared) = &prepared {
        build_core::execute_prepared_with_locations(
            &plan, project, prepared, provenance, &ibcmd, observe,
        )
    } else {
        build_core::execute_streaming_with_manifest(
            &plan,
            project,
            provenance,
            &ibcmd,
            |stage, stream, line| observe(stage, stream, line, None),
        )
    }
    .map_err(|error| {
        let failure = errors::build(error);
        Failure {
            diagnostics: json!({"detail":failure.diagnostics,"platform":events}),
            ..failure
        }
    })?;
    let manifest = result
        .manifest()
        .ok_or_else(|| Failure::new("build_manifest_missing"))?;
    let path = path_text(result.output())?;
    let human = localizer.format(
        "tool-build-completed",
        &[
            ("path", LocalizationValue::Text(path)),
            ("manifest", LocalizationValue::Text(path_text(manifest)?)),
        ],
    );
    Ok((
        json!({"artifact":path, "manifest":path_text(manifest)?,
        "artifact_type":plan.artifact_type().as_str(),
        "platform_version":ibcmd.version().as_str(), "runner":ibcmd.runner_kind(),
        "source_format":project.configuration().source_format().as_str(),
        "git_provenance":"unavailable", "platform":events,
        "exclusions":prepared.as_ref().map(PreparedSource::diagnostics).unwrap_or_default()}),
        human,
    ))
}

/// Resolve output scope and overrides without creating an infobase or consulting the platform.
fn build_plan(selection: &Selected, args: &Build) -> Result<BuildPlan, Failure> {
    let project = &selection.project;
    let version = args
        .platform_version
        .as_deref()
        .map(PlatformVersion::parse)
        .transpose()
        .map_err(|_| Failure::new("platform_version_invalid").at("platform_version"))?;
    let plan = match (&selection.workspace_root, &selection.member) {
        (Some(root), Some(member)) => BuildPlan::for_workspace_member(
            project,
            root,
            member.as_str(),
            args.output.as_deref(),
            version,
        ),
        _ => BuildPlan::with_platform_version(project, args.output.as_deref(), version),
    }
    .map_err(errors::plan)?;
    let base = args
        .base_configuration
        .as_ref()
        .map(|path| project.root().join(path));
    let plan = plan
        .with_base_configuration(base)
        .map_err(errors::plan)?
        .with_recreate_infobase(args.recreate_infobase);
    path_text(plan.output())?;
    path_text(plan.infobase_root())?;
    if let Some(base) = plan.base_configuration() {
        path_text(base)?;
    }
    Ok(plan)
}

/// Only a freshly prepared private proof authorizes ESKA; a JSON report or stale XML cannot substitute.
fn prepare_source(selection: &Selected, args: &Build) -> Result<Option<PreparedSource>, Failure> {
    let project = &selection.project;
    if matches!(
        project.configuration().source_format(),
        SourceFormat::DesignerXml
    ) {
        if args.accepted_base_sha256.is_some()
            || args.generations.is_some()
            || args.allow_exclusions
        {
            return Err(Failure::new("build_source_options_incompatible"));
        }
        return Ok(None);
    }
    let pin = tool_cli::accepted_ref(args.accepted_base_sha256.as_deref())?;
    let generations = args.generations.as_ref().map_or_else(
        || project.root().join(DEFAULT_GENERATIONS_DIRECTORY),
        |path| project.root().join(path),
    );
    path_text(&generations)?;
    let source = EskaSource::open(project.clone(), selection.scope.clone())?;
    source
        .prepare(&pin, &generations, !args.allow_exclusions)
        .map(Some)
        .map_err(|failure| Failure {
            diagnostics: json!(failure.diagnostics),
            ..Failure::from(failure.error)
        })
}
