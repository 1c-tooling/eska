//! One path declaration supplies both workspace membership and explicit analysis roles.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    application::{
        ConfigurationReference, ExtensionScope, RawApplication, RawConfiguration, SettingsError,
        TargetPlatformVersion,
    },
    project::{ProjectName, WorkspaceMember},
};

use super::workspace::{WorkspaceConfigError, validate_member_path};

/// Applications own the only workspace membership declaration.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MemberLayout(pub BTreeMap<String, ApplicationMembers>);

/// Local paths point to ESKA projects; an explicit source points directly to reference XML.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ConfigurationMember {
    Project(#[serde(serialize_with = "super::paths::one")] PathBuf),
    Reference(ConfigurationReference),
}

/// Keep a member's deployment and dependency settings with its only path declaration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionMember {
    #[serde(serialize_with = "super::paths::one")]
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "ExtensionScope::is_default")]
    pub scope: ExtensionScope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

/// An application's members are declared once, under their semantic roles.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationMembers {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<ConfigurationMember>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ExtensionMember>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[serde(serialize_with = "super::paths::many")]
    pub external_objects: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_platform_version: Option<String>,
}

impl ApplicationMembers {
    /// Enumerate only owned project roots; reference XML never enters workspace operations.
    pub(crate) fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        let configuration = match &self.configuration {
            Some(ConfigurationMember::Project(path)) => Some(path),
            _ => None,
        };
        configuration
            .into_iter()
            .chain(self.extensions.iter().map(|extension| &extension.path))
            .chain(&self.external_objects)
    }

    /// Use discovered project identities so directory names never become semantic identifiers.
    pub(crate) fn resolve(&self, paths: &[PathBuf], members: &[WorkspaceMember]) -> RawApplication {
        let name = |path: &PathBuf| {
            paths
                .iter()
                .zip(members)
                .find(|(candidate, _)| *candidate == path)
                .map_or_else(String::new, |(_, member)| member.name().as_str().to_owned())
        };
        RawApplication {
            configuration: self
                .configuration
                .as_ref()
                .map(|configuration| match configuration {
                    ConfigurationMember::Project(path) => RawConfiguration::Member(name(path)),
                    ConfigurationMember::Reference(reference) => {
                        RawConfiguration::Reference(reference.clone())
                    }
                }),
            extensions: self
                .extensions
                .iter()
                .map(|extension| name(&extension.path))
                .collect(),
            external_objects: self.external_objects.iter().map(name).collect(),
            extension_dependencies: self
                .extensions
                .iter()
                .filter(|extension| !extension.depends_on.is_empty())
                .map(|extension| (name(&extension.path), extension.depends_on.clone()))
                .collect(),
            extension_scopes: self
                .extensions
                .iter()
                .map(|extension| (name(&extension.path), extension.scope))
                .collect(),
            target_platform_version: self.target_platform_version.clone(),
        }
    }
}

impl MemberLayout {
    /// Validate each role path and retain one owned member for paths shared across applications.
    pub(crate) fn paths(&self) -> Result<Vec<PathBuf>, WorkspaceConfigError> {
        let mut paths = Vec::new();
        for application in self.0.values() {
            if let Some(ConfigurationMember::Reference(reference)) = &application.configuration {
                reference.validate().map_err(invalid_analysis)?;
            }
            if let Some(version) = &application.target_platform_version {
                TargetPlatformVersion::parse(version).map_err(invalid_analysis)?;
            }
            for path in application.paths() {
                validate_member_path(path)?;
                if !paths.contains(path) {
                    paths.push(path.clone());
                }
            }
        }
        Ok(paths)
    }

    /// Validate names even before filesystem discovery resolves the path-based declarations.
    pub(crate) fn names(&self) -> Result<Vec<ProjectName>, WorkspaceConfigError> {
        self.0
            .keys()
            .map(|name| {
                ProjectName::parse(name.clone()).map_err(|_| {
                    invalid_analysis(SettingsError::InvalidName {
                        field: "workspace.members",
                        value: name.clone(),
                    })
                })
            })
            .collect()
    }
}

/// Preserve structured settings errors inside the manifest boundary.
fn invalid_analysis(error: SettingsError) -> WorkspaceConfigError {
    WorkspaceConfigError::InvalidAnalysis(Box::new(error))
}
