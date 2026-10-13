//! Explicit pin selection and localized preview; source comparison and writes stay in core.

mod workspace;

use std::{fmt::Write, path::Path};

use serde_json::{Value, json};

use super::super::args::UpdateBase;
use crate::{
    project_core::source_format::{
        BaseUpdateConflict, BaseUpdateConflictKind, prepare_project_base_update,
    },
    tool_cli::{
        self, Failure,
        localization::{LocalizationValue, Localizer},
        path_text, selected,
    },
};

/// Preview is read-only; explicit write requires zero unresolved conflicts and publishes a new copy.
pub(super) fn update(
    start: &Path,
    args: &UpdateBase,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    if args.workspace {
        return workspace::update(start, args, localizer);
    }
    path_text(&args.incoming)?;
    path_text(&args.destination)?;
    let previous = tool_cli::accepted_ref(args.accepted_base_sha256.as_deref())?;
    let required = args
        .incoming_base_sha256
        .as_deref()
        .ok_or_else(|| Failure::new("accepted_base_pin_required"))?;
    let incoming_pin = tool_cli::accepted_ref(Some(required))?;
    let current = selected(start, args.selector.project.as_deref())?;
    let incoming = selected(&args.incoming, args.incoming_project.as_deref())?;
    let plan = prepare_project_base_update(
        &current.project,
        &current.scope,
        &previous,
        &incoming.project,
        &incoming_pin,
        &args.destination,
    )?;
    let source = plan.source_plan();
    let mut data = json!({
        "root":path_text(current.project.root())?,
        "incoming_root":path_text(incoming.project.root())?,
        "destination":path_text(plan.destination())?,
        "previous_baseline":source.previous_reference().sha256,
        "baseline":source.incoming_reference().sha256,
        "working_revision":source.working_revision(), "output_revision":source.output_revision(),
        "comparison":"whole_file_three_way", "can_write":source.conflicts().is_empty(),
        "changes":source.changes(), "support_changes":source.support_changes(),
        "conflicts":source.conflicts(), "written":false, "publication":null,
        "copied_project_files":plan.ancillary_files().collect::<Vec<_>>(),
        "ci_pin_updated":false, "incoming_working_source_used":false,
        "effective_git_policy_verified":false,
    });
    if args.write {
        let publication = plan.publish().map_err(|error| Failure {
            diagnostics: data.clone(),
            ..Failure::from(error)
        })?;
        data["publication"] = json!(publication);
        data["written"] = json!(true);
    }
    let key = if args.write {
        "tool-project-base-updated"
    } else {
        "tool-project-base-update-preview"
    };
    let mut human = localizer.format(
        key,
        &[
            (
                "path",
                LocalizationValue::Text(path_text(plan.destination())?),
            ),
            (
                "pin",
                LocalizationValue::Text(&source.incoming_reference().sha256),
            ),
            (
                "change_count",
                LocalizationValue::Number(
                    i64::try_from(source.changes().len()).unwrap_or(i64::MAX),
                ),
            ),
            (
                "conflict_count",
                LocalizationValue::Number(
                    i64::try_from(source.conflicts().len()).unwrap_or(i64::MAX),
                ),
            ),
        ],
    );
    append_conflicts(&mut human, source.conflicts(), localizer);
    Ok((data, human))
}

/// Identical core conflicts receive the same human description in project and workspace previews.
fn append_conflicts(human: &mut String, conflicts: &[BaseUpdateConflict], localizer: &Localizer) {
    for conflict in conflicts {
        let location = conflict
            .working
            .as_ref()
            .or(conflict.previous.as_ref())
            .or(conflict.incoming.as_ref())
            .map_or_else(
                || conflict.uuid.as_ref().map_or("", |uuid| uuid.as_str()),
                |path| path.as_str(),
            );
        let _ = write!(
            human,
            "\n{}: {location}",
            localizer.text(conflict_key(conflict.kind))
        );
    }
}

/// Stable core kinds receive human descriptions only at this presentation boundary.
const fn conflict_key(kind: BaseUpdateConflictKind) -> &'static str {
    match kind {
        BaseUpdateConflictKind::BothModified => "tool-base-conflict-both-modified",
        BaseUpdateConflictKind::DeletedModified => "tool-base-conflict-deleted-modified",
        BaseUpdateConflictKind::ModifiedDeleted => "tool-base-conflict-modified-deleted",
        BaseUpdateConflictKind::AuthorityEdited => "tool-base-conflict-authority-edited",
        BaseUpdateConflictKind::PermissionDenied => "tool-base-conflict-permission-denied",
        BaseUpdateConflictKind::FreezeRemoved => "tool-base-conflict-freeze-removed",
        BaseUpdateConflictKind::UnmappedFile => "tool-base-conflict-unmapped-file",
        BaseUpdateConflictKind::UnmappedDirectory => "tool-base-conflict-unmapped-directory",
    }
}
