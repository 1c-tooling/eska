//! Localized human output and stable machine representations of project file changes.

use std::{path::Path, process::ExitCode};

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
                println!("{}", render_human(changes, localizer));
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
                println!("{}", render_revision_human(changes, localizer));
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
fn render_human(diff: &ProjectDiff, localizer: &Localizer) -> String {
    if diff.display.is_empty() {
        return localizer.text("diff-clean");
    }
    render_grouped(
        localizer.text("diff-files"),
        diff.display.iter().map(|change| {
            (
                &change.target,
                human_states(change.index, change.worktree, localizer),
            )
        }),
        localizer,
    )
}

/// Render one committed comparison with explicit resolved endpoints.
fn render_revision_human(diff: &RevisionProjectDiff, localizer: &Localizer) -> String {
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
        header,
        diff.display
            .iter()
            .map(|change| (&change.target, localizer.text(change_key(change.change)))),
        localizer,
    )
}

/// Group already-described logical changes by localized metadata type.
fn render_grouped<'a>(
    header: String,
    changes: impl IntoIterator<Item = (&'a DisplayTarget, String)>,
    localizer: &Localizer,
) -> String {
    let mut lines = vec![header];
    let mut groups: std::collections::BTreeMap<String, Vec<(String, String)>> =
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
    for (group, mut changes) in groups {
        changes.sort_by(|left, right| left.0.cmp(&right.0));
        lines.push(String::new());
        lines.push(format!("{group}:"));
        lines.extend(
            changes
                .into_iter()
                .map(|(target, state)| format!("  {state}  {target}")),
        );
    }
    if !other_files.is_empty() {
        other_files.sort_by(|left, right| left.0.cmp(&right.0));
        lines.push(String::new());
        lines.push(format!("{}:", localizer.text("diff-other-files")));
        lines.extend(
            other_files
                .into_iter()
                .map(|(target, state)| format!("  {state}  {target}")),
        );
    }
    lines.join("\n")
}

/// Keep human commit labels compact while JSON retains complete object IDs.
fn short_id(id: &str) -> &str {
    &id[..id.len().min(7)]
}

/// Describe every populated comparison stage for one path.
fn human_states(index: Option<Change>, worktree: Option<Change>, localizer: &Localizer) -> String {
    let mut states = Vec::with_capacity(2);
    if let Some(change) = index {
        states.push(format!(
            "{} ({})",
            localizer.text(change_key(change)),
            localizer.text("diff-index")
        ));
    }
    if let Some(change) = worktree {
        states.push(format!(
            "{} ({})",
            localizer.text(change_key(change)),
            localizer.text("diff-worktree")
        ));
    }
    states.join(", ")
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

/// Select the shared localized name of a file state.
const fn change_key(change: Change) -> &'static str {
    match change {
        Change::Added => "diff-added",
        Change::Modified => "diff-modified",
        Change::Deleted => "diff-deleted",
        Change::TypeChanged => "diff-type-changed",
        Change::Untracked => "diff-untracked",
        Change::IntentToAdd => "diff-intent-to-add",
        Change::Conflict => "diff-conflict",
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

    use super::{change_name, display_path, json_path, raw_code, render_human, render_raw};
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
            (Change::Added, "added", 'A'),
            (Change::Modified, "modified", 'M'),
            (Change::Deleted, "deleted", 'D'),
            (Change::TypeChanged, "type_changed", 'T'),
            (Change::Untracked, "untracked", '?'),
            (Change::IntentToAdd, "intent_to_add", 'I'),
            (Change::Conflict, "conflict", 'U'),
        ];
        for (change, name, code) in values {
            assert_eq!(change_name(change), name);
            assert_eq!(raw_code(Some(change)), code);
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
            let output = render_human(&diff, &localizer);
            assert!(output.contains(logical), "{output}");
            assert!(output.find(logical).unwrap() < output.find(other).unwrap());
        }
    }
}
