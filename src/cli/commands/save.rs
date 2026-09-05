//! Localized presentation and deterministic message drafts for saving a project `ChangeSet`.

use std::{collections::BTreeSet, path::Path, process::ExitCode};

use clap::Args;
use gix::bstr::ByteSlice;

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        Project, diff, discovery, object_model, save,
        semantic::{self, SemanticDiff},
    },
    vcs::command,
};

#[derive(Debug, Args)]
pub(in crate::cli) struct SaveArgs {
    #[arg(short, long, value_name = "MESSAGE")]
    message: Option<String>,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl SaveArgs {
    /// Discover the project and save its complete current `ChangeSet`.
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover(project_dir) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("{}", diagnostics::present_project_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let result = if let Some(message) = self.message.as_deref() {
            save::execute(&project, Some(message))
        } else {
            let draft = match generate_draft(&project, localizer) {
                Ok(draft) => draft,
                Err(error) => {
                    eprintln!("{}", present_diff_error(&error, localizer));
                    return ExitCode::FAILURE;
                }
            };
            save::execute_with_draft(&project, &draft)
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                eprintln!("{}", present_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let commit = result.commit.to_string();
        println!(
            "{}",
            localizer.format(
                "save-created",
                &[
                    (
                        "files",
                        LocalizationValue::Number(i64::try_from(result.files).unwrap_or(i64::MAX)),
                    ),
                    ("commit", LocalizationValue::Text(short_id(&commit))),
                ],
            )
        );
        ExitCode::SUCCESS
    }
}

/// Build a locale-specific commit draft from the exact current file and semantic changes.
fn generate_draft(project: &Project, localizer: &Localizer) -> Result<String, diff::DiffError> {
    let files = diff::inspect(project)?;
    let semantic = object_model::discover(project)
        .ok()
        .and_then(|objects| semantic::diff_workspace(project, &objects, &files).ok())
        .unwrap_or_default();
    Ok(render_draft(&files, &semantic, localizer))
}

/// Render a Conventional Commit title and deterministic semantic/file detail lines.
fn render_draft(
    files: &diff::ProjectDiff,
    semantic: &SemanticDiff,
    localizer: &Localizer,
) -> String {
    let objects: BTreeSet<_> = semantic
        .events()
        .iter()
        .map(|event| event.object().id())
        .collect();
    let scopes: BTreeSet<_> = objects
        .iter()
        .map(|id| super::diff::semantic_object_group(id))
        .collect();
    let subject = if objects.len() == 1 {
        draft_text(&localizer.format(
            "save-draft-subject-object",
            &[(
                "object",
                LocalizationValue::Text(&super::diff::render_semantic_object(
                    objects.first().copied().unwrap_or_default(),
                    localizer,
                )),
            )],
        ))
    } else if objects.is_empty() {
        localizer.text("save-draft-subject-files")
    } else {
        localizer.text("save-draft-subject-objects")
    };
    let commit_type = if objects.is_empty() { "chore" } else { "feat" };
    let scope = (scopes.len() == 1)
        .then(|| scopes.first().copied())
        .flatten()
        .map(|scope| format!("({scope})"))
        .unwrap_or_default();
    let mut lines = vec![format!("{commit_type}{scope}: {subject}"), String::new()];

    let mut details = BTreeSet::new();
    let mut semantic_paths = BTreeSet::new();
    for event in semantic.events() {
        semantic_paths.insert(event.path().to_owned());
        let member = event
            .member()
            .map(|member| format!(" — {member}"))
            .unwrap_or_default();
        details.insert(format!(
            "- {}: {}{}.",
            localizer.text(super::diff::semantic_event_key(event.kind())),
            super::diff::render_semantic_object(event.object().id(), localizer),
            member
        ));
    }
    for file in &files.files {
        if semantic_paths.contains(file.path.as_bstr()) {
            continue;
        }
        let path = super::diff::display_path(file.path.as_bstr());
        details.insert(draft_text(&localizer.format(
            "save-draft-file-change",
            &[("path", LocalizationValue::Text(&path))],
        )));
    }
    lines.extend(details);
    lines.join("\n")
}

/// Remove Fluent bidi-isolation controls from text persisted in Git history.
fn draft_text(value: &str) -> String {
    value.replace(['\u{2068}', '\u{2069}'], "")
}

/// Map draft-inspection failures onto the existing localized save diagnostics.
fn present_diff_error(error: &diff::DiffError, localizer: &Localizer) -> String {
    match error {
        diff::DiffError::ProjectOutsideRepository { .. } => {
            localizer.text("save-project-outside-repository")
        }
        diff::DiffError::Repository(_)
        | diff::DiffError::Revision { .. }
        | diff::DiffError::MergeBase { .. } => localizer.text("save-repository-error"),
    }
}

/// Apply localized help text after clap has parsed the bootstrap locale.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("save-about"))
        .override_usage(localizer.text("save-usage"))
        .mut_arg("message", |argument| {
            argument
                .help(localizer.text("save-message-help"))
                .value_name(localizer.text("save-message-value"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}

fn present_error(error: &save::SaveError, localizer: &Localizer) -> String {
    match error {
        save::SaveError::Repository(_) => localizer.text("save-repository-error"),
        save::SaveError::ProjectOutsideRepository { .. } => {
            localizer.text("save-project-outside-repository")
        }
        save::SaveError::DetachedHead => localizer.text("save-detached-head"),
        save::SaveError::NoChanges => localizer.text("save-no-changes"),
        save::SaveError::Conflicts { files } => localizer.format(
            "save-conflicts",
            &[(
                "files",
                LocalizationValue::Number(i64::try_from(*files).unwrap_or(i64::MAX)),
            )],
        ),
        save::SaveError::EmptyMessage => localizer.text("save-empty-message"),
        save::SaveError::IndexSnapshot { path, .. } => path_error(
            localizer,
            "save-index-snapshot-error",
            &path.to_string_lossy(),
        ),
        save::SaveError::Command(error) => present_command_error(error, localizer),
        save::SaveError::IndexRestore { path, original, .. } => localizer.format(
            "save-index-restore-error",
            &[
                (
                    "reason",
                    LocalizationValue::Text(&present_error(original, localizer)),
                ),
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
            ],
        ),
        save::SaveError::CommitNotCreated => localizer.text("save-commit-not-created"),
    }
}

fn present_command_error(error: &command::Error, localizer: &Localizer) -> String {
    let (operation, reason) = match error {
        command::Error::Spawn { operation, source } => (*operation, source.to_string()),
        command::Error::Failed {
            operation,
            status,
            stderr,
        } => {
            let stderr = String::from_utf8_lossy(stderr).trim().to_owned();
            let reason = if stderr.is_empty() {
                status.to_string()
            } else {
                stderr
            };
            (*operation, reason)
        }
    };
    let key = match operation {
        command::Operation::Stage => "save-stage-error",
        command::Operation::Commit => "save-commit-error",
        _ => "save-repository-error",
    };
    localizer.format(key, &[("reason", LocalizationValue::Text(&reason))])
}

fn path_error(localizer: &Localizer, key: &str, path: &str) -> String {
    localizer.format(key, &[("path", LocalizationValue::Text(path))])
}

fn short_id(id: &str) -> &str {
    &id[..id.len().min(7)]
}
