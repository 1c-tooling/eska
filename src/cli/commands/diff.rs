//! Localized human output and stable machine representations of project file changes.

use std::{
    io::{self, IsTerminal},
    path::Path,
    process::ExitCode,
};

use clap::{Args, ValueEnum};
use gix::bstr::{BStr, ByteSlice};
use serde::Serialize;

use crate::{
    cli::{
        changes::{
            display_path, metadata_kind, render_metadata_path, render_semantic_object,
            semantic_event_key, semantic_object_group,
        },
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        Project, ProjectName, WorkspaceMember,
        diff::{
            self, DiffError, DisplayTarget, ProjectDiff, RevisionProjectDiff, WorkspaceDiff,
            WorkspaceRevisionDiff,
        },
        discovery::{self, DiscoveryContext},
        object_model,
        selection::{SelectionIntent, select_projects},
        semantic::{self, SemanticDiff, SemanticEvent, SemanticEventKind},
    },
    vcs::status::Change,
};

#[derive(Debug, Args)]
pub(in crate::cli) struct DiffArgs {
    #[arg(value_name = "REVISION", num_args = 0..=2)]
    revisions: Vec<String>,

    #[arg(long, requires = "revisions")]
    since_branch_point: bool,

    #[arg(long, conflicts_with = "format")]
    raw: bool,

    #[arg(long)]
    semantic: bool,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[command(flatten)]
    selectors: DiffSelectors,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Args)]
struct DiffSelectors {
    #[arg(short = 'p', long)]
    project: Vec<String>,

    #[arg(long)]
    workspace: bool,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

impl DiffArgs {
    /// Discover the project, inspect its file changes and select one presentation.
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let context = match discovery::discover_context(project_dir) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("{}", diagnostics::present_context_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let selection = match select_projects(
            &context,
            &self.selectors.project,
            self.selectors.workspace,
            SelectionIntent::ReadOnly,
        ) {
            Ok(selection) => selection,
            Err(error) => {
                eprintln!(
                    "{}",
                    diagnostics::present_selection_error(&error, localizer)
                );
                return ExitCode::FAILURE;
            }
        };
        if !selection.is_aggregate() {
            let Some(selected) = selection.projects().first() else {
                eprintln!("{}", localizer.text("project-selector-single-required"));
                return ExitCode::FAILURE;
            };
            return self.run_project(selected.project(), localizer);
        }
        let DiscoveryContext::Workspace {
            workspace,
            current_member,
        } = &context
        else {
            eprintln!("{}", localizer.text("project-selector-single-required"));
            return ExitCode::FAILURE;
        };
        let selected = selection
            .projects()
            .iter()
            .filter_map(|project| project.name().and_then(|name| workspace.member(name)))
            .collect::<Vec<_>>();
        if selected.len() != selection.projects().len() {
            eprintln!("{}", localizer.text("project-selector-single-required"));
            return ExitCode::FAILURE;
        }
        let include_workspace_files = self.selectors.workspace
            || (self.selectors.project.is_empty() && current_member.is_none());
        if self.revisions.is_empty() {
            let changes =
                match diff::inspect_workspace(workspace, &selected, include_workspace_files) {
                    Ok(changes) => changes,
                    Err(error) => return report_error(&error, localizer),
                };
            self.render_workspace_group(&changes, &selected, localizer)
        } else {
            let from = &self.revisions[0];
            let to = self.revisions.get(1).map_or("HEAD", String::as_str);
            let changes = match diff::compare_workspace(
                workspace,
                &selected,
                include_workspace_files,
                from,
                to,
                self.since_branch_point,
            ) {
                Ok(changes) => changes,
                Err(error) => return report_error(&error, localizer),
            };
            self.render_workspace_revisions(&changes, &selected, localizer)
        }
    }

    fn run_project(&self, project: &Project, localizer: &Localizer) -> ExitCode {
        if self.revisions.is_empty() {
            let changes = match diff::inspect(project) {
                Ok(changes) => changes,
                Err(error) => return report_error(&error, localizer),
            };
            if self.semantic {
                let Some(changes) = analyze_workspace_semantics(project, &changes, localizer)
                else {
                    return ExitCode::FAILURE;
                };
                return self.render_semantic(&changes, None, localizer);
            }
            self.render_workspace(&changes, localizer)
        } else {
            let from = &self.revisions[0];
            let to = self.revisions.get(1).map_or("HEAD", String::as_str);
            let changes = match diff::compare(project, from, to, self.since_branch_point) {
                Ok(changes) => changes,
                Err(error) => return report_error(&error, localizer),
            };
            if self.semantic {
                let Some(semantic) = analyze_revision_semantics(project, &changes, localizer)
                else {
                    return ExitCode::FAILURE;
                };
                return self.render_semantic(&semantic, Some(&changes), localizer);
            }
            self.render_revisions(&changes, localizer)
        }
    }

