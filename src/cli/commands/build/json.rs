//! Versioned build JSON documents, independent of the selected UI locale.

use serde::Serialize;
use std::path::Path;

use crate::{
    cli::{encoding::json_path, localization::Localizer},
    project::{
        ProjectName,
        build::{self, BuildPlan, Ibcmd},
    },
};

use super::{
    PreparedBuild,
    errors::{BuildExecutionError, execution_error_code, execution_error_stage},
};

#[derive(Serialize)]
pub(super) struct BuildDocument {
    schema_version: u8,
    artifact: ArtifactDocument,
    platform: PlatformDocument,
    duration_ms: u128,
}

#[derive(Serialize)]
pub(super) struct BuildPlanDocument {
    schema_version: u8,
    kind: &'static str,
    scope: &'static str,
    projects: Vec<BuildPlanEntryDocument>,
}

#[derive(Serialize)]
struct BuildPlanEntryDocument {
    name: Option<String>,
    root: BuildPlanPathDocument,
    source: BuildPlanPathDocument,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_configuration: Option<BuildPlanPathDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    infobase: Option<BuildPlanInfobaseDocument>,
    artifact: BuildPlanArtifactDocument,
    platform: BuildPlanPlatformDocument,
}

#[derive(Serialize)]
struct BuildPlanPathDocument {
    path: String,
    path_encoding: &'static str,
}

#[derive(Serialize)]
struct BuildPlanArtifactDocument {
    r#type: &'static str,
    path: String,
    path_encoding: &'static str,
    replaces_existing: bool,
}

#[derive(Serialize)]
struct BuildPlanInfobaseDocument {
    root: BuildPlanPathDocument,
    mode: &'static str,
}

#[derive(Serialize)]
struct BuildPlanPlatformDocument {
    required_version: String,
    found_version: String,
    runner: &'static str,
}

#[derive(Serialize)]
struct WorkspaceBuildDocument {
    schema_version: u8,
    projects: Vec<WorkspaceBuildEntry>,
}

#[derive(Serialize)]
pub(super) struct WorkspaceBuildEntry {
    name: String,
    status: &'static str,
    artifact: Option<ArtifactDocument>,
    platform: PlatformDocument,
    duration_ms: Option<u128>,
    error: Option<BuildFailureDocument>,
}

#[derive(Serialize)]
struct BuildFailureDocument {
    code: &'static str,
    stage: Option<&'static str>,
}

#[derive(Serialize)]
struct BuildErrorDetailDocument {
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
}

#[derive(Serialize)]
struct BuildErrorDocument {
    schema_version: u8,
    status: &'static str,
    error: BuildErrorDetailDocument,
}

#[derive(Serialize)]
struct ArtifactDocument {
    r#type: &'static str,
    path: String,
    path_encoding: &'static str,
}

#[derive(Serialize)]
struct PlatformDocument {
    version: String,
}

impl BuildDocument {
    /// Build schema version 1 without locale-dependent values.
    pub(super) fn new(plan: &BuildPlan, result: &build::BuildResult) -> Self {
        let (path, path_encoding) = json_path(result.output().as_os_str());
        Self {
            schema_version: 1,
            artifact: ArtifactDocument {
                r#type: plan.artifact_type().as_str(),
                path,
                path_encoding,
            },
            platform: PlatformDocument {
                version: plan.platform_version().as_str().to_owned(),
            },
            duration_ms: result.duration().as_millis(),
        }
    }
}

