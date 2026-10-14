//! Process-boundary selection and reports, with one source implementation in `project_core`.

mod creation;
mod init;
mod migration;
mod new;
mod source_import;
mod update;

use super::args::{Cli, Command, Prepare, Selector, Transport};
use crate::{
    project_core::{
        SourceFormat,
        creation::DEFAULT_GENERATIONS_DIRECTORY,
        eska_source::EskaSource,
        platform_settings::PlatformVersion,
        source_format::{
            DESCRIPTOR_LAYOUT_VERSION, EXTERNAL_LAYOUT_VERSION, MANAGED_FORM_LAYOUT_VERSION,
            PROFILE, QUERY_LAYOUT_VERSION, READABLE_SCHEMA_VERSION, RESOURCE_LAYOUT_VERSION,
            RULES_VERSION, SCHEMA_VERSION,
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
        Command::New(args) => new::create(&cli.project_dir, args, localizer),
        Command::Init(args) => init::initialize(&cli.project_dir, args, localizer),
        Command::Import(args) => creation::import(args, localizer),
        Command::Migrate(args) => migration::migrate(&cli.project_dir, args, localizer),
        Command::UpdateBase(args) => update::update(&cli.project_dir, args, localizer),
        Command::Inspect(args) => inspect(&cli.project_dir, args, localizer),
        Command::Prepare(args) => prepare(&cli.project_dir, args, localizer),
        Command::Transport(args) => transport(&cli.project_dir, args, localizer),
        Command::CaptureAncestor(args) => source_import::capture(args, localizer),
        Command::RecoverImport(args) => source_import::recover(&cli.project_dir, args, localizer),
        Command::Capabilities => Ok((
            json!({"version":env!("CARGO_PKG_VERSION"),
            "protocol_version":1, "source_schema_version":SCHEMA_VERSION,
            "rules_version":RULES_VERSION, "profile":PROFILE,
            "commands":["new","init","import","migrate","update-base","inspect","prepare","transport","capture-ancestor","recover-import","capabilities"],
            "platform_required":false,"git_required":false,"ide_required":false,
            "creation_project_types":["configuration","extension","processing","report"], "metadata_formats":["eska"],
            "new_project_types":["configuration","extension","processing","report"],
            "new_source_layout":"queries", "new_initial_version":"0.1.0",
            "initialization_project_types":["configuration","extension","processing","report"],
            "initialization_source_layout":"queries", "initialization_converts_source":false,
            "import_layouts":["family","readable","queries"],
            "initial_ancestor_layouts":["queries"],
            "import_layout_project_types":{"family":["configuration","extension"],
            "readable":["configuration","extension"],
            "queries":["configuration","extension","processing","report"]},
            "source_layout_versions":[SCHEMA_VERSION,DESCRIPTOR_LAYOUT_VERSION,RESOURCE_LAYOUT_VERSION,
            MANAGED_FORM_LAYOUT_VERSION,QUERY_LAYOUT_VERSION,EXTERNAL_LAYOUT_VERSION],
            "descriptor_schema_versions":[SCHEMA_VERSION,READABLE_SCHEMA_VERSION],
            "migration_layouts":["readable"], "migration_scopes":["standalone","workspace"], "workspace_pin_schema_version":1,
            "base_update_scopes":["standalone","workspace"], "base_update_comparison":"whole_file_three_way",
            "structural_edits":false,"import_ancestor_capture":true,"source_import_recovery":["complete","roll-back"]}),
            localizer.text("tool-project-capabilities"),
        )),
    }
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
