//! Argument parsing, project selection and presentation dispatch for `eska diff`.

mod analysis;
mod human;
mod json;
mod raw;

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        Project, WorkspaceMember,
        diff::{
            self, DiffError, ProjectDiff, RevisionProjectDiff, WorkspaceDiff, WorkspaceRevisionDiff,
        },
        discovery::{self, DiscoveryContext},
        selection::{SelectionIntent, select_projects},
        semantic::{SemanticDiff, SemanticDiffError},
    },
};
use analysis::{
    NamedSemanticDiff, analyze_revision_semantics, analyze_workspace_group,
    analyze_workspace_revision_group, analyze_workspace_semantics,
};
use clap::{Args, ValueEnum};
use human::{
    render_human, render_revision_human, render_semantic_human, render_workspace_human,
    render_workspace_revision_human, render_workspace_semantic_human, styling_enabled,
};
use json::{
    DiffDocument, RevisionDiffDocument, SemanticDiffDocument, SemanticErrorDetailDocument,
    SemanticErrorDocument, WorkspaceDiffDocument, WorkspaceRevisionDiffDocument,
    WorkspaceSemanticDiffDocument, semantic_error_code, serialize_json,
};
use raw::{
    render_raw, render_revision_raw, render_semantic_raw, render_workspace_raw,
    render_workspace_revision_raw, render_workspace_semantic_raw,
};
use std::{path::Path, process::ExitCode};

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

    /// Keep single-project comparison and output selection together.
    fn run_project(&self, project: &Project, localizer: &Localizer) -> ExitCode {
        if self.revisions.is_empty() {
            let changes = match diff::inspect(project) {
                Ok(changes) => changes,
                Err(error) => return report_error(&error, localizer),
            };
            if self.semantic {
                let changes = match analyze_workspace_semantics(project, &changes, localizer) {
                    Ok(changes) => changes,
                    Err(error) => return self.report_semantic_error(&error, "semantic", localizer),
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
                let semantic = match analyze_revision_semantics(project, &changes, localizer) {
                    Ok(changes) => changes,
                    Err(error) => return self.report_semantic_error(&error, "semantic", localizer),
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

    /// Present current changes while preserving the selected member order.
    fn render_workspace_group(
        &self,
        changes: &WorkspaceDiff,
        selected: &[&WorkspaceMember],
        localizer: &Localizer,
    ) -> ExitCode {
        if self.semantic {
            let semantic = match analyze_workspace_group(changes, selected, localizer) {
                Ok(changes) => changes,
                Err(error) => {
                    return self.report_semantic_error(&error, "semantic_workspace", localizer);
                }
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

    /// Present committed changes for the selected workspace members.
    fn render_workspace_revisions(
        &self,
        changes: &WorkspaceRevisionDiff,
        selected: &[&WorkspaceMember],
        localizer: &Localizer,
    ) -> ExitCode {
        if self.semantic {
            let semantic = match analyze_workspace_revision_group(changes, selected, localizer) {
                Ok(changes) => changes,
                Err(error) => {
                    return self.report_semantic_error(&error, "semantic_workspace", localizer);
                }
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

    /// Report a failed semantic operation while preserving a stable JSON error envelope.
    fn report_semantic_error(
        &self,
        error: &SemanticDiffError,
        kind: &'static str,
        localizer: &Localizer,
    ) -> ExitCode {
        eprintln!("{}", localizer.text("diff-semantic-error"));
        if !self.raw && matches!(self.format, OutputFormat::Json) {
            let document = SemanticErrorDocument {
                schema_version: 4,
                kind,
                status: "error",
                error: SemanticErrorDetailDocument {
                    code: semantic_error_code(error),
                },
            };
            if serialize_json(&document, localizer) == ExitCode::FAILURE {
                return ExitCode::FAILURE;
            }
        }
        ExitCode::FAILURE
    }
}

#[derive(Clone, Copy)]
enum WorkspaceFileDiff<'a> {
    Current(&'a ProjectDiff),
    Revisions(&'a RevisionProjectDiff),
}

/// Select one aggregate semantic representation without repeating analysis.
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
