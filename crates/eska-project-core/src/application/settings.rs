//! Validated application declarations; membership and paths come from project discovery.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::project::{ProjectName, ProjectType, WorkspaceMember, build::PlatformVersion};

use super::{
    SourceRole, graph,
    policy::{RawTools, ToolSettings},
};

/// An analysis target accepts upstream's release line or exact platform build.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct TargetPlatformVersion(String);

impl TargetPlatformVersion {
    /// Validate a three- or four-component numeric platform target without selecting a fallback.
    ///
    /// # Errors
    /// Returns the rejected value when its shape or numeric components are invalid.
    pub fn parse(value: &str) -> Result<Self, SettingsError> {
        if !matches!(value.split('.').count(), 3 | 4)
            || value.split('.').any(|part| {
                part.is_empty()
                    || !part.bytes().all(|byte| byte.is_ascii_digit())
                    || part.parse::<u32>().is_err()
            })
        {
            return Err(SettingsError::InvalidPlatformVersion {
                value: value.into(),
            });
        }
        Ok(Self(value.into()))
    }

    /// Return the exact configured target, including an optional build component.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&PlatformVersion> for TargetPlatformVersion {
    /// Every validated build version already satisfies the four-component target shape.
    fn from(value: &PlatformVersion) -> Self {
        Self(value.as_str().into())
    }
}

/// Root analysis selection; an application-specific target belongs to its declaration.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize)]
pub struct AnalysisOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    default_application: Option<ProjectName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_platform_version: Option<TargetPlatformVersion>,
}

impl AnalysisOptions {
    /// Report whether serializing this table would add any settings.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.default_application.is_none() && self.target_platform_version.is_none()
    }

    /// Return an explicitly selected default, never a guessed neighboring member.
    #[must_use]
    pub const fn default_application(&self) -> Option<&ProjectName> {
        self.default_application.as_ref()
    }

    /// Return the target override used by a standalone context.
    #[must_use]
    pub const fn target_platform_version(&self) -> Option<&TargetPlatformVersion> {
        self.target_platform_version.as_ref()
    }
}

/// One explicitly declared application; all references are portable member names.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct Application {
    configuration: ProjectName,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    extensions: Vec<ProjectName>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    external_objects: Vec<ProjectName>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    extension_dependencies: BTreeMap<ProjectName, Vec<ProjectName>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_platform_version: Option<TargetPlatformVersion>,
}

impl Application {
    /// Return the main configuration's workspace member name.
    #[must_use]
    pub const fn configuration(&self) -> &ProjectName {
        &self.configuration
    }

    /// Preserve the declaration order independently of dependency ordering.
    #[must_use]
    pub fn extensions(&self) -> &[ProjectName] {
        &self.extensions
    }

    /// Return external processing/report members explicitly included in this context.
    #[must_use]
    pub fn external_objects(&self) -> &[ProjectName] {
        &self.external_objects
    }

    /// Return declared extension dependencies without inferring relations from paths.
    #[must_use]
    pub const fn extension_dependencies(&self) -> &BTreeMap<ProjectName, Vec<ProjectName>> {
        &self.extension_dependencies
    }

    /// Return the application's deliberate override of its main configuration's build version.
    #[must_use]
    pub const fn target_platform_version(&self) -> Option<&TargetPlatformVersion> {
        self.target_platform_version.as_ref()
    }

    /// Convert raw strings once, before either manifest consumers or the resolver can use them.
    fn parse(name: &ProjectName, raw: RawApplication) -> Result<Self, SettingsError> {
        let result = Self {
            configuration: member_name(raw.configuration, "configuration")?,
            extensions: member_names(raw.extensions, "extensions")?,
            external_objects: member_names(raw.external_objects, "external_objects")?,
            extension_dependencies: raw
                .extension_dependencies
                .into_iter()
                .map(|(key, values)| {
                    Ok((
                        member_name(key, "extension_dependencies")?,
                        member_names(values, "extension_dependencies")?,
                    ))
                })
                .collect::<Result<_, SettingsError>>()?,
            target_platform_version: raw
                .target_platform_version
                .as_deref()
                .map(TargetPlatformVersion::parse)
                .transpose()?,
        };
        let mut unique = BTreeSet::new();
        for member in std::iter::once(&result.configuration)
            .chain(&result.extensions)
            .chain(&result.external_objects)
        {
            if !unique.insert(member) {
                return Err(SettingsError::DuplicateMember {
                    application: name.clone(),
                    member: member.clone(),
                });
            }
        }
        graph::ordered_extensions(name, &result)?;
        Ok(result)
    }

    /// Check roles against resolved manifests instead of directory or XML filename conventions.
    fn validate_members(
        &self,
        application: &ProjectName,
        members: &[WorkspaceMember],
    ) -> Result<(), SettingsError> {
        for (name, expected) in std::iter::once((&self.configuration, SourceRole::Configuration))
            .chain(
                self.extensions
                    .iter()
                    .map(|name| (name, SourceRole::Extension)),
            )
            .chain(
                self.external_objects
                    .iter()
                    .map(|name| (name, SourceRole::ExternalObject)),
            )
        {
            let member = members
                .iter()
                .find(|member| member.name() == name)
                .ok_or_else(|| SettingsError::UnknownMember {
                    application: application.clone(),
                    member: name.clone(),
                })?;
            let actual = member.project().configuration().project_type();
            let valid = expected.project_types().contains(&actual);
            if !valid {
                return Err(SettingsError::WrongMemberType {
                    application: application.clone(),
                    member: name.clone(),
                    expected,
                    actual,
                });
            }
        }
        Ok(())
    }
}

