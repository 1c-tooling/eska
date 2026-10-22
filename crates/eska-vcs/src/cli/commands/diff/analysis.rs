//! Semantic analysis and localized fallback diagnostics for selected projects.

use crate::{
    cli::{
        changes::{display_path, semantic_fallback_key},
        localization::{LocalizationValue, Localizer},
    },
    project::{
        ProjectName, WorkspaceMember,
        diff::{ProjectDiff, RevisionProjectDiff, WorkspaceDiff, WorkspaceRevisionDiff},
        semantic::{self, SemanticDiff, SemanticDiffError},
    },
};

pub(super) struct NamedSemanticDiff {
    pub(super) name: ProjectName,
    pub(super) diff: SemanticDiff,
}

/// Discover the current logical model and analyze workspace snapshot pairs.
pub(super) fn analyze_workspace_semantics(
    project: &crate::project::Project,
    changes: &ProjectDiff,
    localizer: &Localizer,
) -> Result<SemanticDiff, SemanticDiffError> {
    let diff = semantic::diff_workspace_affected(project, changes)?;
    report_semantic_fallbacks(&diff, None, localizer);
    Ok(diff)
}

/// Analyze committed tree blob pairs independently of the current worktree contents.
pub(super) fn analyze_revision_semantics(
    project: &crate::project::Project,
    changes: &RevisionProjectDiff,
    localizer: &Localizer,
) -> Result<SemanticDiff, SemanticDiffError> {
    let diff = semantic::diff_revisions(project, changes)?;
    report_semantic_fallbacks(&diff, None, localizer);
    Ok(diff)
}

/// Analyze only selected members and attach their names to fallback diagnostics.
pub(super) fn analyze_workspace_group(
    changes: &WorkspaceDiff,
    selected: &[&WorkspaceMember],
    localizer: &Localizer,
) -> Result<Vec<NamedSemanticDiff>, SemanticDiffError> {
    changes
        .projects
        .iter()
        .zip(selected)
        .map(|(changes, member)| {
            let diff = semantic::diff_workspace_affected(member.project(), &changes.diff)?;
            report_semantic_fallbacks(&diff, Some(changes.name.as_str()), localizer);
            Ok(NamedSemanticDiff {
                name: changes.name.clone(),
                diff,
            })
        })
        .collect()
}

/// Analyze committed member changes in the same order as their file diffs.
pub(super) fn analyze_workspace_revision_group(
    changes: &WorkspaceRevisionDiff,
    selected: &[&WorkspaceMember],
    localizer: &Localizer,
) -> Result<Vec<NamedSemanticDiff>, SemanticDiffError> {
    changes
        .projects
        .iter()
        .zip(selected)
        .map(|(changes, member)| {
            let diff = semantic::diff_revisions(member.project(), &changes.diff)?;
            report_semantic_fallbacks(&diff, Some(changes.name.as_str()), localizer);
            Ok(NamedSemanticDiff {
                name: changes.name.clone(),
                diff,
            })
        })
        .collect()
}

/// Explain every non-fatal reduction while leaving machine events locale-independent.
fn report_semantic_fallbacks(diff: &SemanticDiff, project: Option<&str>, localizer: &Localizer) {
    for fallback in diff.fallbacks() {
        let path = display_path(fallback.path());
        let path = project.map_or_else(|| path.clone(), |project| format!("{project}/{path}"));
        let reason = localizer.text(semantic_fallback_key(fallback.reason()));
        eprintln!(
            "{}",
            localizer.format(
                "semantic-fallback",
                &[
                    ("path", LocalizationValue::Text(&path)),
                    ("reason", LocalizationValue::Text(&reason)),
                ],
            )
        );
    }
}
