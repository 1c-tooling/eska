//! Migration CLI keeps standalone reports stable and explicitly selects complete workspaces.

use super::super::args::{Migrate, MigrationLayout};
use crate::{
    project_core::{
        discovery::{DiscoveryContext, discover_context},
        eska_source::EskaSource,
        source_format::{
            READABLE_SCHEMA_VERSION, RESOURCE_LAYOUT_VERSION, WorkspaceAcceptedRefs,
            WorkspaceLayoutMember, prepare_query_project, prepare_query_workspace,
            prepare_readable_project, prepare_readable_workspace,
        },
    },
    tool_cli::{
        self, Failure,
        localization::{LocalizationValue, Localizer},
        path_text, selected,
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};

/// Workspace migration requires an explicit complete pin document, never the standalone fallback.
pub(super) fn migrate(
    start: &Path,
    args: &Migrate,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    if args.workspace {
        migrate_workspace(start, args, localizer)
    } else {
        migrate_project(start, args, localizer)
    }
}

/// Select one project; independent core plans own all conversion, validation and publication.
fn migrate_project(
    start: &Path,
    args: &Migrate,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    path_text(&args.destination)?;
    let pin = tool_cli::accepted_ref(args.accepted_base_sha256.as_deref())?;
    let selection = selected(start, args.selector.project.as_deref())?;
    let source = EskaSource::open(selection.project, selection.scope)?;
    let prepare = match args.layout {
        MigrationLayout::Readable => prepare_readable_project,
        MigrationLayout::Queries => prepare_query_project,
    };
    let plan = prepare(source.project(), source.scope(), &pin, &args.destination)?;
    let destination = path_text(plan.destination())?;
    let publication = args.write.then(|| plan.publish()).transpose()?;
    let key = match (args.layout, args.write) {
        (MigrationLayout::Readable, true) => "tool-project-migrated",
        (MigrationLayout::Readable, false) => "tool-project-migration-preview",
        (MigrationLayout::Queries, true) => "tool-project-query-migrated",
        (MigrationLayout::Queries, false) => "tool-project-query-migration-preview",
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
    let mut data = json!({"root":path_text(source.project().root())?, "destination":destination,
        "source_layout":"readable", "source_layout_version":RESOURCE_LAYOUT_VERSION,
        "descriptor_schema_version":READABLE_SCHEMA_VERSION,
        "previous_baseline":plan.previous_reference().sha256,
        "baseline":plan.accepted_reference().sha256, "layout_changed":plan.layout_changed(),
        "written":args.write, "publication":publication,
        "source_changes":plan.source_changes(), "accepted_changes":plan.accepted_changes(),
        "copied_project_files":plan.ancillary_files().collect::<Vec<_>>(),
        "ci_pin_updated":false, "effective_git_policy_verified":false});
    if matches!(args.layout, MigrationLayout::Queries) {
        data["source_layout"] = json!("queries");
        data["source_layout_version"] = json!(plan.source_layout_version());
        data["ancestry_preserved"] = json!(plan.ancestry_preserved());
    }
    Ok((data, human))
}

/// Prepare all declared members before publishing a root manifest with retained inheritance.
fn migrate_workspace(
    start: &Path,
    args: &Migrate,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    path_text(&args.destination)?;
    let path = args
        .accepted_bases
        .as_deref()
        .ok_or_else(|| Failure::new("source_workspace_pins_required"))?;
    path_text(path)?;
    let input =
        fs::read_to_string(path).map_err(|_| Failure::new("source_workspace_pins_invalid"))?;
    let pins = WorkspaceAcceptedRefs::from_json(&input)?;
    let context = discover_context(start)
        .map_err(|_| Failure::new("project_discovery_failed").at(start.to_string_lossy()))?;
    let DiscoveryContext::Workspace { workspace, .. } = context else {
        return Err(Failure::new("source_migration_scope_unsupported"));
    };
    path_text(workspace.root())?;
    let prepare = match args.layout {
        MigrationLayout::Readable => prepare_readable_workspace,
        MigrationLayout::Queries => prepare_query_workspace,
    };
    let plan = prepare(&workspace, &pins, &args.destination)?;
    let queries = matches!(args.layout, MigrationLayout::Queries);
    let destination = path_text(plan.destination())?;
    let members = plan
        .members()
        .iter()
        .map(|member| member_report(member, queries))
        .collect::<Result<Vec<_>, Failure>>()?;
    let publication = args.write.then(|| plan.publish()).transpose()?;
    let key = match (args.layout, args.write) {
        (MigrationLayout::Readable, true) => "tool-workspace-migrated",
        (MigrationLayout::Readable, false) => "tool-workspace-migration-preview",
        (MigrationLayout::Queries, true) => "tool-workspace-query-migrated",
        (MigrationLayout::Queries, false) => "tool-workspace-query-migration-preview",
    };
    let mut human = localizer.format(
        key,
        &[
            ("path", LocalizationValue::Text(destination)),
            (
                "count",
                LocalizationValue::Number(i64::try_from(plan.members().len()).unwrap_or(i64::MAX)),
            ),
        ],
    );
    for member in plan.members() {
        human.push('\n');
        human.push_str(&localizer.format(
            "tool-workspace-migration-member",
            &[
                ("name", LocalizationValue::Text(member.name().as_str())),
                (
                    "pin",
                    LocalizationValue::Text(&member.accepted_reference().sha256),
                ),
            ],
        ));
    }
    let mut data = json!({"scope":"workspace", "root":path_text(workspace.root())?,
            "destination":destination, "source_layout":"readable",
            "source_layout_version":RESOURCE_LAYOUT_VERSION,
            "descriptor_schema_version":READABLE_SCHEMA_VERSION,
            "layout_changed":plan.layout_changed(), "written":args.write,
            "publication":publication, "members":members,
            "copied_workspace_files":plan.ancillary_files().collect::<Vec<_>>(),
            "ci_pin_updated":false, "effective_git_policy_verified":false});
    if queries {
        data["source_layout"] = json!("queries");
        if let Some(object) = data.as_object_mut() {
            object.remove("source_layout_version");
        }
        data["source_layout_versions"] = json!(
            plan.members()
                .iter()
                .map(WorkspaceLayoutMember::source_layout_version)
                .collect::<BTreeSet<_>>()
        );
    }
    Ok((data, human))
}

/// Queries reports describe each actual member representation without changing older readable JSON.
fn member_report(member: &WorkspaceLayoutMember, queries: bool) -> Result<Value, Failure> {
    let mut data = json!({
        "name":member.name().as_str(), "path":member.path(),
        "destination":path_text(member.destination())?,
        "previous_baseline":member.previous_reference().sha256,
        "baseline":member.accepted_reference().sha256,
        "layout_changed":member.layout_changed(),
        "source_changes":member.source_changes(),
        "accepted_changes":member.accepted_changes(),
        "copied_project_files":member.ancillary_files().collect::<Vec<_>>()
    });
    if queries {
        data["source_layout_version"] = json!(member.source_layout_version());
        data["ancestry_preserved"] = json!(member.ancestry_preserved());
    }
    Ok(data)
}