    /// Render the unchanged workspace comparison contract.
    fn render_workspace(&self, changes: &ProjectDiff, localizer: &Localizer) -> ExitCode {
        if self.raw {
            print!("{}", render_raw(changes));
            return ExitCode::SUCCESS;
        }
        match self.format {
            OutputFormat::Human => {
                println!("{}", render_human(changes, localizer, styling_enabled()));
                ExitCode::SUCCESS
            }
            OutputFormat::Json => serialize_json(&DiffDocument::from(changes), localizer),
        }
    }

    /// Render a committed revision comparison without workspace-stage terminology.
    fn render_revisions(&self, changes: &RevisionProjectDiff, localizer: &Localizer) -> ExitCode {
        if self.raw {
            print!("{}", render_revision_raw(changes));
            return ExitCode::SUCCESS;
        }
        match self.format {
            OutputFormat::Human => {
                println!(
                    "{}",
                    render_revision_human(changes, localizer, styling_enabled())
                );
                ExitCode::SUCCESS
            }
            OutputFormat::Json => serialize_json(&RevisionDiffDocument::from(changes), localizer),
        }
    }

    /// Render semantic events through the selected human, raw or JSON contract.
    fn render_semantic(
        &self,
        changes: &SemanticDiff,
        revisions: Option<&RevisionProjectDiff>,
        localizer: &Localizer,
    ) -> ExitCode {
        if self.raw {
            print!("{}", render_semantic_raw(changes));
            return ExitCode::SUCCESS;
        }
        match self.format {
            OutputFormat::Human => {
                println!(
                    "{}",
                    render_semantic_human(changes, localizer, styling_enabled())
                );
                ExitCode::SUCCESS
            }
            OutputFormat::Json => {
                serialize_json(&SemanticDiffDocument::new(changes, revisions), localizer)
            }
        }
    }

    fn render_workspace_group(
        &self,
        changes: &WorkspaceDiff,
        selected: &[&WorkspaceMember],
        localizer: &Localizer,
    ) -> ExitCode {
        if self.semantic {
            let Some(semantic) = analyze_workspace_group(changes, selected, localizer) else {
                return ExitCode::FAILURE;
            };
            return render_workspace_semantic(
                self,
                &semantic,
                changes
                    .workspace_files
                    .as_ref()
                    .map(WorkspaceFileDiff::Current),
                None,
                localizer,
            );
        }
        if self.raw {
            print!("{}", render_workspace_raw(changes));
            return ExitCode::SUCCESS;
        }
        match self.format {
            OutputFormat::Human => {
                println!(
                    "{}",
                    render_workspace_human(changes, localizer, styling_enabled())
                );
                ExitCode::SUCCESS
            }
            OutputFormat::Json => serialize_json(&WorkspaceDiffDocument::from(changes), localizer),
        }
    }

