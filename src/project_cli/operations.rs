//! Process-boundary selection and reports, with one source implementation in `project_core`.

use super::args::{Cli, Command, Import, Prepare, Selector, Transport};
use crate::{
    project_core::{
        Project, ProjectName, SourceFormat,
        creation::{self, DEFAULT_GENERATIONS_DIRECTORY},
        discovery::discover_context,
        eska_source::EskaSource,
        metadata_model::ProjectScope,
        platform_settings::PlatformVersion,
        selection::{SelectionIntent, select_projects},
        source_format::{NativeUuid, PROFILE, RULES_VERSION, SCHEMA_VERSION},
        transport::TransportPlan,
    },
    tool_cli::{
        self, Failure,
        localization::{LocalizationValue, Localizer},
    },
};
use serde_json::{Value, json};
use std::path::Path;

/// Dispatch only source operations; native CF unpacking belongs to the build tool.
pub(super) fn execute(cli: &Cli, localizer: &Localizer) -> Result<(Value, String), Failure> {
    match &cli.command {
        Command::Import(args) => import(args, localizer),
        Command::Inspect(args) => inspect(&cli.project_dir, args, localizer),
        Command::Prepare(args) => prepare(&cli.project_dir, args, localizer),
        Command::Transport(args) => transport(&cli.project_dir, args, localizer),
        Command::Capabilities => Ok((
            json!({"version":env!("CARGO_PKG_VERSION"),
            "protocol_version":1, "source_schema_version":SCHEMA_VERSION,
            "rules_version":RULES_VERSION, "profile":PROFILE,
            "commands":["import","inspect","prepare","transport","capabilities"],
            "platform_required":false,"git_required":false,"ide_required":false,
            "creation_project_types":["configuration"], "metadata_formats":["eska"],
            "structural_edits":false}),
            localizer.text("tool-project-capabilities"),
        )),
    }
}

/// Import into an exclusively new directory, returning a pin for independent approval/storage.
fn import(args: &Import, localizer: &Localizer) -> Result<(Value, String), Failure> {
    path_text(&args.input)?;
    path_text(&args.destination)?;
    let parent = args
        .destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|_| Failure::new("source_file_missing"))?;
    path_text(&parent)?;
    let name = ProjectName::parse(args.name.clone())
        .map_err(|_| Failure::new("project_name_invalid").at("name"))?;
    let platform = args
        .platform_version
        .as_deref()
        .map(PlatformVersion::parse)
        .transpose()
        .map_err(|_| Failure::new("platform_version_invalid").at("platform_version"))?;
    let frozen = args
        .freeze
        .iter()
        .map(|uuid| NativeUuid::parse(uuid))
        .collect::<Result<Vec<_>, _>>()?;
    let imported = creation::import_project(
        &args.input,
        &args.destination,
        name,
        platform.as_ref(),
        &frozen,
    )?;
    let root = path_text(imported.project.root())?;
    let human = localizer.format(
        "tool-project-imported",
        &[
            ("path", LocalizationValue::Text(root)),
            ("pin", LocalizationValue::Text(&imported.accepted.sha256)),
        ],
    );
    Ok((
        json!({"root":root,"name":args.name,"source_format":"eska",
        "source":path_text(imported.project.source())?,"accepted_base":".eska/accepted",
        "accepted_base_sha256":imported.accepted.sha256}),
        human,
    ))
}

/// Inspect manifest/configuration facts without loading an accepted base or all owner payloads.
fn inspect(
    start: &Path,
    selector: &Selector,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    let (project, scope) = selected(start, selector)?;
    let format = project.configuration().source_format().as_str();
    let metadata = if matches!(
        project.configuration().source_format(),
        SourceFormat::Eska { .. }
    ) {
        let source = EskaSource::open(project.clone(), scope)?;
        let configuration = source.reader().configuration();
        let declaration = configuration
            .declarations
            .first()
            .ok_or_else(|| Failure::new("source_configuration_missing"))?;
        json!({"name":declaration.name, "uuid":declaration.uuid,
            "owners":source.reader().owner_paths().count(), "source_sha256":configuration.source_sha256,
            "validation":"manifest_and_configuration_only"})
    } else {
        Value::Null
    };
    let root = path_text(project.root())?;
    let human = localizer.format(
        "tool-project-inspected",
        &[
            ("path", LocalizationValue::Text(root)),
            ("format", LocalizationValue::Text(format)),
        ],
    );
    Ok((
        json!({"root":root,"source":path_text(project.source())?,"source_format":format,
        "type":project.configuration().project_type().as_str(),"metadata":metadata,
        "platform_version":project.configuration().build_settings().platform_version().map(PlatformVersion::as_str)}),
        human,
    ))
}

