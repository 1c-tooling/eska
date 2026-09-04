//! Localized human output and stable machine representations of project file changes.

use std::{path::Path, process::ExitCode};

use clap::{Args, ValueEnum};
use gix::bstr::{BStr, ByteSlice};
use serde::Serialize;

use crate::{
    cli::{diagnostics, localization::Localizer},
    project::{
        diff::{self, DiffError, FileChange, ProjectDiff},
        discovery,
    },
    vcs::status::Change,
};

#[derive(Debug, Args)]
pub(in crate::cli) struct DiffArgs {
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
        let changes = match diff::inspect(&project) {
            Ok(changes) => changes,
            Err(error) => {
                eprintln!("{}", present_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };

        if self.raw {
            print!("{}", render_raw(&changes));
        } else {
            match self.format {
                OutputFormat::Human => println!("{}", render_human(&changes, localizer)),
                OutputFormat::Json => {
                    let Ok(json) = serde_json::to_string_pretty(&DiffDocument::from(&changes))
                    else {
                        eprintln!("{}", localizer.text("diff-json-error"));
                        return ExitCode::FAILURE;
                    };
                    println!("{json}");
                }
            }
        }
        ExitCode::SUCCESS
    }
}

/// Apply localized help text after clap has parsed the bootstrap locale.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("diff-about"))
        .override_usage(localizer.text("diff-usage"))
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
    localizer.text(match error {
        DiffError::Repository(_) => "diff-repository-error",
        DiffError::ProjectOutsideRepository { .. } => "diff-project-outside-repository",
    })
}

/// Render readable one-line descriptions while retaining index/worktree distinctions.
fn render_human(diff: &ProjectDiff, localizer: &Localizer) -> String {
    if diff.files.is_empty() {
        return localizer.text("diff-clean");
    }
    let mut lines = vec![localizer.text("diff-files")];
    lines.extend(diff.files.iter().map(|file| {
        format!(
            "  {}  {}",
            human_states(file, localizer),
            display_path(file.path.as_bstr())
        )
    }));
    lines.join("\n")
}

/// Describe every populated comparison stage for one path.
fn human_states(file: &FileChange, localizer: &Localizer) -> String {
    let mut states = Vec::with_capacity(2);
    if let Some(change) = file.index {
        states.push(format!(
            "{} ({})",
            localizer.text(change_key(change)),
            localizer.text("diff-index")
        ));
    }
    if let Some(change) = file.worktree {
        states.push(format!(
            "{} ({})",
            localizer.text(change_key(change)),
            localizer.text("diff-worktree")
        ));
    }
    states.join(", ")
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

    use super::{change_name, display_path, json_path, raw_code, render_raw};
    use crate::{
        project::diff::{FileChange, ProjectDiff},
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
}
