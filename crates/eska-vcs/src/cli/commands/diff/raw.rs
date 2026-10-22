//! Stable tab-separated output, independent of locale and terminal capabilities.

use super::{WorkspaceFileDiff, analysis::NamedSemanticDiff};
use crate::{
    cli::changes::display_path,
    project::{
        diff::{ProjectDiff, RevisionProjectDiff, WorkspaceDiff, WorkspaceRevisionDiff},
        semantic::SemanticDiff,
    },
    vcs::status::Change,
};
use gix::bstr::ByteSlice;

/// Prefix current file rows with their workspace member name.
pub(super) fn render_workspace_raw(changes: &WorkspaceDiff) -> String {
    use std::fmt::Write as _;
    let mut output = String::new();
    for project in &changes.projects {
        for line in render_raw(&project.diff).lines() {
            writeln!(output, "{}\t{line}", project.name).expect("writing to String cannot fail");
        }
    }
    if let Some(files) = &changes.workspace_files {
        for line in render_raw(files).lines() {
            writeln!(output, "-\t{line}").expect("writing to String cannot fail");
        }
    }
    output
}

/// Prefix revision rows without changing their existing columns.
pub(super) fn render_workspace_revision_raw(changes: &WorkspaceRevisionDiff) -> String {
    use std::fmt::Write as _;
    let mut output = String::new();
    for project in &changes.projects {
        for line in render_revision_raw(&project.diff).lines() {
            writeln!(output, "{}\t{line}", project.name).expect("writing to String cannot fail");
        }
    }
    if let Some(files) = &changes.workspace_files {
        for line in render_revision_raw(files).lines() {
            writeln!(output, "-\t{line}").expect("writing to String cannot fail");
        }
    }
    output
}

/// Prefix semantic rows and retain file-only workspace rows.
pub(super) fn render_workspace_semantic_raw(
    semantic: &[NamedSemanticDiff],
    workspace_files: Option<WorkspaceFileDiff<'_>>,
) -> String {
    use std::fmt::Write as _;
    let mut output = String::new();
    for project in semantic {
        for line in render_semantic_raw(&project.diff).lines() {
            writeln!(output, "{}\t{line}", project.name).expect("writing to String cannot fail");
        }
    }
    if let Some(files) = workspace_files {
        let rendered = match files {
            WorkspaceFileDiff::Current(files) => render_raw(files),
            WorkspaceFileDiff::Revisions(files) => render_revision_raw(files),
        };
        for line in rendered.lines() {
            writeln!(output, "-\t{line}").expect("writing to String cannot fail");
        }
    }
    output
}

/// Render a compact locale-independent two-column representation.
pub(super) fn render_raw(diff: &ProjectDiff) -> String {
    use std::fmt::Write as _;

    let mut output = String::new();
    for file in &diff.files {
        writeln!(
            output,
            "{}{}\t{}",
            raw_code(file.index),
            raw_code(file.worktree),
            display_path(file.path.as_bstr())
        )
        .expect("writing to String cannot fail");
    }
    output
}

/// Render one stable status column for an exact revision comparison.
pub(super) fn render_revision_raw(diff: &RevisionProjectDiff) -> String {
    use std::fmt::Write as _;

    let mut output = String::new();
    for file in &diff.files {
        writeln!(
            output,
            "{}\t{}",
            raw_code(Some(file.change)),
            display_path(file.path.as_bstr())
        )
        .expect("writing to String cannot fail");
    }
    output
}

/// Map a file state to the stable raw column code.
pub(super) const fn raw_code(change: Option<Change>) -> char {
    match change {
        None => '.',
        Some(Change::Added) => 'A',
        Some(Change::Modified) => 'M',
        Some(Change::Deleted) => 'D',
        Some(Change::TypeChanged) => 'T',
        Some(Change::Untracked) => '?',
        Some(Change::IntentToAdd) => 'I',
        Some(Change::Conflict) => 'U',
    }
}

/// Render one stable tab-separated semantic event per line.
pub(super) fn render_semantic_raw(diff: &SemanticDiff) -> String {
    use std::fmt::Write as _;

    let mut output = String::new();
    for event in diff.events() {
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            event.stage().as_str(),
            event.kind().as_str(),
            event.object().id(),
            event.member().unwrap_or("-"),
            display_path(event.path())
        )
        .expect("writing to String cannot fail");
    }
    output
}
