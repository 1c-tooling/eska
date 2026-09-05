//! Localized human output and a stable JSON document for local commit history.

use std::{
    io::{self, IsTerminal},
    path::Path,
    process::ExitCode,
};

use clap::{Args, ValueEnum};
use gix::bstr::{BStr, ByteSlice};
use serde::Serialize;

use crate::{
    cli::{diagnostics, localization::Localizer},
    project::{
        discovery,
        history::{self, HistoryEntry, HistoryError},
    },
};

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 1_000;

#[derive(Debug, Args)]
pub(in crate::cli) struct HistoryArgs {
    #[arg(
        short = 'n',
        long,
        default_value_t = DEFAULT_LIMIT,
        value_parser = clap::builder::RangedU64ValueParser::<usize>::new()
            .range(1..=MAX_LIMIT as u64)
    )]
    limit: usize,

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

impl HistoryArgs {
    /// Discover the project, read local history and select one presentation.
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover(project_dir) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("{}", diagnostics::present_project_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let entries = match history::inspect(&project, self.limit) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("{}", present_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };

        match self.format {
            OutputFormat::Human => {
                println!("{}", render_human(&entries, localizer, styling_enabled()));
            }
            OutputFormat::Json => {
                let Ok(json) =
                    serde_json::to_string_pretty(&HistoryDocument::from(entries.as_slice()))
                else {
                    eprintln!("{}", localizer.text("history-json-error"));
                    return ExitCode::FAILURE;
                };
                println!("{json}");
            }
        }
        ExitCode::SUCCESS
    }
}

