//! Localized and stable JSON presentation of the read-only project status.

use std::{
    io::{self, IsTerminal},
    path::Path,
    process::ExitCode,
};

use clap::{Args, ValueEnum};
use serde::Serialize;

use crate::{
    cli::{diagnostics, localization::Localizer},
    project::{
        discovery,
        status::{self, ChangeSummary, HeadState, ProjectStatus, StatusError},
    },
};

#[derive(Debug, Args)]
pub(in crate::cli) struct StatusArgs {
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

impl StatusArgs {
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover(project_dir) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("{}", diagnostics::present_project_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let status = match status::inspect(&project) {
            Ok(status) => status,
            Err(error) => {
                eprintln!("{}", present_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };

        match self.format {
            OutputFormat::Human => {
                println!("{}", render_human(&status, localizer, styling_enabled()));
            }
            OutputFormat::Json => {
                let Ok(json) = serde_json::to_string_pretty(&StatusDocument::from(&status)) else {
                    eprintln!("{}", localizer.text("status-json-error"));
                    return ExitCode::FAILURE;
                };
                println!("{json}");
            }
        }
        ExitCode::SUCCESS
    }
}

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("status-about"))
        .override_usage(localizer.text("status-usage"))
        .mut_arg("format", |argument| {
            argument
                .help(localizer.text("status-format-help"))
                .value_name(localizer.text("status-format-value"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}

fn present_error(error: &StatusError, localizer: &Localizer) -> String {
    localizer.text(match error {
        StatusError::WorkflowNotConfigured => "status-workflow-missing",
        StatusError::Policy(_) => "status-policy-error",
        StatusError::Repository(_) => "status-repository-error",
        StatusError::ProjectOutsideRepository { .. } => "status-project-outside-repository",
    })
}

fn render_human(status: &ProjectStatus, localizer: &Localizer, styled: bool) -> String {
    let branch = status
        .branch
        .as_ref()
        .map_or_else(|| "—".to_owned(), ToString::to_string);
    let project_type = localizer.text(project_type_key(status));
    let workflow = localizer.text(workflow_key(status));
    let mut lines = render_fields(
        &[
            (localizer.text("status-project"), project_name(&status.root)),
            (localizer.text("status-project-type"), project_type),
            (localizer.text("status-workflow"), workflow),
            (
                localizer.text("status-task"),
                status.task.as_deref().unwrap_or("—").to_owned(),
            ),
            (localizer.text("status-branch"), branch),
            (localizer.text("status-base"), status.base_branch.clone()),
        ],
        "",
        styled,
    );
    lines.extend([String::new(), section(localizer, "status-changes", styled)]);
    append_changes(&mut lines, status.changes, localizer, styled);
    lines.extend([
        String::new(),
        section(localizer, "status-synchronization", styled),
    ]);
    if let Some(synchronization) = status.synchronization {
        lines.extend(render_fields(
            &[
                (
                    localizer.text("status-ahead"),
                    synchronization.ahead.to_string(),
                ),
                (
                    localizer.text("status-behind"),
                    synchronization.behind.to_string(),
                ),
            ],
            "  ",
            styled,
        ));
    } else {
        lines.extend(unavailable(localizer, styled));
    }
    lines.extend([String::new(), section(localizer, "status-locks", styled)]);
    if status.locks.available {
        lines.extend(render_fields(
            &[(
                localizer.text("status-objects"),
                status.locks.count.unwrap_or_default().to_string(),
            )],
            "  ",
            styled,
        ));
    } else {
        lines.extend(unavailable(localizer, styled));
    }
    lines.extend([
        String::new(),
        section(localizer, "status-readiness", styled),
        readiness(
            localizer,
            "status-ready-save",
            status.readiness.save,
            styled,
        ),
        readiness(
            localizer,
            "status-ready-publish",
            status.readiness.publish,
            styled,
        ),
    ]);
    lines.join("\n")
}

fn append_changes(
    lines: &mut Vec<String>,
    changes: ChangeSummary,
    localizer: &Localizer,
    styled: bool,
) {
    let mut fields = vec![(localizer.text("status-files"), changes.files.to_string())];
    for (key, value) in [
        ("status-added", changes.added),
        ("status-modified", changes.modified),
        ("status-deleted", changes.deleted),
        ("status-type-changed", changes.type_changed),
        ("status-untracked", changes.untracked),
        ("status-intent-to-add", changes.intent_to_add),
        ("status-conflicts", changes.conflicts),
    ] {
        if value > 0 {
            fields.push((localizer.text(key), value.to_string()));
        }
    }
    lines.extend(render_fields(&fields, "  ", styled));
}

fn render_fields(fields: &[(String, String)], indent: &str, styled: bool) -> Vec<String> {
    let width = fields
        .iter()
        .map(|(label, _)| label.chars().count() + 2)
        .max()
        .unwrap_or_default();
    fields
        .iter()
        .map(|(label, value)| {
            let label = format!("{label}:");
            let padded = format!("{label:<width$}");
            if styled {
                format!("{indent}\x1b[1m{padded}\x1b[0m{value}")
            } else {
                format!("{indent}{padded}{value}")
            }
        })
        .collect()
}

fn section(localizer: &Localizer, key: &str, styled: bool) -> String {
    let title = localizer.text(key);
    if styled {
        format!("\x1b[1;36m{title}\x1b[0m")
    } else {
        title
    }
}

fn unavailable(localizer: &Localizer, styled: bool) -> Vec<String> {
    let value = localizer.text("status-unavailable");
    let value = if styled {
        format!("\x1b[1;33m{value}\x1b[0m")
    } else {
        value
    };
    render_fields(
        &[(localizer.text("status-availability"), value)],
        "  ",
        styled,
    )
}

fn readiness(localizer: &Localizer, key: &str, ready: bool, styled: bool) -> String {
    let symbol = if ready { "✓" } else { "✗" };
    let symbol = if styled {
        format!("\x1b[1;{}m{symbol}\x1b[0m", if ready { "32" } else { "31" })
    } else {
        symbol.to_owned()
    };
    format!("  {symbol} {}", localizer.text(key))
}

fn styling_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

fn project_name(root: &Path) -> String {
    root.file_name()
        .unwrap_or(root.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn project_type_key(status: &ProjectStatus) -> &'static str {
    match status.project_type.as_str() {
        "configuration" => "new-type-configuration",
        "extension" => "new-type-extension",
        "processing" => "new-type-processing",
        "report" => "new-type-report",
        _ => unreachable!("all project types have a localization key"),
    }
}

fn workflow_key(status: &ProjectStatus) -> &'static str {
    match status.workflow.as_str() {
        "trunk" => "new-workflow-trunk",
        "git-flow" => "new-workflow-git-flow",
        "github-flow" => "new-workflow-github-flow",
        "custom" => "new-workflow-custom",
        _ => unreachable!("all workflow presets have a localization key"),
    }
}

#[derive(Serialize)]
struct StatusDocument {
    schema_version: u8,
    project: ProjectDocument,
    workflow: WorkflowDocument,
    changes: ChangeDocument,
    synchronization: Option<SynchronizationDocument>,
    locks: LockDocument,
    readiness: ReadinessDocument,
}

#[derive(Serialize)]
struct ProjectDocument {
    name: String,
    root: String,
    #[serde(rename = "type")]
    project_type: &'static str,
}

#[derive(Serialize)]
struct WorkflowDocument {
    preset: &'static str,
    task: Option<String>,
    branch: Option<String>,
    base: String,
    head: &'static str,
}

#[derive(Serialize)]
struct ChangeDocument {
    files: usize,
    added: usize,
    modified: usize,
    deleted: usize,
    type_changed: usize,
    untracked: usize,
    intent_to_add: usize,
    conflicts: usize,
}

#[derive(Serialize)]
struct SynchronizationDocument {
    ahead: usize,
    behind: usize,
}

#[derive(Serialize)]
struct LockDocument {
    available: bool,
    count: Option<usize>,
}

#[derive(Serialize)]
struct ReadinessDocument {
    save: bool,
    publish: bool,
}

impl From<&ProjectStatus> for StatusDocument {
    fn from(status: &ProjectStatus) -> Self {
        Self {
            schema_version: 1,
            project: ProjectDocument {
                name: project_name(&status.root),
                root: status.root.to_string_lossy().into_owned(),
                project_type: status.project_type.as_str(),
            },
            workflow: WorkflowDocument {
                preset: status.workflow.as_str(),
                task: status.task.clone(),
                branch: status.branch.as_ref().map(ToString::to_string),
                base: status.base_branch.clone(),
                head: match status.head {
                    HeadState::Attached => "attached",
                    HeadState::Detached => "detached",
                    HeadState::Unborn => "unborn",
                },
            },
            changes: ChangeDocument {
                files: status.changes.files,
                added: status.changes.added,
                modified: status.changes.modified,
                deleted: status.changes.deleted,
                type_changed: status.changes.type_changed,
                untracked: status.changes.untracked,
                intent_to_add: status.changes.intent_to_add,
                conflicts: status.changes.conflicts,
            },
            synchronization: status.synchronization.map(|state| SynchronizationDocument {
                ahead: state.ahead,
                behind: state.behind,
            }),
            locks: LockDocument {
                available: status.locks.available,
                count: status.locks.count,
            },
            readiness: ReadinessDocument {
                save: status.readiness.save,
                publish: status.readiness.publish,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::render_fields;

    #[test]
    fn fields_are_aligned_and_style_only_the_labels() {
        let fields = [
            ("Тип".to_owned(), "Конфигурация".to_owned()),
            ("Workflow".to_owned(), "Git Flow".to_owned()),
        ];

        assert_eq!(
            render_fields(&fields, "", false),
            ["Тип:      Конфигурация", "Workflow: Git Flow"]
        );
        assert_eq!(
            render_fields(&fields, "  ", true),
            [
                "  \x1b[1mТип:      \x1b[0mКонфигурация",
                "  \x1b[1mWorkflow: \x1b[0mGit Flow",
            ]
        );
    }
}
