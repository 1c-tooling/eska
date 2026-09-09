//! Localized presentation and deterministic message drafts for saving a project `ChangeSet`.

use std::{collections::BTreeSet, path::Path, process::ExitCode};

use clap::Args;
use gix::bstr::ByteSlice;

use crate::{
    cli::{
        changes, diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        Project, Workspace, diff,
        discovery::{self, DiscoveryContext},
        save,
        selection::{SelectionIntent, select_projects},
        semantic::{self, SemanticDiff, SemanticDiffError},
    },
    vcs::command,
};

#[derive(Debug, Args)]
pub(in crate::cli) struct SaveArgs {
    #[arg(short, long, value_name = "MESSAGE")]
    message: Option<String>,

    #[arg(short = 'p', long)]
    project: Vec<String>,

    #[arg(long)]
    workspace: bool,

    #[arg(long)]
    dry_run: bool,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl SaveArgs {
    /// Discover the project and save its complete current `ChangeSet`.
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
            &self.project,
            self.workspace,
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
        let workspace_scope = match &context {
            DiscoveryContext::Workspace { current_member, .. } => {
                self.workspace || (self.project.is_empty() && current_member.is_none())
            }
            DiscoveryContext::Standalone(_) => false,
        };
        if workspace_scope {
            let DiscoveryContext::Workspace { workspace, .. } = &context else {
                eprintln!("{}", localizer.text("project-selector-standalone"));
                return ExitCode::FAILURE;
            };
            return self.save_workspace(workspace, localizer);
        }
        if selection.projects().len() != 1 {
            eprintln!("{}", localizer.text("save-single-project-required"));
            return ExitCode::FAILURE;
        }
        let selected = selection.projects()[0];
        self.save_project(
            selected.project(),
            selected.name().map(crate::project::ProjectName::as_str),
            localizer,
        )
    }

    fn save_project(
        &self,
        project: &Project,
        name: Option<&str>,
        localizer: &Localizer,
    ) -> ExitCode {
        if self.dry_run {
            return self.preview_project(project, name, localizer);
        }
        let result = if let Some(message) = self.message.as_deref() {
            save::execute(project, Some(message))
        } else {
            let draft = match generate_draft(project, localizer) {
                Ok(draft) => draft,
                Err(error) => {
                    eprintln!("{}", present_diff_error(&error, localizer));
                    return ExitCode::FAILURE;
                }
            };
            save::execute_with_draft(project, &draft)
        };
        present_result(result, localizer)
    }

    fn save_workspace(&self, workspace: &Workspace, localizer: &Localizer) -> ExitCode {
        if self.dry_run {
            return self.preview_workspace(workspace, localizer);
        }
        let result = if let Some(message) = self.message.as_deref() {
            save::execute_workspace(workspace, Some(message))
        } else {
            let draft = match generate_workspace_draft(workspace, localizer) {
                Ok(draft) => draft,
                Err(error) => {
                    eprintln!("{}", present_diff_error(&error, localizer));
                    return ExitCode::FAILURE;
                }
            };
            save::execute_workspace_with_draft(workspace, &draft)
        };
        present_result(result, localizer)
    }

    fn preview_project(
        &self,
        project: &Project,
        name: Option<&str>,
        localizer: &Localizer,
    ) -> ExitCode {
        if let Some(message) = self.message.as_deref()
            && let Err(error) = save::validate_message(message)
        {
            return present_failure(&error, localizer);
        }
        let plan = match save::plan(project) {
            Ok(plan) => plan,
            Err(error) => return present_failure(&error, localizer),
        };
        let message = match self.message.as_deref() {
            Some(message) => message.to_owned(),
            None => match generate_draft(project, localizer) {
                Ok(draft) => draft,
                Err(error) => {
                    eprintln!("{}", present_diff_error(&error, localizer));
                    return ExitCode::FAILURE;
                }
            },
        };
        present_preview(
            &plan,
            PreviewScope::Project {
                name,
                root: project.root(),
            },
            &message,
            localizer,
        )
    }

    fn preview_workspace(&self, workspace: &Workspace, localizer: &Localizer) -> ExitCode {
        if let Some(message) = self.message.as_deref()
            && let Err(error) = save::validate_message(message)
        {
            return present_failure(&error, localizer);
        }
        let plan = match save::plan_workspace(workspace) {
            Ok(plan) => plan,
            Err(error) => return present_failure(&error, localizer),
        };
        let message = match self.message.as_deref() {
            Some(message) => message.to_owned(),
            None => match generate_workspace_draft(workspace, localizer) {
                Ok(draft) => draft,
                Err(error) => {
                    eprintln!("{}", present_diff_error(&error, localizer));
                    return ExitCode::FAILURE;
                }
            },
        };
        present_preview(
            &plan,
            PreviewScope::Workspace {
                root: workspace.root(),
            },
            &message,
            localizer,
        )
    }
}

#[derive(Clone, Copy)]
enum PreviewScope<'a> {
    Project {
        name: Option<&'a str>,
        root: &'a Path,
    },
    Workspace {
        root: &'a Path,
    },
}

