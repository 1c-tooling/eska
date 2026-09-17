//! Shared shelf presentation for explicit shelf commands and automatic task switching.

use crate::{
    cli::{
        changes::display_path,
        diagnostics,
        encoding::json_git_text,
        localization::{LocalizationValue, Localizer},
    },
    project::discovery::{self, DiscoveryContext},
    vcs::{
        repository::Repository,
        shelves::{Error, Shelf},
    },
};
use clap::ValueEnum;
use gix::bstr::ByteSlice;
use serde::Serialize;
use std::{path::Path, process::ExitCode};

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub(super) enum OutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Serialize)]
pub(super) struct ShelfDocument<'a> {
    id: Option<&'a str>,
    branch: String,
    branch_encoding: &'static str,
    base: &'a str,
    created_at: Option<u64>,
    files: Vec<PathDocument<'a>>,
}

#[derive(Serialize)]
struct PathDocument<'a> {
    path: String,
    path_encoding: &'static str,
    index: Option<&'a str>,
    worktree: Option<&'a str>,
}

impl<'a> From<&'a Shelf> for ShelfDocument<'a> {
    /// Preserve Git byte strings and original staged/unstaged classifications in JSON.
    fn from(saved: &'a Shelf) -> Self {
        let (branch, branch_encoding) = json_git_text(&saved.branch);
        Self {
            id: (!saved.id.is_empty()).then_some(saved.id.as_str()),
            branch,
            branch_encoding,
            base: &saved.base,
            created_at: (saved.created_at != 0).then_some(saved.created_at),
            files: saved
                .files
                .iter()
                .map(|entry| {
                    let (path, path_encoding) = json_git_text(&entry.path);
                    PathDocument {
                        path,
                        path_encoding,
                        index: entry.index.as_deref(),
                        worktree: entry.worktree.as_deref(),
                    }
                })
                .collect(),
        }
    }
}

/// Discover a standalone or workspace repository without restricting the shelf to a member.
pub(super) fn repository(
    start: &Path,
    format: OutputFormat,
    localizer: &Localizer,
) -> Result<Repository, ExitCode> {
    let context = discovery::discover_context(start).map_err(|error| {
        fail(
            format,
            "project-discovery",
            &diagnostics::present_context_error(&error, localizer),
        )
    })?;
    let root = match &context {
        DiscoveryContext::Standalone(project) => project.root(),
        DiscoveryContext::Workspace { workspace, .. } => workspace.root(),
    };
    Repository::discover(root)
        .map_err(|error| write_error(format, &Error::Repository(error), localizer))
}

/// Emit one successful explicit shelf operation using the selected output contract.
pub(super) fn write_result(
    format: OutputFormat,
    saved: &Shelf,
    restore: bool,
    preview: bool,
    localizer: &Localizer,
) -> ExitCode {
    if matches!(format, OutputFormat::Json) {
        return write_json(
            &serde_json::json!({
                "schema_version": 1, "operation": if restore {"unshelve"} else {"shelve"},
                "dry_run": preview, "shelf": ShelfDocument::from(saved),
            }),
            localizer,
        );
    }
    write_human(saved, restore, preview, localizer);
    ExitCode::SUCCESS
}

/// Show the shelf identity and its independent index/worktree changes without ANSI control bytes.
pub(super) fn write_human(saved: &Shelf, restore: bool, preview: bool, localizer: &Localizer) {
    let key = match (restore, preview) {
        (false, false) => "shelf-saved",
        (true, false) => "shelf-restored",
        (false, true) => "shelf-preview-save",
        (true, true) => "shelf-preview-restore",
    };
    let branch = display_path(saved.branch.as_bstr());
    println!(
        "{}",
        localizer.format(
            key,
            &[
                ("id", LocalizationValue::Text(&saved.id)),
                ("branch", LocalizationValue::Text(&branch))
            ]
        )
    );
    for entry in &saved.files {
        let path = display_path(entry.path.as_bstr());
        let index = change_text(entry.index.as_deref(), localizer);
        let worktree = change_text(entry.worktree.as_deref(), localizer);
        println!(
            "  {}",
            localizer.format(
                "shelf-path",
                &[
                    ("path", LocalizationValue::Text(&path)),
                    ("index", LocalizationValue::Text(&index)),
                    ("worktree", LocalizationValue::Text(&worktree))
                ]
            )
        );
    }
}

