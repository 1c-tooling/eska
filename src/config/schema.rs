//! TOML field layout, defaults and machine-facing value conversions.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ProjectConfigError;
use crate::project::{ProjectType, SourceFormat};

pub(super) const DEFAULT_SOURCE: &str = "src";
const DEFAULT_SOURCE_FORMAT: &str = "designer-xml";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawDocument {
    pub(super) project: RawProject,
    pub(super) vcs: Option<RawVcs>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawVcs {
    pub(super) workflow: RawWorkflow,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawWorkflow {
    pub(super) preset: String,
    pub(super) extends: Option<String>,
    pub(super) policy: Option<RawPolicy>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawPolicy {
    pub(super) base_branch: Option<String>,
    pub(super) working_branch: Option<String>,
    pub(super) task_branch_template: Option<String>,
    pub(super) remote: Option<String>,
    pub(super) sync_strategy: Option<String>,
    pub(super) integration_target: Option<String>,
    pub(super) publish: Option<String>,
    pub(super) finish: Option<String>,
    pub(super) delete_local_branch: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawProject {
    #[serde(rename = "type")]
    pub(super) project_type: String,
    #[serde(default = "default_source")]
    pub(super) source: PathBuf,
    #[serde(default = "default_source_format")]
    pub(super) source_format: String,
}

#[derive(Serialize)]
pub(super) struct SerializedDocument<'a> {
    pub(super) project: SerializedProject<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) vcs: Option<SerializedVcs<'a>>,
}

#[derive(Serialize)]
pub(super) struct SerializedVcs<'a> {
    pub(super) workflow: SerializedWorkflow<'a>,
}

#[derive(Serialize)]
pub(super) struct SerializedWorkflow<'a> {
    pub(super) preset: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) extends: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) policy: Option<SerializedPolicy<'a>>,
}

#[derive(Serialize)]
pub(super) struct SerializedPolicy<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) base_branch: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) working_branch: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) task_branch_template: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) remote: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) sync_strategy: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) integration_target: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) publish: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) finish: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) delete_local_branch: Option<bool>,
}

#[derive(Serialize)]
pub(super) struct SerializedProject<'a> {
    #[serde(rename = "type")]
    pub(super) project_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) source: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) source_format: Option<&'static str>,
}

pub(super) fn default_source() -> PathBuf {
    PathBuf::from(DEFAULT_SOURCE)
}

pub(super) fn default_source_format() -> String {
    DEFAULT_SOURCE_FORMAT.to_owned()
}

pub fn parse_project_type(value: String) -> Result<ProjectType, ProjectConfigError> {
    match value.as_str() {
        "configuration" => Ok(ProjectType::Configuration),
        "extension" => Ok(ProjectType::Extension),
        "processing" => Ok(ProjectType::Processing),
        "report" => Ok(ProjectType::Report),
        _ => Err(ProjectConfigError::UnknownProjectType { value }),
    }
}

pub(super) fn parse_source_format(value: String) -> Result<SourceFormat, ProjectConfigError> {
    match value.as_str() {
        DEFAULT_SOURCE_FORMAT => Ok(SourceFormat::DesignerXml),
        _ => Err(ProjectConfigError::UnknownSourceFormat { value }),
    }
}

pub(super) const fn project_type_name(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Configuration => "configuration",
        ProjectType::Extension => "extension",
        ProjectType::Processing => "processing",
        ProjectType::Report => "report",
    }
}

pub(super) const fn source_format_name(source_format: SourceFormat) -> &'static str {
    match source_format {
        SourceFormat::DesignerXml => DEFAULT_SOURCE_FORMAT,
    }
}
