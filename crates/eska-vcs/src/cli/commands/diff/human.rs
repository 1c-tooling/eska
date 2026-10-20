//! Localized grouping, semantic labels and optional terminal styling.

use super::{WorkspaceFileDiff, analysis::NamedSemanticDiff, raw::raw_code};
use crate::{
    cli::{
        changes::{
            display_path, metadata_group_rank, metadata_kind, render_metadata_path,
            render_semantic_object, semantic_event_key, semantic_object_group,
        },
        localization::{LocalizationValue, Localizer},
    },
    project::{
        diff::{
            DisplayTarget, ProjectDiff, RevisionProjectDiff, WorkspaceDiff, WorkspaceRevisionDiff,
        },
        semantic::{self, SemanticDiff, SemanticEvent, SemanticEventKind},
    },
    vcs::status::Change,
};
use gix::bstr::ByteSlice;
use std::io::{self, IsTerminal};

/// Separate member sections from workspace-owned files in human output.
pub(super) fn render_workspace_human(
    changes: &WorkspaceDiff,
    localizer: &Localizer,
    styled: bool,
) -> String {
    let mut sections = changes
        .projects
        .iter()
        .map(|project| {
            format!(
                "{}\n{}",
                workspace_project_heading(project.name.as_str(), localizer, styled),
                render_human(&project.diff, localizer, styled)
            )
        })
        .collect::<Vec<_>>();
    if let Some(files) = &changes.workspace_files {
        sections.push(format!(
            "{}\n{}",
            style_header(&localizer.text("diff-workspace-files"), styled),
            render_human(files, localizer, styled)
        ));
    }
    sections.join("\n\n")
}

/// Retain revision context in each selected member section.
pub(super) fn render_workspace_revision_human(
    changes: &WorkspaceRevisionDiff,
    localizer: &Localizer,
    styled: bool,
) -> String {
    let mut sections = changes
        .projects
        .iter()
        .map(|project| {
            format!(
                "{}\n{}",
                workspace_project_heading(project.name.as_str(), localizer, styled),
                render_revision_human(&project.diff, localizer, styled)
            )
        })
        .collect::<Vec<_>>();
    if let Some(files) = &changes.workspace_files {
        sections.push(format!(
            "{}\n{}",
            style_header(&localizer.text("diff-workspace-files"), styled),
            render_revision_human(files, localizer, styled)
        ));
    }
    sections.join("\n\n")
}

/// Format one localized workspace member heading.
fn workspace_project_heading(name: &str, localizer: &Localizer, styled: bool) -> String {
    style_header(
        &localizer.format(
            "diff-workspace-project",
            &[("name", LocalizationValue::Text(name))],
        ),
        styled,
    )
}

/// Render semantic member sections with an optional file-only root section.
pub(super) fn render_workspace_semantic_human(
    semantic: &[NamedSemanticDiff],
    workspace_files: Option<WorkspaceFileDiff<'_>>,
    localizer: &Localizer,
    styled: bool,
) -> String {
    let mut sections = semantic
        .iter()
        .map(|project| {
            format!(
                "{}\n{}",
                workspace_project_heading(project.name.as_str(), localizer, styled),
                render_semantic_human(&project.diff, localizer, styled)
            )
        })
        .collect::<Vec<_>>();
    if let Some(files) = workspace_files {
        let rendered = match files {
            WorkspaceFileDiff::Current(files) => render_human(files, localizer, styled),
            WorkspaceFileDiff::Revisions(files) => render_revision_human(files, localizer, styled),
        };
        sections.push(format!(
            "{}\n{rendered}",
            style_header(&localizer.text("diff-workspace-files"), styled),
        ));
    }
    sections.join("\n\n")
}

/// Render readable one-line descriptions while retaining index/worktree distinctions.
pub(super) fn render_human(diff: &ProjectDiff, localizer: &Localizer, styled: bool) -> String {
    if diff.display.is_empty() {
        return localizer.text("diff-clean");
    }
    let header = localizer.text("diff-files");
    render_grouped(
        &header,
        diff.display.iter().filter_map(|change| {
            HumanState::workspace(change.index, change.worktree)
                .map(|state| (&change.target, state))
        }),
        localizer,
        styled,
    )
}

