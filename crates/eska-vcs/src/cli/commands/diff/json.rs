//! Versioned JSON documents and byte-preserving machine values for diff output.

use super::{WorkspaceFileDiff, analysis::NamedSemanticDiff};
use crate::{
    cli::{encoding::json_git_text, localization::Localizer},
    project::{
        diff::{self, ProjectDiff, RevisionProjectDiff, WorkspaceDiff, WorkspaceRevisionDiff},
        semantic::{SemanticDiff, SemanticDiffError, SemanticEvent, SemanticFallback},
    },
    vcs::status::Change,
};
use gix::bstr::ByteSlice;
use serde::Serialize;
use std::process::ExitCode;

/// Serialize one locale-independent document and report the shared failure.
pub(super) fn serialize_json(document: &impl Serialize, localizer: &Localizer) -> ExitCode {
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

#[derive(Serialize)]
pub(super) struct SemanticDiffDocument<'a> {
    schema_version: u8,
    kind: &'static str,
    comparison: SemanticComparisonDocument<'a>,
    events: Vec<SemanticEventDocument>,
    analysis: SemanticAnalysisDocument,
}

#[derive(Serialize)]
pub(super) struct WorkspaceSemanticDiffDocument<'a> {
    schema_version: u8,
    kind: &'static str,
    comparison: SemanticComparisonDocument<'a>,
    projects: Vec<WorkspaceSemanticProjectDocument>,
    workspace_files: Option<WorkspaceSemanticFilesDocument>,
}

#[derive(Serialize)]
struct WorkspaceSemanticProjectDocument {
    name: String,
    events: Vec<SemanticEventDocument>,
    analysis: SemanticAnalysisDocument,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WorkspaceSemanticFilesDocument {
    Workspace { files: Vec<FileDocument> },
    Revisions { files: Vec<RevisionFileDocument> },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SemanticComparisonDocument<'a> {
    Workspace,
    Revisions {
        strategy: &'static str,
        from: RevisionEndpointDocument<'a>,
        to: RevisionEndpointDocument<'a>,
        merge_base_commit: Option<String>,
    },
}

#[derive(Serialize)]
struct SemanticEventDocument {
    kind: &'static str,
    stage: &'static str,
    object: SemanticObjectDocument,
    member: Option<String>,
    path: String,
    path_encoding: &'static str,
}

#[derive(Serialize)]
struct SemanticObjectDocument {
    id: String,
    metadata_type: &'static str,
    name: String,
}

#[derive(Serialize)]
struct SemanticAnalysisDocument {
    complete: bool,
    fallbacks: Vec<SemanticFallbackDocument>,
}

#[derive(Serialize)]
struct SemanticFallbackDocument {
    reason: &'static str,
    stage: &'static str,
    path: String,
    path_encoding: &'static str,
}

#[derive(Serialize)]
pub(super) struct SemanticErrorDocument {
    pub(super) schema_version: u8,
    pub(super) kind: &'static str,
    pub(super) status: &'static str,
    pub(super) error: SemanticErrorDetailDocument,
}

#[derive(Serialize)]
pub(super) struct SemanticErrorDetailDocument {
    pub(super) code: &'static str,
}

/// Map semantic operation failures to locale-independent machine codes.
pub(super) const fn semantic_error_code(error: &SemanticDiffError) -> &'static str {
    match error {
        SemanticDiffError::Repository(_) => "repository",
        SemanticDiffError::ObjectModel(_) => "object-model",
        SemanticDiffError::ProjectOutsideRepository { .. } => "project-outside-repository",
    }
}

impl<'a> SemanticDiffDocument<'a> {
    /// Build semantic schema version 4 with explicit analysis completeness.
    pub(super) fn new(diff: &SemanticDiff, revisions: Option<&'a RevisionProjectDiff>) -> Self {
        let comparison = semantic_comparison(revisions.map(|diff| &diff.comparison));
        Self {
            schema_version: 4,
            kind: "semantic",
            comparison,
            events: diff
                .events()
                .iter()
                .map(SemanticEventDocument::from)
                .collect(),
            analysis: SemanticAnalysisDocument::from(diff),
        }
    }
}

impl<'a> WorkspaceSemanticDiffDocument<'a> {
    /// Build the aggregate semantic document using the shared comparison mapping.
    pub(super) fn new(
        semantic: &[NamedSemanticDiff],
        workspace_files: Option<WorkspaceFileDiff<'_>>,
        comparison: Option<&'a diff::RevisionComparison>,
    ) -> Self {
        Self {
            schema_version: 4,
            kind: "semantic_workspace",
            comparison: semantic_comparison(comparison),
            projects: semantic
                .iter()
                .map(|project| WorkspaceSemanticProjectDocument {
                    name: project.name.as_str().to_owned(),
                    events: project
                        .diff
                        .events()
                        .iter()
                        .map(SemanticEventDocument::from)
                        .collect(),
                    analysis: SemanticAnalysisDocument::from(&project.diff),
                })
                .collect(),
            workspace_files: workspace_files.map(|files| match files {
                WorkspaceFileDiff::Current(files) => WorkspaceSemanticFilesDocument::Workspace {
                    files: file_documents(files),
                },
                WorkspaceFileDiff::Revisions(files) => WorkspaceSemanticFilesDocument::Revisions {
                    files: revision_file_documents(files),
                },
            }),
        }
    }
}

