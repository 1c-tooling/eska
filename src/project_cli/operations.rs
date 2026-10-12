//! Process-boundary selection and reports, with one source implementation in `project_core`.

use super::args::{Cli, Command, Import, ImportLayout, Migrate, Prepare, Selector, Transport};
use crate::{
    project_core::{
        ProjectName, SourceFormat,
        creation::{self, DEFAULT_GENERATIONS_DIRECTORY},
        eska_source::EskaSource,
        platform_settings::PlatformVersion,
        source_format::{
            DESCRIPTOR_LAYOUT_VERSION, NativeUuid, PROFILE, READABLE_SCHEMA_VERSION,
            RESOURCE_LAYOUT_VERSION, RULES_VERSION, SCHEMA_VERSION, prepare_readable_project,
        },
        transport::TransportPlan,
    },
    tool_cli::{
        self, Failure,
        localization::{LocalizationValue, Localizer},
        path_text, selected,
    },
};
use serde_json::{Value, json};
use std::path::Path;

/// Dispatch only source operations; native CF unpacking belongs to the build tool.
pub(super) fn execute(cli: &Cli, localizer: &Localizer) -> Result<(Value, String), Failure> {
    match &cli.command {
        Command::Import(args) => import(args, localizer),
        Command::Migrate(args) => migrate(&cli.project_dir, args, localizer),
        Command::Inspect(args) => inspect(&cli.project_dir, args, localizer),
        Command::Prepare(args) => prepare(&cli.project_dir, args, localizer),
        Command::Transport(args) => transport(&cli.project_dir, args, localizer),
        Command::Capabilities => Ok((
            json!({"version":env!("CARGO_PKG_VERSION"),
            "protocol_version":1, "source_schema_version":SCHEMA_VERSION,
            "rules_version":RULES_VERSION, "profile":PROFILE,
            "commands":["import","migrate","inspect","prepare","transport","capabilities"],
            "platform_required":false,"git_required":false,"ide_required":false,
            "creation_project_types":["configuration"], "metadata_formats":["eska"],
            "import_layouts":["family","readable"],
            "source_layout_versions":[SCHEMA_VERSION,DESCRIPTOR_LAYOUT_VERSION,RESOURCE_LAYOUT_VERSION],
            "descriptor_schema_versions":[SCHEMA_VERSION,READABLE_SCHEMA_VERSION],
            "migration_layouts":["readable"], "migration_scopes":["standalone"],
            "structural_edits":false}),
            localizer.text("tool-project-capabilities"),
        )),
    }
}

/// Select one project; independent core plans own all conversion, validation and publication.
fn migrate(
    start: &Path,
    args: &Migrate,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    path_text(&args.destination)?;
    let pin = tool_cli::accepted_ref(args.accepted_base_sha256.as_deref())?;
    let selection = selected(start, args.selector.project.as_deref())?;
    let source = EskaSource::open(selection.project, selection.scope)?;
    let plan = prepare_readable_project(source.project(), source.scope(), &pin, &args.destination)?;
    let destination = path_text(plan.destination())?;
    let publication = args.write.then(|| plan.publish()).transpose()?;
    let key = if args.write {
        "tool-project-migrated"
    } else {
        "tool-project-migration-preview"
    };
    let human = localizer.format(
        key,
        &[
            ("path", LocalizationValue::Text(destination)),
            (
                "pin",
                LocalizationValue::Text(&plan.accepted_reference().sha256),
            ),
            (
                "source_count",
                LocalizationValue::Number(
                    i64::try_from(plan.source_changes().len()).unwrap_or(i64::MAX),
                ),
            ),
            (
                "base_count",
                LocalizationValue::Number(
                    i64::try_from(plan.accepted_changes().len()).unwrap_or(i64::MAX),
                ),
            ),
        ],
    );
    Ok((
        json!({"root":path_text(source.project().root())?, "destination":destination,
        "source_layout":"readable", "source_layout_version":RESOURCE_LAYOUT_VERSION,
        "descriptor_schema_version":READABLE_SCHEMA_VERSION,
        "previous_baseline":plan.previous_reference().sha256,
        "baseline":plan.accepted_reference().sha256, "layout_changed":plan.layout_changed(),
        "written":args.write, "publication":publication,
        "source_changes":plan.source_changes(), "accepted_changes":plan.accepted_changes(),
        "copied_project_files":plan.ancillary_files().collect::<Vec<_>>(),
        "ci_pin_updated":false, "effective_git_policy_verified":false}),
        human,
    ))
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
    let import = match args.layout {
        ImportLayout::Family => creation::import_project,
        ImportLayout::Readable => creation::import_readable_project,
    };
    let imported = import(
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
        "source_layout":args.layout.as_str(),
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
    let selection = selected(start, selector.project.as_deref())?;
    let project = selection.project;
    let scope = selection.scope;
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
    let selection = selected(start, args.selector.project.as_deref())?;
    let project = selection.project;
    let scope = selection.scope;
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
    let project = selected(start, args.selector.project.as_deref())?.project;
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