/// Render one committed comparison with explicit resolved endpoints.
pub(super) fn render_revision_human(
    diff: &RevisionProjectDiff,
    localizer: &Localizer,
    styled: bool,
) -> String {
    let comparison = &diff.comparison;
    let from_commit = comparison
        .merge_base_commit
        .unwrap_or(comparison.from_commit)
        .to_string();
    let to_commit = comparison.to_commit.to_string();
    let values = [
        ("from", LocalizationValue::Text(&comparison.from_revision)),
        ("to", LocalizationValue::Text(&comparison.to_revision)),
        (
            "from_commit",
            LocalizationValue::Text(short_id(&from_commit)),
        ),
        ("to_commit", LocalizationValue::Text(short_id(&to_commit))),
    ];
    if diff.display.is_empty() {
        return localizer.format("diff-revision-clean", &values);
    }
    let header = if comparison.merge_base_commit.is_some() {
        localizer.format("diff-revision-branch-files", &values)
    } else {
        localizer.format("diff-revision-files", &values)
    };
    render_grouped(
        &header,
        diff.display
            .iter()
            .map(|change| (&change.target, HumanState::Revision(change.change))),
        localizer,
        styled,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HumanState {
    Index(Change),
    Worktree(Change),
    IndexAndWorktree { index: Change, worktree: Change },
    Revision(Change),
}

impl HumanState {
    /// Represent simultaneous index and worktree changes as one human state.
    const fn workspace(index: Option<Change>, worktree: Option<Change>) -> Option<Self> {
        match (index, worktree) {
            (Some(index), Some(worktree)) => Some(Self::IndexAndWorktree { index, worktree }),
            (Some(change), None) => Some(Self::Index(change)),
            (None, Some(change)) => Some(Self::Worktree(change)),
            (None, None) => None,
        }
    }
}

/// Group logical changes first by metadata type and then by their exact state.
fn render_grouped<'a>(
    header: &str,
    changes: impl IntoIterator<Item = (&'a DisplayTarget, HumanState)>,
    localizer: &Localizer,
    styled: bool,
) -> String {
    let mut lines = vec![style_header(header, styled)];
    let mut groups: std::collections::BTreeMap<String, Vec<(String, HumanState)>> =
        std::collections::BTreeMap::new();
    let mut other_files = Vec::new();
    for (target, state) in changes {
        match target {
            DisplayTarget::Metadata(path) => groups
                .entry(metadata_kind(path.group, localizer))
                .or_default()
                .push((render_metadata_path(path, localizer), state)),
            DisplayTarget::File(path) => {
                other_files.push((display_path(path.as_bstr()), state));
            }
        }
    }
    for (group, changes) in groups {
        lines.push(String::new());
        lines.push(style_metadata_group(&format!("{group}:"), styled));
        append_state_groups(&mut lines, changes, localizer, styled);
    }
    if !other_files.is_empty() {
        lines.push(String::new());
        lines.push(style_metadata_group(
            &format!("{}:", localizer.text("diff-other-files")),
            styled,
        ));
        append_state_groups(&mut lines, other_files, localizer, styled);
    }
    lines.join("\n")
}

/// Append deterministic state subgroups with a compact marker column.
fn append_state_groups(
    lines: &mut Vec<String>,
    mut changes: Vec<(String, HumanState)>,
    localizer: &Localizer,
    styled: bool,
) {
    changes.sort_by(|left, right| {
        state_sort_key(left.1)
            .cmp(&state_sort_key(right.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut start = 0;
    while start < changes.len() {
        let state = changes[start].1;
        let end = changes[start..]
            .iter()
            .position(|(_, candidate)| *candidate != state)
            .map_or(changes.len(), |offset| start + offset);
        let count = end - start;
        let marker_change = marker_change(state);
        let title = human_state_title(state, localizer);
        lines.push(style_state_heading(&title, count, marker_change, styled));
        lines.extend(
            changes[start..end]
                .iter()
                .map(|(target, _)| format_change(target, marker_change, styled)),
        );
        start = end;
    }
}

/// Keep human commit labels compact while JSON retains complete object IDs.
fn short_id(id: &str) -> &str {
    &id[..id.len().min(7)]
}

/// Describe one state subgroup without repeating it for every target.
fn human_state_title(state: HumanState, localizer: &Localizer) -> String {
    match state {
        HumanState::Revision(change) => localizer.text(change_group_key(change)),
        HumanState::Index(change) => format!(
            "{} — {}",
            localizer.text(change_group_key(change)),
            localizer.text("diff-index")
        ),
        HumanState::Worktree(change) => format!(
            "{} — {}",
            localizer.text(change_group_key(change)),
            localizer.text("diff-worktree")
        ),
        HumanState::IndexAndWorktree { index, worktree } if index == worktree => format!(
            "{} — {}",
            localizer.text(change_group_key(index)),
            localizer.text("diff-index-and-worktree")
        ),
        HumanState::IndexAndWorktree { index, worktree } => format!(
            "{} — {}; {} — {}",
            localizer.text(change_group_key(index)),
            localizer.text("diff-index"),
            localizer.text(change_group_key(worktree)),
            localizer.text("diff-worktree")
        ),
    }
}

/// Use the latest worktree state for the marker while the heading retains both stages.
const fn marker_change(state: HumanState) -> Change {
    match state {
        HumanState::Revision(change) | HumanState::Index(change) | HumanState::Worktree(change) => {
            change
        }
        HumanState::IndexAndWorktree { worktree, .. } => worktree,
    }
}

/// Keep state groups deterministic across locales.
const fn state_sort_key(state: HumanState) -> (u8, char, char) {
    match state {
        HumanState::Index(change) => (change_rank(change), raw_code(Some(change)), raw_code(None)),
        HumanState::Revision(change) | HumanState::Worktree(change) => {
            (change_rank(change), raw_code(None), raw_code(Some(change)))
        }
        HumanState::IndexAndWorktree { index, worktree } => (
            change_rank(marker_change(state)),
            raw_code(Some(index)),
            raw_code(Some(worktree)),
        ),
    }
}

/// Order change groups independently of their translated labels.
const fn change_rank(change: Change) -> u8 {
    match change {
        Change::Modified => 0,
        Change::Added => 1,
        Change::Deleted => 2,
        Change::TypeChanged => 3,
        Change::Conflict => 4,
        Change::Untracked => 5,
        Change::IntentToAdd => 6,
    }
}

/// Keep compact human markers distinct from machine-facing raw codes.
const fn change_marker(change: Change) -> char {
    match change {
        Change::Modified => '✎',
        Change::Added => '+',
        Change::Deleted => '−',
        Change::TypeChanged => '↔',
        Change::Conflict => '!',
        Change::Untracked => '?',
        Change::IntentToAdd => '◌',
    }
}

/// Choose the terminal palette for each file state.
const fn change_color(change: Change) -> u8 {
    match change {
        Change::Modified => 33,
        Change::Added | Change::IntentToAdd => 32,
        Change::Deleted => 31,
        Change::TypeChanged => 35,
        Change::Conflict => 91,
        Change::Untracked => 36,
    }
}

/// Apply the top-level heading style only for interactive output.
fn style_header(header: &str, styled: bool) -> String {
    if styled {
        format!("\x1b[1m{header}\x1b[0m")
    } else {
        header.to_owned()
    }
}

/// Distinguish metadata group headings when terminal styling is enabled.
fn style_metadata_group(group: &str, styled: bool) -> String {
    if styled {
        format!("\x1b[1;36m{group}\x1b[0m")
    } else {
        group.to_owned()
    }
}

/// Include the group count without repeating the state on every item.
fn style_state_heading(title: &str, count: usize, change: Change, styled: bool) -> String {
    if styled {
        format!(
            "  \x1b[1;{}m{title}\x1b[0m \x1b[2m({count})\x1b[0m:",
            change_color(change)
        )
    } else {
        format!("  {title} ({count}):")
    }
}

/// Render one target with its compact state marker.
fn format_change(target: &str, change: Change, styled: bool) -> String {
    let marker = change_marker(change);
    if styled {
        format!(
            "    \x1b[1;{}m{marker}\x1b[0m {target}",
            change_color(change)
        )
    } else {
        format!("    {marker} {target}")
    }
}

/// Respect redirected stdout and an explicit `NO_COLOR` setting.
pub(super) fn styling_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Select the plural localized title of a grouped file state.
const fn change_group_key(change: Change) -> &'static str {
    match change {
        Change::Added => "diff-group-added",
        Change::Modified => "diff-group-modified",
        Change::Deleted => "diff-group-deleted",
        Change::TypeChanged => "diff-group-type-changed",
        Change::Untracked => "diff-group-untracked",
        Change::IntentToAdd => "diff-group-intent-to-add",
        Change::Conflict => "diff-group-conflict",
    }
}

/// Render localized semantic event labels while retaining stable object identities.
pub(super) fn render_semantic_human(
    diff: &SemanticDiff,
    localizer: &Localizer,
    styled: bool,
) -> String {
    if diff.is_empty() {
        return localizer.text("diff-semantic-clean");
    }
    let mut aggregated = std::collections::BTreeMap::new();
    for event in diff.events() {
        let group = semantic_object_group(event.object().id()).to_owned();
        let object = render_semantic_object(event.object().id(), localizer);
        let target = semantic_event_target(event, &object, localizer);
        aggregated
            .entry((group, object, target, event.kind(), event.location()))
            .and_modify(|stage: &mut SemanticHumanStage| stage.merge(event.stage()))
            .or_insert_with(|| SemanticHumanStage::from(event.stage()));
    }
    let mut groups: std::collections::BTreeMap<String, Vec<SemanticHumanChange>> =
        std::collections::BTreeMap::new();
    for ((group, object, target, kind, location), stage) in aggregated {
        groups.entry(group).or_default().push(SemanticHumanChange {
            object,
            target,
            location,
            kind,
            stage,
        });
    }
    let mut groups = groups.into_iter().collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        metadata_group_rank(&left.0)
            .cmp(&metadata_group_rank(&right.0))
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut lines = vec![style_header(
        &localizer.text("diff-semantic-events"),
        styled,
    )];
    for (group, changes) in groups {
        lines.push(String::new());
        lines.push(style_metadata_group(
            &format!("{}:", metadata_kind(&group, localizer)),
            styled,
        ));
        append_semantic_event_groups(&mut lines, changes, localizer, styled);
    }
    lines.join("\n")
}

/// Render a semantic target, adding declaration kind and coordinates only for routines.
fn semantic_event_target(event: &SemanticEvent, object: &str, localizer: &Localizer) -> String {
    let Some(member) = event.member() else {
        return object.to_owned();
    };
    let declaration = if matches!(
        event.kind(),
        SemanticEventKind::FunctionAdded
            | SemanticEventKind::FunctionRemoved
            | SemanticEventKind::FunctionChanged
    ) {
        localizer.text("diff-semantic-function")
    } else {
        localizer.text("diff-semantic-procedure")
    };
    event.location().map_or_else(
        || format!("{object} — {declaration}.{member}"),
        |location| {
            format!(
                "{object} — {declaration}.{member} ({}, {})",
                location.line, location.column
            )
        },
    )
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SemanticHumanChange {
    object: String,
    target: String,
    location: Option<semantic::SourceLocation>,
    kind: SemanticEventKind,
    stage: SemanticHumanStage,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SemanticHumanStage {
    Index,
    Worktree,
    IndexAndWorktree,
    Revision,
}

impl SemanticHumanStage {
    /// Merge identical index/worktree events into one compact human state.
    const fn merge(&mut self, stage: semantic::ChangeStage) {
        if matches!(
            (*self, stage),
            (Self::Index, semantic::ChangeStage::Worktree)
                | (Self::Worktree, semantic::ChangeStage::Index)
        ) {
            *self = Self::IndexAndWorktree;
        }
    }
}

impl From<semantic::ChangeStage> for SemanticHumanStage {
    /// Convert one machine comparison edge into its initial human stage.
    fn from(stage: semantic::ChangeStage) -> Self {
        match stage {
            semantic::ChangeStage::Index => Self::Index,
            semantic::ChangeStage::Worktree => Self::Worktree,
            semantic::ChangeStage::Revision => Self::Revision,
        }
    }
}

/// Append event/stage subgroups with the same hierarchy and styling as file-level diff.
fn append_semantic_event_groups(
    lines: &mut Vec<String>,
    mut changes: Vec<SemanticHumanChange>,
    localizer: &Localizer,
    styled: bool,
) {
    changes.sort_by(|left, right| {
        semantic_event_sort_key(left)
            .cmp(&semantic_event_sort_key(right))
            .then_with(|| left.object.cmp(&right.object))
            .then_with(|| location_sort_key(left.location).cmp(&location_sort_key(right.location)))
            .then_with(|| {
                semantic_declaration_rank(left.kind).cmp(&semantic_declaration_rank(right.kind))
            })
            .then_with(|| left.target.cmp(&right.target))
    });
    let mut start = 0;
    while start < changes.len() {
        let kind = semantic_event_group_kind(changes[start].kind);
        let stage = changes[start].stage;
        let end = changes[start..]
            .iter()
            .position(|candidate| {
                semantic_event_group_kind(candidate.kind) != kind || candidate.stage != stage
            })
            .map_or(changes.len(), |offset| start + offset);
        let title = format!(
            "{} — {}",
            localizer.text(semantic_event_group_key(kind)),
            localizer.text(semantic_stage_key(stage))
        );
        let change = semantic_event_change(kind);
        lines.push(style_state_heading(&title, end - start, change, styled));
        lines.extend(
            changes[start..end]
                .iter()
                .map(|change| format_change(&change.target, semantic_event_change(kind), styled)),
        );
        start = end;
    }
}

/// Sort modified, added and removed semantic events like ordinary diff states.
const fn semantic_event_sort_key(change: &SemanticHumanChange) -> (u8, u8, u8) {
    (
        change_rank(semantic_event_change(change.kind)),
        semantic_event_kind_rank(semantic_event_group_kind(change.kind)),
        semantic_stage_rank(change.stage),
    )
}

/// Keep routines in their declaration order inside one metadata object.
const fn location_sort_key(location: Option<semantic::SourceLocation>) -> (usize, usize) {
    match location {
        Some(location) => (location.line, location.column),
        None => (usize::MAX, usize::MAX),
    }
}

/// Break ties deterministically when routine coordinates are unavailable or equal.
const fn semantic_declaration_rank(kind: SemanticEventKind) -> u8 {
    match kind {
        SemanticEventKind::FunctionAdded
        | SemanticEventKind::FunctionRemoved
        | SemanticEventKind::FunctionChanged => 1,
        _ => 0,
    }
}

/// Group procedures and functions by lifecycle without changing their machine event kinds.
const fn semantic_event_group_kind(kind: SemanticEventKind) -> SemanticEventKind {
    match kind {
        SemanticEventKind::FunctionAdded => SemanticEventKind::MethodAdded,
        SemanticEventKind::FunctionRemoved => SemanticEventKind::MethodRemoved,
        SemanticEventKind::FunctionChanged => SemanticEventKind::MethodChanged,
        other => other,
    }
}

/// Select human-only routine group labels while retaining existing labels elsewhere.
const fn semantic_event_group_key(kind: SemanticEventKind) -> &'static str {
    match kind {
        SemanticEventKind::MethodAdded => "diff-semantic-routine-added",
        SemanticEventKind::MethodRemoved => "diff-semantic-routine-removed",
        SemanticEventKind::MethodChanged => "diff-semantic-routine-changed",
        other => semantic_event_key(other),
    }
}

/// Keep related object, module and member event groups deterministic.
const fn semantic_event_kind_rank(kind: SemanticEventKind) -> u8 {
    match kind {
        SemanticEventKind::ObjectChanged
        | SemanticEventKind::ObjectAdded
        | SemanticEventKind::ObjectRemoved => 0,
        SemanticEventKind::MetadataAttributeChanged => 1,
        SemanticEventKind::ModuleChanged => 2,
        SemanticEventKind::MethodChanged
        | SemanticEventKind::MethodAdded
        | SemanticEventKind::MethodRemoved
        | SemanticEventKind::FunctionChanged
        | SemanticEventKind::FunctionAdded
        | SemanticEventKind::FunctionRemoved => 3,
        SemanticEventKind::FormChanged => 4,
    }
}

/// Keep workspace edges in index-to-worktree order; revisions form their own subgroup.
const fn semantic_stage_rank(stage: SemanticHumanStage) -> u8 {
    match stage {
        SemanticHumanStage::Index => 0,
        SemanticHumanStage::Worktree => 1,
        SemanticHumanStage::IndexAndWorktree => 2,
        SemanticHumanStage::Revision => 3,
    }
}

/// Select the localized comparison-edge label used in a subgroup heading.
const fn semantic_stage_key(stage: SemanticHumanStage) -> &'static str {
    match stage {
        SemanticHumanStage::Index => "diff-semantic-index",
        SemanticHumanStage::Worktree => "diff-semantic-worktree",
        SemanticHumanStage::IndexAndWorktree => "diff-semantic-index-and-worktree",
        SemanticHumanStage::Revision => "diff-semantic-revision",
    }
}

/// Map semantic lifecycle to the existing diff color and marker palette.
const fn semantic_event_change(kind: SemanticEventKind) -> Change {
    match kind {
        SemanticEventKind::ObjectAdded
        | SemanticEventKind::MethodAdded
        | SemanticEventKind::FunctionAdded => Change::Added,
        SemanticEventKind::ObjectRemoved
        | SemanticEventKind::MethodRemoved
        | SemanticEventKind::FunctionRemoved => Change::Deleted,
        SemanticEventKind::ObjectChanged
        | SemanticEventKind::ModuleChanged
        | SemanticEventKind::MethodChanged
        | SemanticEventKind::FunctionChanged
        | SemanticEventKind::FormChanged
        | SemanticEventKind::MetadataAttributeChanged => Change::Modified,
    }
}

#[cfg(test)]
mod tests;
