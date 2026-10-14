//! Exact source changes and authority references are reported independently of locale and platform.

use std::fmt::Write;

use serde_json::{Value, json};

use crate::{
    project_core::{Project, metadata_model::ProjectScope, source_format::ProjectSourceImportPlan},
    tool_cli::{
        Failure,
        localization::{LocalizationValue, Localizer},
        path_text,
    },
};

/// Preview and failure responses expose the same concrete changes, conflicts and recovery digest.
pub(super) fn preview(
    project: &Project,
    scope: &ProjectScope,
    plan: &ProjectSourceImportPlan,
    events: &[Value],
) -> Result<Value, Failure> {
    let source = plan.source_plan();
    let scope = match scope {
        ProjectScope::Standalone => json!({"kind":"standalone"}),
        ProjectScope::Member(name) => json!({"kind":"member","project":name.as_str()}),
    };
    Ok(json!({
        "project":path_text(project.root())?,"scope":scope,"source_format":"eska",
        "project_type":project.configuration().project_type().as_str(),"applied":false,
        "can_apply":source.conflicts().is_empty(),"approved":source.approved_reference(),
        "ancestor":source.ancestor_reference(),"incoming":source.incoming_reference(),
        "resulting_ancestor":plan.resulting_ancestor_reference(),"transaction":plan.reference(),
        "working_revision":source.working_revision(),"output_revision":source.output_revision(),
        "unchanged":plan.is_unchanged(),
        "changes":source.changes(),"conflicts":source.conflicts(),"support":source.support_permissions(),"platform":events,
    }))
}

/// Human output lists every affected path; structured conflict categories remain stable identifiers.
pub(super) fn human(plan: &ProjectSourceImportPlan, write: bool, localizer: &Localizer) -> String {
    let source = plan.source_plan();
    let mut human = localizer.format(
        if write && plan.is_unchanged() {
            "tool-build-source-import-unchanged"
        } else if write {
            "tool-build-source-imported"
        } else {
            "tool-build-source-import-preview"
        },
        &[
            (
                "changes",
                LocalizationValue::Number(
                    i64::try_from(source.changes().len()).unwrap_or(i64::MAX),
                ),
            ),
            (
                "conflicts",
                LocalizationValue::Number(
                    i64::try_from(source.conflicts().len()).unwrap_or(i64::MAX),
                ),
            ),
            ("pin", LocalizationValue::Text(plan.reference().sha256())),
        ],
    );
    for change in source.changes() {
        let path = change
            .selected
            .as_ref()
            .or(change.working.as_ref())
            .or(change.incoming.as_ref())
            .or(change.ancestor.as_ref());
        if let Some(path) = path {
            let _ = write!(
                human,
                "\n{}",
                localizer.format(
                    "tool-build-source-import-file",
                    &[("path", LocalizationValue::Text(path.path.as_str()))]
                )
            );
        }
    }
    for conflict in source.conflicts() {
        let kind = serde_json::to_value(conflict.kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        let _ = write!(
            human,
            "\n{}",
            localizer.format(
                "tool-build-source-import-conflict",
                &[
                    ("path", LocalizationValue::Text(conflict.path.as_str())),
                    ("kind", LocalizationValue::Text(&kind)),
                ]
            )
        );
    }
    human
}
