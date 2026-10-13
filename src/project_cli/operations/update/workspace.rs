//! Workspace CLI requires two complete approvals and reports every member before writing.

use super::{UpdateBase, append_conflicts};
use crate::{
    project_core::{
        Workspace,
        discovery::{DiscoveryContext, discover_context},
        source_format::{WorkspaceAcceptedRefs, prepare_workspace_base_update},
    },
    tool_cli::{
        Failure,
        localization::{LocalizationValue, Localizer},
        path_text,
    },
};
use serde_json::{Value, json};
use std::{fs, path::Path};

/// Approval is explicit for both workspaces; one environment checksum never approves members.
pub(super) fn update(
    start: &Path,
    args: &UpdateBase,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    let current = workspace(start)?;
    let incoming = workspace(&args.incoming)?;
    let previous = pins(args.accepted_bases.as_deref())?;
    let new = pins(args.incoming_bases.as_deref())?;
    path_text(&args.destination)?;
    let plan =
        prepare_workspace_base_update(&current, &previous, &incoming, &new, &args.destination)?;
    let members = plan.members().iter().map(|member| {
        let source = member.source_plan();
        Ok(json!({
            "name":member.name().as_str(), "path":member.path(),
            "destination":path_text(member.destination())?,
            "previous_baseline":source.previous_reference().sha256,
            "baseline":source.incoming_reference().sha256,
            "working_revision":source.working_revision(), "output_revision":source.output_revision(),
            "can_write":source.conflicts().is_empty(), "changes":source.changes(),
            "support_changes":source.support_changes(), "conflicts":source.conflicts(),
            "copied_project_files":member.ancillary_files().collect::<Vec<_>>(),
        }))
    }).collect::<Result<Vec<_>, Failure>>()?;
    let mut data = json!({
        "scope":"workspace", "root":path_text(current.root())?, "incoming_root":path_text(incoming.root())?,
        "destination":path_text(plan.destination())?, "comparison":"whole_file_three_way",
        "can_write":plan.can_write(), "members":members, "written":false, "publication":null,
        "copied_workspace_files":plan.ancillary_files().collect::<Vec<_>>(),
        "ci_pin_updated":false, "incoming_working_source_used":false,
        "effective_git_policy_verified":false,
    });
    if args.write {
        let publication = plan.publish().map_err(|error| Failure {
            diagnostics: data.clone(),
            ..Failure::from(error)
        })?;
        data["written"] = json!(true);
        data["publication"] = json!(publication);
    }
    let count = plan
        .members()
        .iter()
        .map(|member| member.source_plan().conflicts().len())
        .sum::<usize>();
    let mut human = localizer.format(
        if args.write {
            "tool-workspace-base-updated"
        } else {
            "tool-workspace-base-update-preview"
        },
        &[
            (
                "path",
                LocalizationValue::Text(path_text(plan.destination())?),
            ),
            (
                "count",
                LocalizationValue::Number(i64::try_from(plan.members().len()).unwrap_or(i64::MAX)),
            ),
            (
                "conflict_count",
                LocalizationValue::Number(i64::try_from(count).unwrap_or(i64::MAX)),
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
                    LocalizationValue::Text(&member.source_plan().incoming_reference().sha256),
                ),
            ],
        ));
        append_conflicts(&mut human, member.source_plan().conflicts(), localizer);
    }
    Ok((data, human))
}

/// Shared discovery validates names and inheritance; standalone roots cannot impersonate workspaces.
fn workspace(start: &Path) -> Result<Workspace, Failure> {
    path_text(start)?;
    let context = discover_context(start)
        .map_err(|_| Failure::new("project_discovery_failed").at(start.to_string_lossy()))?;
    let DiscoveryContext::Workspace { workspace, .. } = context else {
        return Err(Failure::new("source_update_scope_unsupported"));
    };
    path_text(workspace.root())?;
    Ok(workspace)
}

/// Both documents use the existing strict schema with complete, unique member names.
fn pins(path: Option<&Path>) -> Result<WorkspaceAcceptedRefs, Failure> {
    let path = path.ok_or_else(|| Failure::new("source_workspace_pins_required"))?;
    path_text(path)?;
    let text =
        fs::read_to_string(path).map_err(|_| Failure::new("source_workspace_pins_invalid"))?;
    Ok(WorkspaceAcceptedRefs::from_json(&text)?)
}
