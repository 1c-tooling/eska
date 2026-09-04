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
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        diff::{self, DiffError, DisplayTarget, ProjectDiff, RevisionProjectDiff},
        discovery,
        metadata::MetadataPath,
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

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
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
        let project = match discovery::discover(project_dir) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("{}", diagnostics::present_project_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        if self.revisions.is_empty() {
            let changes = match diff::inspect(&project) {
                Ok(changes) => changes,
                Err(error) => return report_error(&error, localizer),
            };
            self.render_workspace(&changes, localizer)
        } else {
            let from = &self.revisions[0];
            let to = self.revisions.get(1).map_or("HEAD", String::as_str);
            let changes = match diff::compare(&project, from, to, self.since_branch_point) {
                Ok(changes) => changes,
                Err(error) => return report_error(&error, localizer),
            };
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
        .mut_arg("format", |argument| {
            argument
                .help(localizer.text("diff-format-help"))
                .value_name(localizer.text("diff-format-value"))
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

/// Render a logical metadata identity in Configurator notation.
fn render_metadata_path(path: &MetadataPath, localizer: &Localizer) -> String {
    path.parts
        .iter()
        .map(|part| {
            let kind = metadata_kind(part.kind, localizer);
            part.name
                .as_ref()
                .map_or_else(|| kind.clone(), |name| format!("{kind}.{name}"))
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Resolve the localized Configurator name of a stable metadata kind.
fn metadata_kind(kind: &str, localizer: &Localizer) -> String {
    localizer.text(&format!("diff-metadata-{kind}"))
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

#[derive(Serialize)]
struct DiffDocument {
    schema_version: u8,
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
            files: diff
                .files
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
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct RevisionDiffDocument<'a> {
    schema_version: u8,
    comparison: RevisionComparisonDocument<'a>,
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
            comparison: RevisionComparisonDocument {
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
            },
            files: diff
                .files
                .iter()
                .map(|file| {
                    let (path, path_encoding) = json_path(file.path.as_bstr());
                    RevisionFileDocument {
                        path,
                        path_encoding,
                        change: change_name(file.change),
                    }
                })
                .collect(),
        }
    }
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

/// Quote paths only when control characters, quotes or arbitrary bytes require escaping.
fn display_path(path: &BStr) -> String {
    if let Ok(path) = path.to_str()
        && path
            .chars()
            .all(|character| !character.is_control() && character != '\\' && character != '"')
    {
        return path.to_owned();
    }

    let mut escaped = String::with_capacity(path.len() + 2);
    escaped.push('"');
    for byte in path.as_bytes() {
        match byte {
            b'\\' => escaped.push_str("\\\\"),
            b'"' => escaped.push_str("\\\""),
            b'\n' => escaped.push_str("\\n"),
            b'\r' => escaped.push_str("\\r"),
            b'\t' => escaped.push_str("\\t"),
            0x20..=0x7e => escaped.push(char::from(*byte)),
            _ => {
                use std::fmt::Write as _;
                write!(escaped, "\\x{byte:02X}").expect("writing to String cannot fail");
            }
        }
    }
    escaped.push('"');
    escaped
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
        HumanState, change_marker, change_name, display_path, human_state_title, json_path,
        raw_code, render_human, render_raw,
    };
    use crate::{
        cli::localization::{Locale, Localizer},
        project::{
            ProjectType,
            diff::{DisplayChange, DisplayTarget, FileChange, ProjectDiff},
            metadata,
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

    /// Path presentation remains one-line and JSON retains arbitrary Git bytes.
    #[test]
    fn path_encodings_are_unambiguous() {
        assert_eq!(
            display_path(b"src/line\nname".as_bstr()),
            "\"src/line\\nname\""
        );
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
}
