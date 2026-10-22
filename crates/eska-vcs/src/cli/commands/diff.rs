//! Argument parsing, project selection and presentation dispatch for `eska diff`.

use eska_presentation::arguments::vcs::diff::OutputFormat;
pub use eska_presentation::arguments::vcs::diff::{DiffArgs, localize};

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

struct DiffHandler<'a> {
    args: &'a DiffArgs,
}

impl DiffHandler<'_> {
    /// Discover the project, inspect its file changes and select one presentation.
    #[must_use]
    pub fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let context = match discovery::discover_context(project_dir) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("{}", diagnostics::present_context_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let selection = match select_projects(
            &context,
            &self.args.selectors.project,
            self.args.selectors.workspace,
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
        let include_workspace_files = self.args.selectors.workspace
            || (self.args.selectors.project.is_empty() && current_member.is_none());
        if self.args.revisions.is_empty() {
            let changes =
                match diff::inspect_workspace(workspace, &selected, include_workspace_files) {
                    Ok(changes) => changes,
                    Err(error) => return report_error(&error, localizer),
                };
            self.render_workspace_group(&changes, &selected, localizer)
        } else {
            let from = &self.args.revisions[0];
            let to = self.args.revisions.get(1).map_or("HEAD", String::as_str);
            let changes = match diff::compare_workspace(
                workspace,
                &selected,
                include_workspace_files,
                from,
                to,
                self.args.since_branch_point,
            ) {
                Ok(changes) => changes,
                Err(error) => return report_error(&error, localizer),
            };
            self.render_workspace_revisions(&changes, &selected, localizer)
        }
    }

    /// Keep single-project comparison and output selection together.
    fn run_project(&self, project: &Project, localizer: &Localizer) -> ExitCode {
        if self.args.revisions.is_empty() {
            let changes = match diff::inspect(project) {
                Ok(changes) => changes,
                Err(error) => return report_error(&error, localizer),
            };
            if self.args.semantic {
                let changes = match analyze_workspace_semantics(project, &changes, localizer) {
                    Ok(changes) => changes,
                    Err(error) => return self.report_semantic_error(&error, "semantic", localizer),
                };
                return self.render_semantic(&changes, None, localizer);
            }
            self.render_workspace(&changes, localizer)
        } else {
            let from = &self.args.revisions[0];
            let to = self.args.revisions.get(1).map_or("HEAD", String::as_str);
            let changes = match diff::compare(project, from, to, self.args.since_branch_point) {
                Ok(changes) => changes,
                Err(error) => return report_error(&error, localizer),
            };
            if self.args.semantic {
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
        if self.args.raw {
            print!("{}", render_raw(changes));
            return ExitCode::SUCCESS;
        }
        match self.args.format {
            OutputFormat::Human => {
                println!("{}", render_human(changes, localizer, styling_enabled()));
                ExitCode::SUCCESS
            }
            OutputFormat::Json => serialize_json(&DiffDocument::from(changes), localizer),
        }
    }

    /// Render a committed revision comparison without workspace-stage terminology.
    fn render_revisions(&self, changes: &RevisionProjectDiff, localizer: &Localizer) -> ExitCode {
        if self.args.raw {
            print!("{}", render_revision_raw(changes));
            return ExitCode::SUCCESS;
        }
        match self.args.format {
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
        if self.args.raw {
            print!("{}", render_semantic_raw(changes));
            return ExitCode::SUCCESS;
        }
        match self.args.format {
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
        if self.args.semantic {
            let semantic = match analyze_workspace_group(changes, selected, localizer) {
                Ok(changes) => changes,
                Err(error) => {
                    return self.report_semantic_error(&error, "semantic_workspace", localizer);
                }
            };
            return render_workspace_semantic(
                self.args,
                &semantic,
                changes
                    .workspace_files
                    .as_ref()
                    .map(WorkspaceFileDiff::Current),
                None,
                localizer,
            );
        }
        if self.args.raw {
            print!("{}", render_workspace_raw(changes));
            return ExitCode::SUCCESS;
        }
        match self.args.format {
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
        if self.args.semantic {
            let semantic = match analyze_workspace_revision_group(changes, selected, localizer) {
                Ok(changes) => changes,
                Err(error) => {
                    return self.report_semantic_error(&error, "semantic_workspace", localizer);
                }
            };
            return render_workspace_semantic(
                self.args,
                &semantic,
                changes
                    .workspace_files
                    .as_ref()
                    .map(WorkspaceFileDiff::Revisions),
                Some(&changes.comparison),
                localizer,
            );
        }
        if self.args.raw {
            print!("{}", render_workspace_revision_raw(changes));
            return ExitCode::SUCCESS;
        }
        match self.args.format {
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
        if !self.args.raw && matches!(self.args.format, OutputFormat::Json) {
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

/// Execute parsed VCS options in the repository owner.
#[must_use]
pub fn run(args: &DiffArgs, directory: &Path, localizer: &Localizer) -> ExitCode {
    DiffHandler { args }.run(directory, localizer)
}