/// Share exact revision endpoints between single and aggregate semantic JSON.
fn semantic_comparison(
    comparison: Option<&diff::RevisionComparison>,
) -> SemanticComparisonDocument<'_> {
    comparison.map_or(SemanticComparisonDocument::Workspace, |comparison| {
        SemanticComparisonDocument::Revisions {
            strategy: if comparison.merge_base_commit.is_some() {
                "merge_base"
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
        }
    })
}

impl From<&SemanticEvent> for SemanticEventDocument {
    /// Preserve stable identities, event names and arbitrary Git path bytes.
    fn from(event: &SemanticEvent) -> Self {
        let (path, path_encoding) = json_git_text(event.path());
        Self {
            kind: event.kind().as_str(),
            stage: event.stage().as_str(),
            object: SemanticObjectDocument {
                id: event.object().id().to_owned(),
                metadata_type: event.object().metadata_type(),
                name: event.object().name().to_owned(),
            },
            member: event.member().map(str::to_owned),
            path,
            path_encoding,
        }
    }
}

impl From<&SemanticDiff> for SemanticAnalysisDocument {
    /// Map domain data into the existing locale-independent JSON schema.
    fn from(diff: &SemanticDiff) -> Self {
        Self {
            complete: diff.is_complete(),
            fallbacks: diff
                .fallbacks()
                .iter()
                .map(SemanticFallbackDocument::from)
                .collect(),
        }
    }
}

impl From<&SemanticFallback> for SemanticFallbackDocument {
    /// Map domain data into the existing locale-independent JSON schema.
    fn from(fallback: &SemanticFallback) -> Self {
        let (path, path_encoding) = json_git_text(fallback.path());
        Self {
            reason: fallback.reason().as_str(),
            stage: fallback.stage().as_str(),
            path,
            path_encoding,
        }
    }
}

#[derive(Serialize)]
pub(super) struct DiffDocument {
    schema_version: u8,
    files: Vec<FileDocument>,
}

#[derive(Serialize)]
pub(super) struct WorkspaceDiffDocument {
    schema_version: u8,
    projects: Vec<WorkspaceFileProjectDocument>,
    workspace_files: Option<Vec<FileDocument>>,
}

#[derive(Serialize)]
struct WorkspaceFileProjectDocument {
    name: String,
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
            files: file_documents(diff),
        }
    }
}

impl From<&WorkspaceDiff> for WorkspaceDiffDocument {
    /// Map domain data into the existing locale-independent JSON schema.
    fn from(diff: &WorkspaceDiff) -> Self {
        Self {
            schema_version: 1,
            projects: diff
                .projects
                .iter()
                .map(|project| WorkspaceFileProjectDocument {
                    name: project.name.as_str().to_owned(),
                    files: file_documents(&project.diff),
                })
                .collect(),
            workspace_files: diff.workspace_files.as_ref().map(file_documents),
        }
    }
}

/// Retain both comparison edges and reversible paths for current files.
fn file_documents(diff: &ProjectDiff) -> Vec<FileDocument> {
    diff.files
        .iter()
        .map(|file| {
            let (path, path_encoding) = json_git_text(file.path.as_bstr());
            FileDocument {
                path,
                path_encoding,
                index: file.index.map(change_name),
                worktree: file.worktree.map(change_name),
            }
        })
        .collect()
}

#[derive(Serialize)]
pub(super) struct RevisionDiffDocument<'a> {
    schema_version: u8,
    comparison: RevisionComparisonDocument<'a>,
    files: Vec<RevisionFileDocument>,
}

#[derive(Serialize)]
pub(super) struct WorkspaceRevisionDiffDocument<'a> {
    schema_version: u8,
    comparison: RevisionComparisonDocument<'a>,
    projects: Vec<WorkspaceRevisionProjectDocument>,
    workspace_files: Option<Vec<RevisionFileDocument>>,
}

#[derive(Serialize)]
struct WorkspaceRevisionProjectDocument {
    name: String,
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
            comparison: revision_comparison_document(comparison),
            files: revision_file_documents(diff),
        }
    }
}

impl<'a> From<&'a WorkspaceRevisionDiff> for WorkspaceRevisionDiffDocument<'a> {
    /// Map domain data into the existing locale-independent JSON schema.
    fn from(diff: &'a WorkspaceRevisionDiff) -> Self {
        Self {
            schema_version: 2,
            comparison: revision_comparison_document(&diff.comparison),
            projects: diff
                .projects
                .iter()
                .map(|project| WorkspaceRevisionProjectDocument {
                    name: project.name.as_str().to_owned(),
                    files: revision_file_documents(&project.diff),
                })
                .collect(),
            workspace_files: diff.workspace_files.as_ref().map(revision_file_documents),
        }
    }
}

/// Retain requested revision names alongside immutable resolved commit IDs.
fn revision_comparison_document(
    comparison: &diff::RevisionComparison,
) -> RevisionComparisonDocument<'_> {
    RevisionComparisonDocument {
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
    }
}

/// Encode committed file changes without workspace-stage terminology.
fn revision_file_documents(diff: &RevisionProjectDiff) -> Vec<RevisionFileDocument> {
    diff.files
        .iter()
        .map(|file| {
            let (path, path_encoding) = json_git_text(file.path.as_bstr());
            RevisionFileDocument {
                path,
                path_encoding,
                change: change_name(file.change),
            }
        })
        .collect()
}

/// Map a state to its stable JSON value.
pub(super) const fn change_name(change: Change) -> &'static str {
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