fn present_preview(
    plan: &save::SavePlan,
    scope: PreviewScope<'_>,
    message: &str,
    localizer: &Localizer,
) -> ExitCode {
    println!("{}", localizer.text("save-preview-title"));
    match scope {
        PreviewScope::Project {
            name: Some(name),
            root,
        } => println!(
            "{}",
            localizer.format(
                "save-preview-scope-member",
                &[
                    ("project", LocalizationValue::Text(name)),
                    ("path", LocalizationValue::Text(&root.to_string_lossy())),
                ],
            )
        ),
        PreviewScope::Project { name: None, root } => println!(
            "{}",
            localizer.format(
                "save-preview-scope-project",
                &[("path", LocalizationValue::Text(&root.to_string_lossy()))],
            )
        ),
        PreviewScope::Workspace { root } => println!(
            "{}",
            localizer.format(
                "save-preview-scope-workspace",
                &[("path", LocalizationValue::Text(&root.to_string_lossy()))],
            )
        ),
    }
    println!(
        "{}",
        localizer.format(
            "save-preview-files",
            &[(
                "files",
                LocalizationValue::Number(i64::try_from(plan.files().len()).unwrap_or(i64::MAX)),
            )],
        )
    );
    for file in plan.files() {
        let path = changes::display_path(file.path());
        let index = localizer.text(save_state_key(file.index()));
        let worktree = localizer.text(save_state_key(file.worktree()));
        println!(
            "{}",
            localizer.format(
                "save-preview-file",
                &[
                    ("path", LocalizationValue::Text(&path)),
                    ("index", LocalizationValue::Text(&index)),
                    ("worktree", LocalizationValue::Text(&worktree)),
                ],
            )
        );
    }
    println!("{}", localizer.text("save-preview-message"));
    println!("{message}");
    ExitCode::SUCCESS
}

const fn save_state_key(change: Option<crate::vcs::status::Change>) -> &'static str {
    use crate::vcs::status::Change;

    match change {
        None => "save-preview-state-unchanged",
        Some(Change::Added) => "save-preview-state-added",
        Some(Change::Modified) => "save-preview-state-modified",
        Some(Change::Deleted) => "save-preview-state-deleted",
        Some(Change::TypeChanged) => "save-preview-state-type-changed",
        Some(Change::Untracked) => "save-preview-state-untracked",
        Some(Change::IntentToAdd) => "save-preview-state-intent-to-add",
        Some(Change::Conflict) => "save-preview-state-conflict",
    }
}

fn present_failure(error: &save::SaveError, localizer: &Localizer) -> ExitCode {
    eprintln!("{}", present_error(error, localizer));
    ExitCode::FAILURE
}

fn present_result(
    result: Result<save::SaveResult, save::SaveError>,
    localizer: &Localizer,
) -> ExitCode {
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

/// Build a locale-specific commit draft from the exact current file and semantic changes.
fn generate_draft(project: &Project, localizer: &Localizer) -> Result<String, diff::DiffError> {
    let files = diff::inspect(project)?;
    let semantic = draft_semantics(project, &files, None, localizer);
    Ok(render_draft(&files, &semantic, localizer))
}

fn generate_workspace_draft(
    workspace: &Workspace,
    localizer: &Localizer,
) -> Result<String, diff::DiffError> {
    let members = workspace.members().iter().collect::<Vec<_>>();
    let changes = diff::inspect_workspace(workspace, &members, true)?;
    let projects = changes
        .projects
        .iter()
        .zip(&members)
        .map(|(changes, member)| {
            let semantic = draft_semantics(
                member.project(),
                &changes.diff,
                Some(changes.name.as_str()),
                localizer,
            );
            WorkspaceDraftProject {
                name: changes.name.as_str(),
                files: &changes.diff,
                semantic,
            }
        })
        .collect::<Vec<_>>();
    Ok(render_workspace_draft(
        &projects,
        changes.workspace_files.as_ref(),
        localizer,
    ))
}

/// Analyze one draft scope while retaining exact files and explaining every fallback.
fn draft_semantics(
    project: &Project,
    files: &diff::ProjectDiff,
    member: Option<&str>,
    localizer: &Localizer,
) -> SemanticDiff {
    match semantic::diff_workspace_affected(project, files) {
        Ok(semantic) => {
            for fallback in semantic.fallbacks() {
                let path = changes::display_path(fallback.path());
                let path = member.map_or_else(|| path.clone(), |member| format!("{member}/{path}"));
                let reason = localizer.text(changes::semantic_fallback_key(fallback.reason()));
                eprintln!(
                    "{}",
                    localizer.format(
                        "semantic-fallback",
                        &[
                            ("path", LocalizationValue::Text(&path)),
                            ("reason", LocalizationValue::Text(&reason)),
                        ],
                    )
                );
            }
            semantic
        }
        Err(error) => {
            let scope = member.map_or_else(
                || {
                    project
                        .root()
                        .file_name()
                        .unwrap_or_else(|| project.root().as_os_str())
                        .to_string_lossy()
                        .into_owned()
                },
                str::to_owned,
            );
            let reason = localizer.text(semantic_error_key(&error));
            eprintln!(
                "{}",
                localizer.format(
                    "semantic-unavailable",
                    &[
                        ("scope", LocalizationValue::Text(&scope)),
                        ("reason", LocalizationValue::Text(&reason)),
                    ],
                )
            );
            SemanticDiff::default()
        }
    }
}

/// Select a localized high-level reason for a semantic-analysis error.
const fn semantic_error_key(error: &SemanticDiffError) -> &'static str {
    match error {
        SemanticDiffError::Repository(_) => "semantic-unavailable-repository",
        SemanticDiffError::ObjectModel(_) => "semantic-unavailable-object-model",
        SemanticDiffError::ProjectOutsideRepository { .. } => "semantic-unavailable-scope",
    }
}