/// Present a stable shelf list, including all branch refs in this repository.
pub(super) fn write_list(format: OutputFormat, saved: &[Shelf], localizer: &Localizer) -> ExitCode {
    if matches!(format, OutputFormat::Json) {
        let documents: Vec<_> = saved.iter().map(ShelfDocument::from).collect();
        return write_json(
            &serde_json::json!({"schema_version": 1, "shelves": documents}),
            localizer,
        );
    }
    if saved.is_empty() {
        println!("{}", localizer.text("shelf-list-empty"));
    }
    for entry in saved {
        let branch = display_path(entry.branch.as_bstr());
        let created = gix::date::Time {
            seconds: i64::try_from(entry.created_at).unwrap_or(i64::MAX),
            offset: 0,
        }
        .format_or_unix(gix::date::time::format::ISO8601_STRICT);
        println!(
            "{}",
            localizer.format(
                "shelf-list-entry",
                &[
                    ("id", LocalizationValue::Text(&entry.id)),
                    ("branch", LocalizationValue::Text(&branch)),
                    ("created", LocalizationValue::Text(&created)),
                    (
                        "files",
                        LocalizationValue::Number(
                            i64::try_from(entry.files.len()).unwrap_or(i64::MAX)
                        )
                    ),
                ]
            )
        );
    }
    ExitCode::SUCCESS
}

/// Serialize a document without mixing localized diagnostics into stdout.
pub(super) fn write_json(value: &impl Serialize, localizer: &Localizer) -> ExitCode {
    serde_json::to_string_pretty(value).map_or_else(
        |_| {
            eprintln!("{}", localizer.text("shelf-json-error"));
            ExitCode::FAILURE
        },
        |text| {
            println!("{text}");
            ExitCode::SUCCESS
        },
    )
}

/// Emit a stable error envelope and retain human detail on stderr.
pub(super) fn fail(format: OutputFormat, code: &str, detail: &str) -> ExitCode {
    if matches!(format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::json!({"schema_version": 1, "status": "error", "error": {"code": code}})
        );
    }
    eprintln!("{detail}");
    ExitCode::FAILURE
}

/// Localize a structured shelf failure without exposing backend command terminology.
pub(super) fn write_error(format: OutputFormat, error: &Error, localizer: &Localizer) -> ExitCode {
    let code = error_code(error);
    let path = match error {
        Error::Collision(path) => path.display().to_string(),
        _ => String::new(),
    };
    let detail = localizer.format(
        &format!("shelf-error-{code}"),
        &[("path", LocalizationValue::Text(&path))],
    );
    fail(format, code, &detail)
}

/// Keep error identifiers stable and independent of locale.
pub(super) const fn error_code(error: &Error) -> &'static str {
    match error {
        Error::Repository(_) => "repository",
        Error::Command(_) => "command",
        Error::Io(_) => "io",
        Error::InvalidShelf => "invalid-shelf",
        Error::Locked => "locked",
        Error::Detached => "detached",
        Error::Unborn => "unborn",
        Error::InProgress => "in-progress",
        Error::UnsupportedIndex => "unsupported-index",
        Error::Empty => "empty",
        Error::Exists => "shelf-exists",
        Error::Missing => "shelf-missing",
        Error::WrongBranch => "wrong-branch",
        Error::MovedBranch => "moved-branch",
        Error::Dirty => "dirty",
        Error::Collision(_) => "collision",
        Error::Incomplete => "incomplete",
    }
}

/// Localize file states while retaining the serialized machine names unchanged.
fn change_text(value: Option<&str>, localizer: &Localizer) -> String {
    value.map_or_else(
        || "—".to_owned(),
        |value| localizer.text(&format!("shelf-change-{value}")),
    )
}
