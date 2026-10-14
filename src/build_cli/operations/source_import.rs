//! Native staging delegates comparison and guarded publication to the shared project core.

mod receipt;
mod report;

use std::io::{self, Write};

use serde_json::{Value, json};

use crate::{
    build_cli::{
        args::{Cli, Import},
        errors,
    },
    build_core::{
        Ibcmd, PlatformVersion, ProcessStream,
        artifact::{ArtifactSourceImportError, PreparedArtifact},
        machine,
    },
    project_core::{
        SourceFormat,
        eska_source::EskaSource,
        source_format::{
            AcceptedBase, ImportAncestor, ImportAncestorRef, ProjectSourceImportRef,
            project_import_ancestor,
        },
    },
    tool_cli::{self, Failure, OutputFormat, localization::Localizer, path_text, selected},
};

/// Preview never mutates the selected source; write retains recovery authority before any source move.
pub(super) fn execute(
    cli: &Cli,
    args: &Import,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    path_text(&args.file)?;
    let selection = selected(&cli.project_dir, args.project.as_deref())?;
    let project = &selection.project;
    let approved = tool_cli::accepted_ref(args.accepted_base_sha256.as_deref())?;
    let reference = ImportAncestorRef::from_sha256(&args.ancestor_sha256)?;
    let expected = args
        .expected_plan_sha256
        .as_deref()
        .map(ProjectSourceImportRef::from_sha256)
        .transpose()?;
    let source = EskaSource::open(project.clone(), selection.scope.clone())?;
    let SourceFormat::Eska { accepted_base } = project.configuration().source_format() else {
        return Err(Failure::new("source_format_mismatch"));
    };
    AcceptedBase::open(&project.root().join(accepted_base.as_path()), &approved)?;
    let ancestor = if let Some(path) = &args.ancestor {
        path_text(path)?;
        ImportAncestor::open(path, &reference)?
    } else {
        project_import_ancestor(project, &selection.scope, &reference)?
    };
    let destination = args
        .receipt
        .as_deref()
        .map(|path| receipt::destination(project, args.ancestor.as_deref(), path))
        .transpose()?;
    let version = args
        .platform_version
        .as_deref()
        .map(PlatformVersion::parse)
        .transpose()
        .map_err(|_| Failure::new("platform_version_invalid").at("platform_version"))?
        .or_else(|| {
            project
                .configuration()
                .build_settings()
                .platform_version()
                .cloned()
        })
        .ok_or_else(|| Failure::new("platform_version_missing"))?;
    let options = machine::tool_options(
        args.ibcmd.clone(),
        args.platform_arch.clone(),
        args.distrobox.clone(),
    )
    .map_err(errors::machine)?;
    let tool = Ibcmd::discover(&version, &options).map_err(errors::tool)?;
    let mut events = Vec::new();
    let artifact = PreparedArtifact::unpack(&args.file, source.project().source(), &tool, |stream, bytes| {
        if matches!(cli.format, OutputFormat::Human) {
            let _ = io::stderr().lock().write_all(bytes);
        }
        events.push(json!({"stream":match stream { ProcessStream::Stdout=>"stdout", ProcessStream::Stderr=>"stderr" },"bytes":bytes}));
    }).map_err(|error| with_events(errors::unpack(error), &events))?;
    let prepared = artifact
        .prepare_source_import(project, &selection.scope, &approved, &ancestor, &|| {
            tool.was_interrupted()
        })
        .map_err(|error| with_events(import_error(error), &events))?;
    let plan = prepared.plan();
    let mut data = report::preview(project, &selection.scope, plan, &events)?;
    let human = report::human(plan, args.write, localizer);
    if expected
        .as_ref()
        .is_some_and(|expected| expected != plan.reference())
    {
        return Err(Failure {
            diagnostics: data,
            ..Failure::new("source_input_changed")
        });
    }
    if args.write {
        if !plan.source_plan().conflicts().is_empty() {
            return Err(Failure {
                diagnostics: data,
                ..Failure::new("source_import_conflicts")
            });
        }
        let destination =
            destination.ok_or_else(|| Failure::new("source_import_receipt_required"))?;
        if !plan.is_unchanged() {
            receipt::retain(&destination, &data)?;
        }
        let publication = prepared.publish(&|| tool.was_interrupted()).map_err(|error| {
            let failure = import_error(error);
            Failure { diagnostics: json!({"preview":data,"receipt":path_text(&destination).ok(),"detail":failure.diagnostics}), ..failure }
        })?;
        data["publication"] = json!(publication);
        data["receipt"] = if plan.is_unchanged() {
            Value::Null
        } else {
            json!(path_text(&destination)?)
        };
        data["applied"] = json!(true);
    }
    Ok((data, human))
}

/// Staging failures retain native categories while project failures keep their original domain code.
fn import_error(error: ArtifactSourceImportError) -> Failure {
    match error {
        ArtifactSourceImportError::Artifact(error) => errors::unpack(error),
        ArtifactSourceImportError::Project(error) => error.into(),
    }
}

/// The JSON failure captures raw platform events instead of mixing streams into stdout.
fn with_events(failure: Failure, events: &[Value]) -> Failure {
    Failure {
        diagnostics: json!({"detail":failure.diagnostics,"platform":events}),
        ..failure
    }
}