struct WorkspaceDraftProject<'a> {
    name: &'a str,
    files: &'a diff::ProjectDiff,
    semantic: SemanticDiff,
}

fn render_workspace_draft(
    projects: &[WorkspaceDraftProject<'_>],
    workspace_files: Option<&diff::ProjectDiff>,
    localizer: &Localizer,
) -> String {
    let objects = projects
        .iter()
        .flat_map(|project| {
            project
                .semantic
                .events()
                .iter()
                .map(move |event| (project.name, event.object().id()))
        })
        .collect::<BTreeSet<_>>();
    let scopes = objects
        .iter()
        .map(|(_, id)| changes::semantic_object_group(id))
        .collect::<BTreeSet<_>>();
    let subject = if objects.len() == 1 {
        let (_, id) = objects.first().copied().unwrap_or_default();
        draft_text(&localizer.format(
            "save-draft-subject-object",
            &[(
                "object",
                LocalizationValue::Text(&changes::render_semantic_object(id, localizer)),
            )],
        ))
    } else if objects.is_empty() {
        localizer.text("save-draft-subject-workspace-files")
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
    for project in projects {
        let fallback_paths = project
            .semantic
            .fallbacks()
            .iter()
            .map(|fallback| fallback.path().to_owned())
            .collect::<BTreeSet<_>>();
        let semantic_paths = project
            .semantic
            .events()
            .iter()
            .filter(|event| !fallback_paths.contains(event.path()))
            .map(|event| event.path().to_owned())
            .collect::<BTreeSet<_>>();
        for event in project.semantic.events() {
            let member = event
                .member()
                .map(|member| format!(" — {member}"))
                .unwrap_or_default();
            details.insert(format!(
                "- {}: {}{} — {}.",
                localizer.text(changes::semantic_event_key(event.kind())),
                changes::render_semantic_object(event.object().id(), localizer),
                member,
                project.name,
            ));
        }
        for file in &project.files.files {
            if semantic_paths.contains(file.path.as_bstr()) {
                continue;
            }
            let path = format!(
                "{}/{}",
                project.name,
                changes::display_path(file.path.as_bstr())
            );
            details.insert(draft_text(&localizer.format(
                "save-draft-file-change",
                &[("path", LocalizationValue::Text(&path))],
            )));
        }
    }
    if let Some(files) = workspace_files {
        details.extend(files.files.iter().map(|file| {
            draft_text(&localizer.format(
                "save-draft-file-change",
                &[(
                    "path",
                    LocalizationValue::Text(&changes::display_path(file.path.as_bstr())),
                )],
            ))
        }));
    }
    lines.extend(details);
    lines.join("\n")
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
        .map(|id| changes::semantic_object_group(id))
        .collect();
    let subject = if objects.len() == 1 {
        draft_text(&localizer.format(
            "save-draft-subject-object",
            &[(
                "object",
                LocalizationValue::Text(&changes::render_semantic_object(
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
    let fallback_paths = semantic
        .fallbacks()
        .iter()
        .map(|fallback| fallback.path().to_owned())
        .collect::<BTreeSet<_>>();
    let mut semantic_paths = BTreeSet::new();
    for event in semantic.events() {
        if !fallback_paths.contains(event.path()) {
            semantic_paths.insert(event.path().to_owned());
        }
        let member = event
            .member()
            .map(|member| format!(" — {member}"))
            .unwrap_or_default();
        details.insert(format!(
            "- {}: {}{}.",
            localizer.text(changes::semantic_event_key(event.kind())),
            changes::render_semantic_object(event.object().id(), localizer),
            member
        ));
    }
    for file in &files.files {
        if semantic_paths.contains(file.path.as_bstr()) {
            continue;
        }
        let path = changes::display_path(file.path.as_bstr());
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
        .mut_arg("project", |argument| {
            argument
                .help(localizer.text("save-project-help"))
                .value_name(localizer.text("save-project-value"))
        })
        .mut_arg("workspace", |argument| {
            argument.help(localizer.text("save-workspace-help"))
        })
        .mut_arg("dry_run", |argument| {
            argument.help(localizer.text("save-dry-run-help"))
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