/// Validated root selection, application declarations and policy data, without analyzer processes.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct AnalysisSettings {
    options: AnalysisOptions,
    applications: BTreeMap<ProjectName, Application>,
    tools: ToolSettings,
}

impl AnalysisSettings {
    /// Return the root-owned policy without starting or linking an analyzer runtime.
    #[must_use]
    pub const fn tools(&self) -> &ToolSettings {
        &self.tools
    }
    /// Return portable root selection options.
    #[must_use]
    pub const fn options(&self) -> &AnalysisOptions {
        &self.options
    }

    /// Return declarations in a deterministic, locale-independent order.
    #[must_use]
    pub const fn applications(&self) -> &BTreeMap<ProjectName, Application> {
        &self.applications
    }

    /// Detect settings that would otherwise be silently ignored in a workspace member.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.options.is_empty() && self.applications.is_empty() && self.tools.is_empty()
    }

    /// Validate the strict TOML fields before constructing the shared model.
    pub(crate) fn parse(
        raw: Option<RawAnalysis>,
        applications: BTreeMap<String, RawApplication>,
        tools: Option<RawTools>,
        workspace: bool,
    ) -> Result<Self, SettingsError> {
        if !workspace && !applications.is_empty() {
            return Err(SettingsError::WorkspaceRequired);
        }
        let raw = raw.unwrap_or_default();
        let options = AnalysisOptions {
            default_application: raw
                .default_application
                .map(|value| member_name(value, "analysis.default_application"))
                .transpose()?,
            target_platform_version: raw
                .target_platform_version
                .as_deref()
                .map(TargetPlatformVersion::parse)
                .transpose()?,
        };
        let applications = applications
            .into_iter()
            .map(|(name, raw)| {
                let name = member_name(name, "applications")?;
                let application = Application::parse(&name, raw)?;
                Ok((name, application))
            })
            .collect::<Result<BTreeMap<_, _>, SettingsError>>()?;
        if let Some(name) = options
            .default_application
            .as_ref()
            .filter(|name| !applications.contains_key(*name))
        {
            return Err(SettingsError::UnknownDefaultApplication { name: name.clone() });
        }
        if !applications.is_empty() && options.target_platform_version.is_some() {
            return Err(SettingsError::TargetPlatformScope);
        }
        Ok(Self {
            options,
            applications,
            tools: ToolSettings::parse(tools)?,
        })
    }

    /// Validate all declarations without reading BSL or selecting an active application.
    pub(crate) fn validate_workspace(
        &self,
        members: &[WorkspaceMember],
    ) -> Result<(), SettingsError> {
        for member in members {
            if !member
                .project()
                .configuration()
                .analysis_settings()
                .is_empty()
            {
                return Err(SettingsError::MemberAnalysisUnsupported {
                    member: member.name().clone(),
                });
            }
        }
        for (name, application) in &self.applications {
            application.validate_members(name, members)?;
        }
        Ok(())
    }
}

/// Raw schema is private to manifest loading; public settings always retain validated names.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawAnalysis {
    default_application: Option<String>,
    target_platform_version: Option<String>,
}

/// An application has one required base and explicit optional members and dependency edges.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawApplication {
    configuration: String,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    external_objects: Vec<String>,
    #[serde(default)]
    extension_dependencies: BTreeMap<String, Vec<String>>,
    target_platform_version: Option<String>,
}

/// Structured configuration errors contain machine-facing values, never localized sentences.
#[derive(Debug, Eq, PartialEq)]
pub enum SettingsError {
    ManagedAnalyzerField {
        field: String,
    },
    InvalidAnalyzerPolicy,
    InvalidName {
        field: &'static str,
        value: String,
    },
    InvalidPlatformVersion {
        value: String,
    },
    WorkspaceRequired,
    UnknownDefaultApplication {
        name: ProjectName,
    },
    DuplicateMember {
        application: ProjectName,
        member: ProjectName,
    },
    DependencyOutsideApplication {
        application: ProjectName,
        member: ProjectName,
    },
    DependencyCycle {
        application: ProjectName,
    },
    UnknownMember {
        application: ProjectName,
        member: ProjectName,
    },
    WrongMemberType {
        application: ProjectName,
        member: ProjectName,
        expected: SourceRole,
        actual: ProjectType,
    },
    MemberAnalysisUnsupported {
        member: ProjectName,
    },
    TargetPlatformScope,
}

/// Reject a non-portable identifier at the field that introduced it.
fn member_name(value: String, field: &'static str) -> Result<ProjectName, SettingsError> {
    ProjectName::parse(value).map_err(|error| SettingsError::InvalidName {
        field,
        value: error.value().into(),
    })
}

/// Preserve the declared order while validating every member reference.
fn member_names(
    values: Vec<String>,
    field: &'static str,
) -> Result<Vec<ProjectName>, SettingsError> {
    values
        .into_iter()
        .map(|value| member_name(value, field))
        .collect()
}