/// Always prepare from current saved inputs; CLI JSON is a report rather than a build proof.
fn prepare(
    start: &Path,
    args: &Prepare,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    let pin = tool_cli::accepted_ref(args.accepted_base_sha256.as_deref())?;
    let (project, scope) = selected(start, &args.selector)?;
    let generations = args.generations.as_ref().map_or_else(
        || project.root().join(DEFAULT_GENERATIONS_DIRECTORY),
        |path| project.root().join(path),
    );
    path_text(&generations)?;
    let source = EskaSource::open(project, scope)?;
    let output = source
        .prepare(&pin, &generations, !args.allow_exclusions)
        .map_err(|failure| Failure {
            diagnostics: json!(failure.diagnostics),
            ..Failure::from(failure.error)
        })?;
    let xml = path_text(output.xml())?;
    let human = localizer.format(
        "tool-project-prepared",
        &[
            ("path", LocalizationValue::Text(xml)),
            (
                "count",
                LocalizationValue::Number(
                    i64::try_from(output.diagnostics().len()).unwrap_or(i64::MAX),
                ),
            ),
        ],
    );
    Ok((
        json!({"xml":xml,"baseline":output.baseline(),"generation":output.generation(),
        "input_revision":output.input_revision(),"native_revision":output.native_revision(),
        "source_map_sha256":output.source_map().checksum()?,"exclusions":output.diagnostics()}),
        human,
    ))
}

/// Preview by default; the explicit write flag uses exact-byte publication.
fn transport(
    start: &Path,
    args: &Transport,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    let (project, _) = selected(start, &args.selector)?;
    let plan = TransportPlan::prepare(&project)?;
    let changed = !plan.is_empty();
    if args.write {
        plan.publish()?;
    }
    let path = project.root().join(".gitattributes");
    let key = if args.write {
        "tool-project-transport-written"
    } else {
        "tool-project-transport-preview"
    };
    let message = localizer.format(key, &[("path", LocalizationValue::Text(path_text(&path)?))]);
    let human = if args.write {
        message
    } else {
        format!("{message}\n\n{}", plan.updated())
    };
    Ok((
        json!({"path":path_text(&path)?,"changed":changed,"written":args.write && changed,
        "content":plan.updated(),"effective_git_policy_verified":false}),
        human,
    ))
}

/// Resolve exactly one member with the shared discovery/inheritance and selector policy.
fn selected(start: &Path, selector: &Selector) -> Result<(Project, ProjectScope), Failure> {
    let context = discover_context(start)
        .map_err(|_| Failure::new("project_discovery_failed").at(start.to_string_lossy()))?;
    let names: Vec<_> = selector.project.iter().cloned().collect();
    let selection = select_projects(&context, &names, false, SelectionIntent::SingleMutation)
        .map_err(|_| Failure::new("project_selection_failed"))?;
    let project = selection
        .projects()
        .first()
        .ok_or_else(|| Failure::new("project_selection_failed"))?;
    path_text(project.project().root())?;
    let scope = project.name().map_or(ProjectScope::Standalone, |name| {
        ProjectScope::Member(name.clone())
    });
    Ok((project.project().clone(), scope))
}

/// Machine paths must be lossless UTF-8; reject unsupported encodings before mutation.
fn path_text(path: &Path) -> Result<&str, Failure> {
    path.to_str()
        .ok_or_else(|| Failure::new("path_encoding_unsupported"))
}