    fn render_workspace_revisions(
        &self,
        changes: &WorkspaceRevisionDiff,
        selected: &[&WorkspaceMember],
        localizer: &Localizer,
    ) -> ExitCode {
        if self.semantic {
            let Some(semantic) = analyze_workspace_revision_group(changes, selected, localizer)
            else {
                return ExitCode::FAILURE;
            };
            return render_workspace_semantic(
                self,
                &semantic,
                changes
                    .workspace_files
                    .as_ref()
                    .map(WorkspaceFileDiff::Revisions),
                Some(&changes.comparison),
                localizer,
            );
        }
        if self.raw {
            print!("{}", render_workspace_revision_raw(changes));
            return ExitCode::SUCCESS;
        }
        match self.format {
            OutputFormat::Human => {
                println!(
                    "{}",
                    render_workspace_revision_human(changes, localizer, styling_enabled())
                );
                ExitCode::SUCCESS
            }
            OutputFormat::Json => {
                serialize_json(&WorkspaceRevisionDiffDocument::from(changes), localizer)
            }
        }
    }
}

struct NamedSemanticDiff {
    name: ProjectName,
    diff: SemanticDiff,
}

#[derive(Clone, Copy)]
enum WorkspaceFileDiff<'a> {
    Current(&'a ProjectDiff),
    Revisions(&'a RevisionProjectDiff),
}

/// Discover the current logical model and analyze workspace snapshot pairs.
fn analyze_workspace_semantics(
    project: &crate::project::Project,
    changes: &ProjectDiff,
    localizer: &Localizer,
) -> Option<SemanticDiff> {
    let objects = object_model::discover(project).ok().or_else(|| {
        eprintln!("{}", localizer.text("diff-semantic-error"));
        None
    })?;
    semantic::diff_workspace(project, &objects, changes)
        .ok()
        .or_else(|| {
            eprintln!("{}", localizer.text("diff-semantic-error"));
            None
        })
}

/// Analyze committed tree blob pairs independently of the current worktree contents.
fn analyze_revision_semantics(
    project: &crate::project::Project,
    changes: &RevisionProjectDiff,
    localizer: &Localizer,
) -> Option<SemanticDiff> {
    semantic::diff_revisions(project, changes).ok().or_else(|| {
        eprintln!("{}", localizer.text("diff-semantic-error"));
        None
    })
}

fn analyze_workspace_group(
    changes: &WorkspaceDiff,
    selected: &[&WorkspaceMember],
    localizer: &Localizer,
) -> Option<Vec<NamedSemanticDiff>> {
    changes
        .projects
        .iter()
        .zip(selected)
        .map(|(changes, member)| {
            analyze_workspace_semantics(member.project(), &changes.diff, localizer).map(|diff| {
                NamedSemanticDiff {
                    name: changes.name.clone(),
                    diff,
                }
            })
        })
        .collect()
}

fn analyze_workspace_revision_group(
    changes: &WorkspaceRevisionDiff,
    selected: &[&WorkspaceMember],
    localizer: &Localizer,
) -> Option<Vec<NamedSemanticDiff>> {
    changes
        .projects
        .iter()
        .zip(selected)
        .map(|(changes, member)| {
            analyze_revision_semantics(member.project(), &changes.diff, localizer).map(|diff| {
                NamedSemanticDiff {
                    name: changes.name.clone(),
                    diff,
                }
            })
        })
        .collect()
}

fn render_workspace_human(changes: &WorkspaceDiff, localizer: &Localizer, styled: bool) -> String {
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

fn render_workspace_revision_human(
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

fn workspace_project_heading(name: &str, localizer: &Localizer, styled: bool) -> String {
    style_header(
        &localizer.format(
            "diff-workspace-project",
            &[("name", LocalizationValue::Text(name))],
        ),
        styled,
    )
}

fn render_workspace_raw(changes: &WorkspaceDiff) -> String {
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

fn render_workspace_revision_raw(changes: &WorkspaceRevisionDiff) -> String {
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

fn render_workspace_semantic(
    arguments: &DiffArgs,
    semantic: &[NamedSemanticDiff],
    workspace_files: Option<WorkspaceFileDiff<'_>>,
    comparison: Option<&diff::RevisionComparison>,
    localizer: &Localizer,
) -> ExitCode {
    if arguments.raw {
        print!(
            "{}",
            render_workspace_semantic_raw(semantic, workspace_files)
        );
        return ExitCode::SUCCESS;
    }
    match arguments.format {
        OutputFormat::Human => println!(
            "{}",
            render_workspace_semantic_human(
                semantic,
                workspace_files,
                localizer,
                styling_enabled(),
            )
        ),
        OutputFormat::Json => {
            return serialize_json(
                &WorkspaceSemanticDiffDocument::new(semantic, workspace_files, comparison),
                localizer,
            );
        }
    }
    ExitCode::SUCCESS
}

fn render_workspace_semantic_raw(
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

fn render_workspace_semantic_human(
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

/// Serialize one locale-independent document and report the shared failure.
fn serialize_json(document: &impl Serialize, localizer: &Localizer) -> ExitCode {
    serde_json::to_string_pretty(document).map_or_else(
        |_| {
            eprintln!("{}", localizer.text("diff-json-error"));
            ExitCode::FAILURE
        },
        |json| {
            println!("{json}");
            ExitCode::SUCCESS
        },
    )
}

/// Apply localized help text after clap has parsed the bootstrap locale.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("diff-about"))
        .override_usage(localizer.text("diff-usage"))
        .mut_arg("revisions", |argument| {
            argument
                .help(localizer.text("diff-revisions-help"))
                .value_name(localizer.text("diff-revisions-value"))
        })
        .mut_arg("since_branch_point", |argument| {
            argument.help(localizer.text("diff-since-branch-point-help"))
        })
        .mut_arg("raw", |argument| {
            argument.help(localizer.text("diff-raw-help"))
        })
        .mut_arg("semantic", |argument| {
            argument.help(localizer.text("diff-semantic-help"))
        })
        .mut_arg("format", |argument| {
            argument
                .help(localizer.text("diff-format-help"))
                .value_name(localizer.text("diff-format-value"))
        })
        .mut_arg("project", |argument| {
            argument
                .help(localizer.text("diff-project-help"))
                .value_name(localizer.text("diff-project-value"))
        })
        .mut_arg("workspace", |argument| {
            argument.help(localizer.text("diff-workspace-help"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}

/// Localize a structured diff error without leaking dependency diagnostics.
fn present_error(error: &DiffError, localizer: &Localizer) -> String {
    match error {
        DiffError::Repository(_) => localizer.text("diff-repository-error"),
        DiffError::Revision { revision, .. } => localizer.format(
            "diff-revision-error",
            &[("revision", LocalizationValue::Text(revision))],
        ),
        DiffError::MergeBase { from, to, .. } => localizer.format(
            "diff-merge-base-error",
            &[
                ("from", LocalizationValue::Text(from)),
                ("to", LocalizationValue::Text(to)),
            ],
        ),
        DiffError::ProjectOutsideRepository { .. } => {
            localizer.text("diff-project-outside-repository")
        }
    }
}

/// Print one localized diff error and return the standard runtime failure code.
fn report_error(error: &DiffError, localizer: &Localizer) -> ExitCode {
    eprintln!("{}", present_error(error, localizer));
    ExitCode::FAILURE
}

/// Render readable one-line descriptions while retaining index/worktree distinctions.
fn render_human(diff: &ProjectDiff, localizer: &Localizer, styled: bool) -> String {
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
fn render_revision_human(
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

fn style_header(header: &str, styled: bool) -> String {
    if styled {
        format!("\x1b[1m{header}\x1b[0m")
    } else {
        header.to_owned()
    }
}

fn style_metadata_group(group: &str, styled: bool) -> String {
    if styled {
        format!("\x1b[1;36m{group}\x1b[0m")
    } else {
        group.to_owned()
    }
}

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

fn styling_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Render a compact locale-independent two-column representation.
fn render_raw(diff: &ProjectDiff) -> String {
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
fn render_revision_raw(diff: &RevisionProjectDiff) -> String {
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
const fn raw_code(change: Option<Change>) -> char {
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

/// Render one stable tab-separated semantic event per line.
fn render_semantic_raw(diff: &SemanticDiff) -> String {
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

/// Render localized semantic event labels while retaining stable object identities.
fn render_semantic_human(diff: &SemanticDiff, localizer: &Localizer, styled: bool) -> String {
    if diff.is_empty() {
        return localizer.text("diff-semantic-clean");
    }
    let mut groups: std::collections::BTreeMap<
        String,
        std::collections::BTreeSet<SemanticHumanChange>,
    > = std::collections::BTreeMap::new();
    for event in diff.events() {
        let member = event
            .member()
            .map(|name| format!(" — {name}"))
            .unwrap_or_default();
        let group = semantic_object_group(event.object().id());
        groups
            .entry(metadata_kind(group, localizer))
            .or_default()
            .insert(SemanticHumanChange {
                target: format!(
                    "{}{}",
                    render_semantic_object(event.object().id(), localizer),
                    member
                ),
                kind: event.kind(),
                stage: event.stage(),
            });
    }
    let mut lines = vec![style_header(
        &localizer.text("diff-semantic-events"),
        styled,
    )];
    for (group, changes) in groups {
        lines.push(String::new());
        lines.push(style_metadata_group(&format!("{group}:"), styled));
        append_semantic_event_groups(&mut lines, changes.into_iter().collect(), localizer, styled);
    }
    lines.join("\n")
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SemanticHumanChange {
    target: String,
    kind: SemanticEventKind,
    stage: semantic::ChangeStage,
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
            .then_with(|| left.target.cmp(&right.target))
    });
    let mut start = 0;
    while start < changes.len() {
        let kind = changes[start].kind;
        let stage = changes[start].stage;
        let end = changes[start..]
            .iter()
            .position(|candidate| candidate.kind != kind || candidate.stage != stage)
            .map_or(changes.len(), |offset| start + offset);
        let title = format!(
            "{} — {}",
            localizer.text(semantic_event_key(kind)),
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
        semantic_event_kind_rank(change.kind),
        semantic_stage_rank(change.stage),
    )
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
        | SemanticEventKind::MethodRemoved => 3,
        SemanticEventKind::FunctionChanged
        | SemanticEventKind::FunctionAdded
        | SemanticEventKind::FunctionRemoved => 4,
        SemanticEventKind::FormChanged => 5,
    }
}

/// Keep workspace edges in index-to-worktree order; revisions form their own subgroup.
const fn semantic_stage_rank(stage: semantic::ChangeStage) -> u8 {
    match stage {
        semantic::ChangeStage::Index => 0,
        semantic::ChangeStage::Worktree => 1,
        semantic::ChangeStage::Revision => 2,
    }
}

/// Select the localized comparison-edge label used in a subgroup heading.
const fn semantic_stage_key(stage: semantic::ChangeStage) -> &'static str {
    match stage {
        semantic::ChangeStage::Index => "diff-index",
        semantic::ChangeStage::Worktree => "diff-worktree",
        semantic::ChangeStage::Revision => "diff-semantic-revision",
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

#[derive(Serialize)]
struct SemanticDiffDocument<'a> {
    schema_version: u8,
    kind: &'static str,
    comparison: SemanticComparisonDocument<'a>,
    events: Vec<SemanticEventDocument>,
}

#[derive(Serialize)]
struct WorkspaceSemanticDiffDocument<'a> {
    schema_version: u8,
    kind: &'static str,
    comparison: SemanticComparisonDocument<'a>,
    projects: Vec<WorkspaceSemanticProjectDocument>,
    workspace_files: Option<WorkspaceSemanticFilesDocument>,
}

#[derive(Serialize)]
struct WorkspaceSemanticProjectDocument {
    name: String,
    events: Vec<SemanticEventDocument>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WorkspaceSemanticFilesDocument {
    Workspace { files: Vec<FileDocument> },
    Revisions { files: Vec<RevisionFileDocument> },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SemanticComparisonDocument<'a> {
    Workspace,
    Revisions {
        strategy: &'static str,
        from: RevisionEndpointDocument<'a>,
        to: RevisionEndpointDocument<'a>,
        merge_base_commit: Option<String>,
    },
}

#[derive(Serialize)]
struct SemanticEventDocument {
    kind: &'static str,
    stage: &'static str,
    object: SemanticObjectDocument,
    member: Option<String>,
    path: String,
    path_encoding: &'static str,
}

#[derive(Serialize)]
struct SemanticObjectDocument {
    id: String,
    metadata_type: &'static str,
    name: String,
}

impl<'a> SemanticDiffDocument<'a> {
    /// Build semantic schema version 3 without locale-dependent values.
    fn new(diff: &SemanticDiff, revisions: Option<&'a RevisionProjectDiff>) -> Self {
        let comparison = revisions.map_or(SemanticComparisonDocument::Workspace, |diff| {
            let comparison = &diff.comparison;
            SemanticComparisonDocument::Revisions {
                strategy: if comparison.merge_base_commit.is_some() {
                    "merge_base"
                } else {
                    "direct"
                },
                from: RevisionEndpointDocument {
                    revision: &comparison.from_revision,
                    commit: comparison.from_commit.to_string(),
                },
                to: RevisionEndpointDocument {
                    revision: &comparison.to_revision,
                    commit: comparison.to_commit.to_string(),
                },
                merge_base_commit: comparison.merge_base_commit.map(|id| id.to_string()),
            }
        });
        Self {
            schema_version: 3,
            kind: "semantic",
            comparison,
            events: diff
                .events()
                .iter()
                .map(SemanticEventDocument::from)
                .collect(),
        }
    }
}

impl<'a> WorkspaceSemanticDiffDocument<'a> {
    fn new(
        semantic: &[NamedSemanticDiff],
        workspace_files: Option<WorkspaceFileDiff<'_>>,
        comparison: Option<&'a diff::RevisionComparison>,
    ) -> Self {
        Self {
            schema_version: 3,
            kind: "semantic_workspace",
            comparison: semantic_comparison(comparison),
            projects: semantic
                .iter()
                .map(|project| WorkspaceSemanticProjectDocument {
                    name: project.name.as_str().to_owned(),
                    events: project
                        .diff
                        .events()
                        .iter()
                        .map(SemanticEventDocument::from)
                        .collect(),
                })
                .collect(),
            workspace_files: workspace_files.map(|files| match files {
                WorkspaceFileDiff::Current(files) => WorkspaceSemanticFilesDocument::Workspace {
                    files: file_documents(files),
                },
                WorkspaceFileDiff::Revisions(files) => WorkspaceSemanticFilesDocument::Revisions {
                    files: revision_file_documents(files),
                },
            }),
        }
    }
}

fn semantic_comparison(
    comparison: Option<&diff::RevisionComparison>,
) -> SemanticComparisonDocument<'_> {
    comparison.map_or(SemanticComparisonDocument::Workspace, |comparison| {
        SemanticComparisonDocument::Revisions {
            strategy: if comparison.merge_base_commit.is_some() {
                "merge_base"
            } else {
                "direct"
            },
            from: RevisionEndpointDocument {
                revision: &comparison.from_revision,
                commit: comparison.from_commit.to_string(),
            },
            to: RevisionEndpointDocument {
                revision: &comparison.to_revision,
                commit: comparison.to_commit.to_string(),
            },
            merge_base_commit: comparison.merge_base_commit.map(|id| id.to_string()),
        }
    })
}

impl From<&SemanticEvent> for SemanticEventDocument {
    /// Preserve stable identities, event names and arbitrary Git path bytes.
    fn from(event: &SemanticEvent) -> Self {
        let (path, path_encoding) = json_path(event.path());
        Self {
            kind: event.kind().as_str(),
            stage: event.stage().as_str(),
            object: SemanticObjectDocument {
                id: event.object().id().to_owned(),
                metadata_type: event.object().metadata_type(),
                name: event.object().name().to_owned(),
            },
            member: event.member().map(str::to_owned),
            path,
            path_encoding,
        }
    }
}

#[derive(Serialize)]
struct DiffDocument {
    schema_version: u8,
    files: Vec<FileDocument>,
}

#[derive(Serialize)]
struct WorkspaceDiffDocument {
    schema_version: u8,
    projects: Vec<WorkspaceFileProjectDocument>,
    workspace_files: Option<Vec<FileDocument>>,
}

#[derive(Serialize)]
struct WorkspaceFileProjectDocument {
    name: String,
    files: Vec<FileDocument>,
}

#[derive(Serialize)]
struct FileDocument {
    path: String,
    path_encoding: &'static str,
    index: Option<&'static str>,
    worktree: Option<&'static str>,
}

impl From<&ProjectDiff> for DiffDocument {
    /// Build schema version 1 without locale-dependent values.
    fn from(diff: &ProjectDiff) -> Self {
        Self {
            schema_version: 1,
            files: file_documents(diff),
        }
    }
}

impl From<&WorkspaceDiff> for WorkspaceDiffDocument {
    fn from(diff: &WorkspaceDiff) -> Self {
        Self {
            schema_version: 1,
            projects: diff
                .projects
                .iter()
                .map(|project| WorkspaceFileProjectDocument {
                    name: project.name.as_str().to_owned(),
                    files: file_documents(&project.diff),
                })
                .collect(),
            workspace_files: diff.workspace_files.as_ref().map(file_documents),
        }
    }
}

fn file_documents(diff: &ProjectDiff) -> Vec<FileDocument> {
    diff.files
        .iter()
        .map(|file| {
            let (path, path_encoding) = json_path(file.path.as_bstr());
            FileDocument {
                path,
                path_encoding,
                index: file.index.map(change_name),
                worktree: file.worktree.map(change_name),
            }
        })
        .collect()
}

#[derive(Serialize)]
struct RevisionDiffDocument<'a> {
    schema_version: u8,
    comparison: RevisionComparisonDocument<'a>,
    files: Vec<RevisionFileDocument>,
}

#[derive(Serialize)]
struct WorkspaceRevisionDiffDocument<'a> {
    schema_version: u8,
    comparison: RevisionComparisonDocument<'a>,
    projects: Vec<WorkspaceRevisionProjectDocument>,
    workspace_files: Option<Vec<RevisionFileDocument>>,
}

#[derive(Serialize)]
struct WorkspaceRevisionProjectDocument {
    name: String,
    files: Vec<RevisionFileDocument>,
}

#[derive(Serialize)]
struct RevisionComparisonDocument<'a> {
    kind: &'static str,
    strategy: &'static str,
    from: RevisionEndpointDocument<'a>,
    to: RevisionEndpointDocument<'a>,
    merge_base_commit: Option<String>,
}

#[derive(Serialize)]
struct RevisionEndpointDocument<'a> {
    revision: &'a str,
    commit: String,
}

#[derive(Serialize)]
struct RevisionFileDocument {
    path: String,
    path_encoding: &'static str,
    change: &'static str,
}

impl<'a> From<&'a RevisionProjectDiff> for RevisionDiffDocument<'a> {
    /// Build the explicit schema version 2 used only for revision comparisons.
    fn from(diff: &'a RevisionProjectDiff) -> Self {
        let comparison = &diff.comparison;
        Self {
            schema_version: 2,
            comparison: revision_comparison_document(comparison),
            files: revision_file_documents(diff),
        }
    }
}

impl<'a> From<&'a WorkspaceRevisionDiff> for WorkspaceRevisionDiffDocument<'a> {
    fn from(diff: &'a WorkspaceRevisionDiff) -> Self {
        Self {
            schema_version: 2,
            comparison: revision_comparison_document(&diff.comparison),
            projects: diff
                .projects
                .iter()
                .map(|project| WorkspaceRevisionProjectDocument {
                    name: project.name.as_str().to_owned(),
                    files: revision_file_documents(&project.diff),
                })
                .collect(),
            workspace_files: diff.workspace_files.as_ref().map(revision_file_documents),
        }
    }
}

fn revision_comparison_document(
    comparison: &diff::RevisionComparison,
) -> RevisionComparisonDocument<'_> {
    RevisionComparisonDocument {
        kind: "revisions",
        strategy: if comparison.merge_base_commit.is_some() {
            "merge-base"
        } else {
            "direct"
        },
        from: RevisionEndpointDocument {
            revision: &comparison.from_revision,
            commit: comparison.from_commit.to_string(),
        },
        to: RevisionEndpointDocument {
            revision: &comparison.to_revision,
            commit: comparison.to_commit.to_string(),
        },
        merge_base_commit: comparison.merge_base_commit.map(|id| id.to_string()),
    }
}

fn revision_file_documents(diff: &RevisionProjectDiff) -> Vec<RevisionFileDocument> {
    diff.files
        .iter()
        .map(|file| {
            let (path, path_encoding) = json_path(file.path.as_bstr());
            RevisionFileDocument {
                path,
                path_encoding,
                change: change_name(file.change),
            }
        })
        .collect()
}

/// Keep valid UTF-8 paths exact and encode arbitrary Git bytes without data loss.
fn json_path(path: &BStr) -> (String, &'static str) {
    path.to_str().map_or_else(
        |_| (percent_encode(path), "percent"),
        |path| (path.to_owned(), "utf-8"),
    )
}

/// Percent-encode every byte so a non-UTF-8 path remains reversible.
fn percent_encode(path: &BStr) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(path.len() * 3);
    for byte in path.as_bytes() {
        write!(encoded, "%{byte:02X}").expect("writing to String cannot fail");
    }
    encoded
}

/// Map a state to its stable JSON value.
const fn change_name(change: Change) -> &'static str {
    match change {
        Change::Added => "added",
        Change::Modified => "modified",
        Change::Deleted => "deleted",
        Change::TypeChanged => "type_changed",
        Change::Untracked => "untracked",
        Change::IntentToAdd => "intent_to_add",
        Change::Conflict => "conflict",
    }
}

#[cfg(test)]
mod tests {
    use gix::bstr::{BString, ByteSlice};

    use super::{
        HumanState, SemanticHumanChange, append_semantic_event_groups, change_marker, change_name,
        human_state_title, json_path, raw_code, render_human, render_raw,
    };
    use crate::{
        cli::localization::{Locale, Localizer},
        project::{
            ProjectType,
            diff::{DisplayChange, DisplayTarget, FileChange, ProjectDiff},
            metadata,
            semantic::{ChangeStage, SemanticEventKind},
        },
        vcs::status::Change,
    };

    /// Machine names and raw codes are exhaustive and locale-independent.
    #[test]
    fn stable_change_representations_cover_every_state() {
        let values = [
            (Change::Added, "added", 'A', '+'),
            (Change::Modified, "modified", 'M', '✎'),
            (Change::Deleted, "deleted", 'D', '−'),
            (Change::TypeChanged, "type_changed", 'T', '↔'),
            (Change::Untracked, "untracked", '?', '?'),
            (Change::IntentToAdd, "intent_to_add", 'I', '◌'),
            (Change::Conflict, "conflict", 'U', '!'),
        ];
        for (change, name, code, marker) in values {
            assert_eq!(change_name(change), name);
            assert_eq!(raw_code(Some(change)), code);
            assert_eq!(change_marker(change), marker);
        }
        assert_eq!(raw_code(None), '.');
    }

    /// Raw output keeps both comparison stages and deterministic path order.
    #[test]
    fn raw_output_has_two_state_columns() {
        let diff = ProjectDiff {
            files: vec![FileChange {
                path: BString::from("src/module.bsl"),
                index: Some(Change::Modified),
                worktree: Some(Change::Deleted),
            }],
            display: Vec::new(),
        };
        assert_eq!(render_raw(&diff), "MD\tsrc/module.bsl\n");
    }

    /// Workspace groups retain both stages while using the latest state marker.
    #[test]
    fn workspace_state_heading_describes_both_stages() {
        let state = HumanState::IndexAndWorktree {
            index: Change::Added,
            worktree: Change::Modified,
        };
        for (locale, expected) in [
            (Locale::RuRu, "Добавлены — индекс; Изменены — рабочая копия"),
            (Locale::EnUs, "Added — index; Modified — working tree"),
        ] {
            let localizer = Localizer::try_new(locale).unwrap();
            assert_eq!(human_state_title(state, &localizer), expected);
        }
        assert_eq!(change_marker(super::marker_change(state)), '✎');
    }

    /// JSON retains arbitrary Git bytes with an explicit reversible encoding.
    #[test]
    fn path_encodings_are_unambiguous() {
        assert_eq!(
            json_path("src/модуль.bsl".as_bytes().as_bstr()),
            ("src/модуль.bsl".into(), "utf-8")
        );
        assert_eq!(
            json_path(b"raw-\xff".as_bstr()),
            ("%72%61%77%2D%FF".into(), "percent")
        );
    }

    /// Human output localizes Configurator identities, groups them and keeps other files last.
    #[test]
    fn human_output_groups_logical_metadata() {
        let catalog = metadata::from_path(
            ProjectType::Configuration,
            "Catalogs/Контрагенты.xml".as_bytes().as_bstr(),
        )
        .unwrap()
        .with_suffix(&[metadata::MetadataPart {
            kind: "attribute",
            name: Some("Реквизит1".to_owned()),
        }]);
        let diff = ProjectDiff {
            files: Vec::new(),
            display: vec![
                DisplayChange {
                    target: DisplayTarget::Metadata(catalog),
                    index: None,
                    worktree: Some(Change::Modified),
                },
                DisplayChange {
                    target: DisplayTarget::File("notes.txt".into()),
                    index: None,
                    worktree: Some(Change::Modified),
                },
            ],
        };

        for (locale, logical, other) in [
            (
                Locale::RuRu,
                "Справочник.Контрагенты.Реквизит.Реквизит1",
                "Прочие файлы",
            ),
            (
                Locale::EnUs,
                "Catalog.Контрагенты.Attribute.Реквизит1",
                "Other files",
            ),
        ] {
            let localizer = Localizer::try_new(locale).unwrap();
            let output = render_human(&diff, &localizer, false);
            assert!(output.contains(logical), "{output}");
            assert!(output.find(logical).unwrap() < output.find(other).unwrap());
            assert!(output.contains(&format!("    ✎ {logical}")), "{output}");
            assert!(!output.contains("\x1b["), "{output:?}");

            let styled = render_human(&diff, &localizer, true);
            assert!(styled.contains("\x1b[1;36m"), "{styled:?}");
            assert!(styled.contains("\x1b[1;33m✎\x1b[0m"), "{styled:?}");
            assert!(styled.contains(logical), "{styled}");
        }
    }

    /// Semantic groups reuse localized headings, counts, markers and the TTY color palette.
    #[test]
    fn semantic_groups_match_file_diff_presentation() {
        let localizer = Localizer::try_new(Locale::RuRu).unwrap();
        let changes = vec![
            SemanticHumanChange {
                target: "ОбщийМодуль.Обмен — Выполнить".to_owned(),
                kind: SemanticEventKind::MethodChanged,
                stage: ChangeStage::Worktree,
            },
            SemanticHumanChange {
                target: "ОбщийМодуль.Новый".to_owned(),
                kind: SemanticEventKind::ObjectAdded,
                stage: ChangeStage::Index,
            },
        ];
        let mut plain = Vec::new();
        append_semantic_event_groups(&mut plain, changes, &localizer, false);
        let plain = plain.join("\n");
        assert!(
            plain.contains(
                "Изменена процедура — рабочая копия (1):\n    ✎ ОбщийМодуль.Обмен — Выполнить"
            ),
            "{plain}"
        );
        assert!(
            plain.contains("Добавлен объект — индекс (1):\n    + ОбщийМодуль.Новый"),
            "{plain}"
        );
        assert!(!plain.contains("\x1b["), "{plain:?}");

        let mut styled = Vec::new();
        append_semantic_event_groups(
            &mut styled,
            vec![SemanticHumanChange {
                target: "ОбщийМодуль.Обмен — Выполнить".to_owned(),
                kind: SemanticEventKind::MethodChanged,
                stage: ChangeStage::Worktree,
            }],
            &localizer,
            true,
        );
        let styled = styled.join("\n");
        assert!(styled.contains("\x1b[1;33m"), "{styled:?}");
        assert!(styled.contains("\x1b[1;33m✎\x1b[0m"), "{styled:?}");
    }
}