/// Apply localized help after the bootstrap locale has been parsed.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("history-about"))
        .override_usage(localizer.text("history-usage"))
        .mut_arg("limit", |argument| {
            argument
                .help(localizer.text("history-limit-help"))
                .value_name(localizer.text("history-limit-value"))
        })
        .mut_arg("format", |argument| {
            argument
                .help(localizer.text("history-format-help"))
                .value_name(localizer.text("history-format-value"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}

/// Map structured history failures to localized, dependency-independent diagnostics.
fn present_error(error: &HistoryError, localizer: &Localizer) -> String {
    localizer.text(match error {
        HistoryError::Policy(_) => "history-policy-error",
        HistoryError::Repository(_) => "history-repository-error",
        HistoryError::ProjectOutsideRepository { .. } => "history-project-outside-repository",
    })
}

/// Render newest commits as compact localized blocks.
fn render_human(entries: &[HistoryEntry], localizer: &Localizer, styled: bool) -> String {
    if entries.is_empty() {
        return localizer.text("history-empty");
    }

    entries
        .iter()
        .map(|entry| render_entry(entry, localizer, styled))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Render one commit while keeping arbitrary Git text readable in a terminal.
fn render_entry(entry: &HistoryEntry, localizer: &Localizer, styled: bool) -> String {
    let id = entry.commit.id.to_string();
    let id = short_id(&id);
    let id = if styled {
        format!("\x1b[1;36m{id}\x1b[0m")
    } else {
        id.to_owned()
    };
    let author = format!(
        "{} <{}>",
        entry.commit.author.name.to_str_lossy(),
        entry.commit.author.email.to_str_lossy()
    );
    let date = entry
        .commit
        .authored_at
        .format_or_unix(gix::date::time::format::ISO8601_STRICT);
    let fields = [
        (localizer.text("history-author"), author),
        (localizer.text("history-date"), date),
        (
            localizer.text("history-task"),
            entry.task.as_deref().unwrap_or("—").to_owned(),
        ),
    ];
    let width = fields
        .iter()
        .map(|(label, _)| label.chars().count() + 2)
        .max()
        .unwrap_or_default();
    let mut lines = vec![format!("{id}  {}", entry.commit.subject.to_str_lossy())];
    lines.extend(fields.into_iter().map(|(label, value)| {
        let label = format!("{label}:");
        format!("  {label:<width$}{value}")
    }));
    lines.join("\n")
}

/// Enable decoration only for an interactive terminal that permits color.
fn styling_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Return the conventional compact prefix of a hexadecimal object ID.
fn short_id(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

#[derive(Serialize)]
struct HistoryDocument {
    schema_version: u8,
    commits: Vec<CommitDocument>,
}

#[derive(Serialize)]
struct CommitDocument {
    id: String,
    parents: Vec<String>,
    author: AuthorDocument,
    authored_at: String,
    subject: String,
    subject_encoding: &'static str,
    task: Option<String>,
}

#[derive(Serialize)]
struct AuthorDocument {
    name: String,
    name_encoding: &'static str,
    email: String,
    email_encoding: &'static str,
}

impl From<&[HistoryEntry]> for HistoryDocument {
    /// Build the locale-independent version 1 history schema.
    fn from(entries: &[HistoryEntry]) -> Self {
        Self {
            schema_version: 1,
            commits: entries.iter().map(CommitDocument::from).collect(),
        }
    }
}

impl From<&HistoryEntry> for CommitDocument {
    /// Preserve arbitrary Git identity and subject bytes without lossy JSON conversion.
    fn from(entry: &HistoryEntry) -> Self {
        let (name, name_encoding) = encoded_text(entry.commit.author.name.as_bstr());
        let (email, email_encoding) = encoded_text(entry.commit.author.email.as_bstr());
        let (subject, subject_encoding) = encoded_text(entry.commit.subject.as_bstr());
        Self {
            id: entry.commit.id.to_string(),
            parents: entry
                .commit
                .parents
                .iter()
                .map(ToString::to_string)
                .collect(),
            author: AuthorDocument {
                name,
                name_encoding,
                email,
                email_encoding,
            },
            authored_at: entry
                .commit
                .authored_at
                .format_or_unix(gix::date::time::format::ISO8601_STRICT),
            subject,
            subject_encoding,
            task: entry.task.clone(),
        }
    }
}

/// Keep valid UTF-8 exact and encode arbitrary Git bytes without data loss.
fn encoded_text(value: &BStr) -> (String, &'static str) {
    value.to_str().map_or_else(
        |_| (percent_encode(value), "percent"),
        |value| (value.to_owned(), "utf-8"),
    )
}

/// Percent-encode every byte so non-UTF-8 Git text remains reversible.
fn percent_encode(value: &BStr) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        write!(encoded, "%{byte:02X}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use gix::{
        ObjectId,
        bstr::{BStr, BString},
    };

    use super::{HistoryDocument, encoded_text, short_id};
    use crate::{
        project::history::HistoryEntry,
        vcs::repository::{Commit, CommitAuthor},
    };

    /// Build one deterministic history entry for presentation tests.
    fn entry() -> HistoryEntry {
        HistoryEntry {
            commit: Commit {
                id: ObjectId::from_hex(b"1111111111111111111111111111111111111111").unwrap(),
                parents: vec![
                    ObjectId::from_hex(b"2222222222222222222222222222222222222222").unwrap(),
                ],
                author: CommitAuthor {
                    name: BString::from("Author"),
                    email: BString::from("author@example.invalid"),
                },
                authored_at: gix::date::Time {
                    seconds: 1_767_225_600,
                    offset: 0,
                },
                subject: BString::from("Subject"),
                message: BString::from("Subject\n\nBody\n"),
            },
            task: Some("FI-1".to_owned()),
        }
    }

    #[test]
    fn json_schema_keeps_full_ids_and_structured_fields() {
        let document = HistoryDocument::from([entry()].as_slice());
        let value = serde_json::to_value(document).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(
            value["commits"][0]["id"],
            "1111111111111111111111111111111111111111"
        );
        assert_eq!(
            value["commits"][0]["parents"][0],
            "2222222222222222222222222222222222222222"
        );
        assert_eq!(value["commits"][0]["author"]["name"], "Author");
        assert_eq!(
            value["commits"][0]["authored_at"],
            "2026-01-01T00:00:00+00:00"
        );
        assert_eq!(value["commits"][0]["subject"], "Subject");
        assert_eq!(value["commits"][0]["task"], "FI-1");
    }

    #[test]
    fn arbitrary_bytes_have_an_explicit_reversible_encoding() {
        assert_eq!(
            encoded_text(BStr::new(b"raw-\xFF")),
            ("%72%61%77%2D%FF".to_owned(), "percent")
        );
        assert_eq!(short_id("1234567890abcdef"), "1234567890ab");
    }
}