impl BuildPlanDocument {
    /// Build a locale-independent dry-run document in execution order.
    pub(super) fn new(prepared: &[PreparedBuild<'_>], tools: &[Ibcmd], aggregate: bool) -> Self {
        let projects = prepared
            .iter()
            .zip(tools)
            .map(|(item, tool)| BuildPlanEntryDocument::new(item, tool))
            .collect();
        Self {
            schema_version: 1,
            kind: "build-plan",
            scope: if aggregate { "workspace" } else { "project" },
            projects,
        }
    }
}

impl BuildPlanEntryDocument {
    /// Serialize one preflighted plan and discovered runner without localized values.
    fn new(prepared: &PreparedBuild<'_>, tool: &Ibcmd) -> Self {
        let (artifact_path, artifact_path_encoding) = json_path(prepared.plan.output().as_os_str());
        Self {
            name: prepared.name.map(|name| name.as_str().to_owned()),
            root: BuildPlanPathDocument::new(prepared.plan.project_root()),
            source: BuildPlanPathDocument::new(prepared.plan.source()),
            base_configuration: prepared
                .plan
                .base_configuration()
                .map(BuildPlanPathDocument::new),
            infobase: prepared
                .plan
                .recreates_infobase()
                .then(|| BuildPlanInfobaseDocument {
                    root: BuildPlanPathDocument::new(prepared.plan.infobase_root()),
                    mode: "recreate",
                }),
            artifact: BuildPlanArtifactDocument {
                r#type: prepared.plan.artifact_type().as_str(),
                path: artifact_path,
                path_encoding: artifact_path_encoding,
                replaces_existing: prepared.plan.output().is_file(),
            },
            platform: BuildPlanPlatformDocument {
                required_version: prepared.plan.platform_version().as_str().to_owned(),
                found_version: tool.version().as_str().to_owned(),
                runner: tool.runner_kind(),
            },
        }
    }
}

impl BuildPlanPathDocument {
    /// Preserve one plan path with the existing reversible build encoding.
    fn new(path: &Path) -> Self {
        let (path, path_encoding) = json_path(path.as_os_str());
        Self {
            path,
            path_encoding,
        }
    }
}

impl WorkspaceBuildEntry {
    /// Serialize one successful member without locale-dependent values.
    pub(super) fn success(
        name: &ProjectName,
        plan: &BuildPlan,
        result: &build::BuildResult,
    ) -> Self {
        let (path, path_encoding) = json_path(result.output().as_os_str());
        Self {
            name: name.as_str().to_owned(),
            status: "success",
            artifact: Some(ArtifactDocument {
                r#type: plan.artifact_type().as_str(),
                path,
                path_encoding,
            }),
            platform: PlatformDocument {
                version: plan.platform_version().as_str().to_owned(),
            },
            duration_ms: Some(result.duration().as_millis()),
            error: None,
        }
    }

    /// Serialize one failed member through stable non-localized error codes.
    pub(super) fn failure(
        name: &ProjectName,
        plan: &BuildPlan,
        error: &BuildExecutionError,
    ) -> Self {
        Self {
            name: name.as_str().to_owned(),
            status: "failed",
            artifact: None,
            platform: PlatformDocument {
                version: plan.platform_version().as_str().to_owned(),
            },
            duration_ms: None,
            error: Some(BuildFailureDocument {
                code: execution_error_code(error),
                stage: execution_error_stage(error),
            }),
        }
    }

    /// Mark a member that was not started because the user interrupted the group.
    pub(super) fn skipped(name: &ProjectName, plan: &BuildPlan) -> Self {
        Self {
            name: name.as_str().to_owned(),
            status: "skipped",
            artifact: None,
            platform: PlatformDocument {
                version: plan.platform_version().as_str().to_owned(),
            },
            duration_ms: None,
            error: Some(BuildFailureDocument {
                code: "not-run-after-interrupt",
                stage: None,
            }),
        }
    }
}

/// Serialize one post-parse failure through the stable top-level error envelope.
pub(super) fn write_build_error(
    code: &'static str,
    stage: Option<&'static str>,
    project: Option<&str>,
    localizer: &Localizer,
) {
    let document = BuildErrorDocument {
        schema_version: 1,
        status: "error",
        error: BuildErrorDetailDocument {
            code,
            stage,
            project: project.map(str::to_owned),
        },
    };
    match serde_json::to_string_pretty(&document) {
        Ok(json) => println!("{json}"),
        Err(_) => eprintln!("{}", localizer.text("build-json-error")),
    }
}

/// Serialize all workspace outcomes in their original execution order.
pub(super) fn write_workspace_json(
    entries: Vec<WorkspaceBuildEntry>,
    localizer: &Localizer,
) -> bool {
    let document = WorkspaceBuildDocument {
        schema_version: 1,
        projects: entries,
    };
    let Ok(json) = serde_json::to_string_pretty(&document) else {
        eprintln!("{}", localizer.text("build-json-error"));
        return false;
    };
    println!("{json}");
    true
}
