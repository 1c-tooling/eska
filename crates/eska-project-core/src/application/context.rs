//! Resolve one application from checked manifests; only selected root descriptors are read.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::project::{
    Project, ProjectName, ProjectType, Workspace, WorkspaceMember,
    designer_source::{self, SourceError},
    discovery::{self, ContextDiscoveryError, DiscoveryContext},
    metadata_model::{MetadataValue, ProjectScope},
    metadata_parser::{self, PropertiesMode},
};

use super::{
    AnalysisSettings, AnalyzerPolicy, Application, SettingsError, TargetPlatformVersion, graph,
};

/// The explicit role of one selected source directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceRole {
    Configuration,
    Extension,
    ExternalObject,
}

impl SourceRole {
    /// Return the manifest project types that can occupy this role.
    #[must_use]
    pub const fn project_types(self) -> &'static [ProjectType] {
        match self {
            Self::Configuration => &[ProjectType::Configuration],
            Self::Extension => &[ProjectType::Extension],
            Self::ExternalObject => &[ProjectType::Processing, ProjectType::Report],
        }
    }
}

/// Resolved metadata identity and source settings, without BSL text or a language index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextSource {
    member: Option<ProjectName>,
    role: SourceRole,
    project: Project,
    descriptor: PathBuf,
    metadata_name: String,
    uuid: String,
    compatibility_mode: Option<String>,
}

impl ContextSource {
    /// Return the workspace member identity, when this is not a standalone project.
    #[must_use]
    pub const fn member(&self) -> Option<&ProjectName> {
        self.member.as_ref()
    }

    /// Return the validated source role in this context.
    #[must_use]
    pub const fn role(&self) -> SourceRole {
        self.role
    }

    /// Return the project with canonical roots and effective inherited build settings.
    #[must_use]
    pub const fn project(&self) -> &Project {
        &self.project
    }

    /// Return the actual root descriptor relative to project.source, including EPF/ERF names.
    #[must_use]
    pub fn descriptor(&self) -> &Path {
        &self.descriptor
    }

    /// Return the Designer name, which is independent of the member and directory names.
    #[must_use]
    pub fn metadata_name(&self) -> &str {
        &self.metadata_name
    }

    /// Return the descriptor's identity without deriving it from its filename.
    #[must_use]
    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    /// Preserve the XML compatibility mode; an absent mode remains unknown.
    #[must_use]
    pub fn compatibility_mode(&self) -> Option<&str> {
        self.compatibility_mode.as_deref()
    }
}

/// One selected context shared by CLI, configuration generation and the editor backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisContext {
    root: PathBuf,
    name: Option<ProjectName>,
    sources: Vec<ContextSource>,
    extension_dependencies: BTreeMap<ProjectName, Vec<ProjectName>>,
    target_platform_version: Option<TargetPlatformVersion>,
    analyzer_policy: AnalyzerPolicy,
}

impl AnalysisContext {
    /// Return the policy selected from the same owning manifest as the source composition.
    #[must_use]
    pub const fn analyzer_policy(&self) -> &AnalyzerPolicy {
        &self.analyzer_policy
    }
    /// Return the canonical root that owns the derived analyzer configuration.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Return the selected application or implicit single workspace member name.
    #[must_use]
    pub const fn name(&self) -> Option<&ProjectName> {
        self.name.as_ref()
    }

    /// Return base first, dependency-ordered extensions next, then explicit external objects.
    #[must_use]
    pub fn sources(&self) -> &[ContextSource] {
        &self.sources
    }

    /// Return only explicitly declared extension edges for the selected application.
    #[must_use]
    pub const fn extension_dependencies(&self) -> &BTreeMap<ProjectName, Vec<ProjectName>> {
        &self.extension_dependencies
    }

    /// Distinguish a valid standalone extension/EPF/ERF from a context with a base configuration.
    #[must_use]
    pub fn limited_context(&self) -> bool {
        !self
            .sources
            .iter()
            .any(|source| source.role == SourceRole::Configuration)
    }

    /// Return the deliberate override or effective build version of the main source.
    #[must_use]
    pub const fn target_platform_version(&self) -> Option<&TargetPlatformVersion> {
        self.target_platform_version.as_ref()
    }
}

/// Context failures retain selections and source errors without claiming successful analysis.
#[derive(Debug)]
pub enum ContextError {
    Discovery(Box<ContextDiscoveryError>),
    Source(Box<SourceError>),
    Settings(Box<SettingsError>),
    UnknownApplication {
        requested: String,
        available: Vec<ProjectName>,
    },
    AmbiguousApplication {
        available: Vec<ProjectName>,
    },
    ApplicationDeclarationsMissing {
        members: Vec<ProjectName>,
    },
    EmptyWorkspace,
}

/// Resolve a context without writing files, running Git/platform tools or loading BSL modules.
///
/// # Errors
/// Rejects invalid manifests, ambiguous selection, mismatched members and invalid root XML.
pub fn resolve(start: &Path, requested: Option<&str>) -> Result<AnalysisContext, ContextError> {
    match discovery::discover_context(start)
        .map_err(|error| ContextError::Discovery(Box::new(error)))?
    {
        DiscoveryContext::Standalone(project) => {
            if let Some(requested) = requested {
                return Err(ContextError::UnknownApplication {
                    requested: requested.into(),
                    available: Vec::new(),
                });
            }
            let settings = project.configuration().analysis_settings();
            single(project.root(), None, &project, settings)
        }
        DiscoveryContext::Workspace { workspace, .. } => {
            let settings = workspace.analysis_settings();
            if settings.applications().is_empty() {
                return single_member(&workspace, requested);
            }
            let (name, application) = choose_application(settings, requested)?;
            application_context(&workspace, name, application)
        }
    }
}

/// Keep standalone projects and a workspace's sole member valid without inventing a base.
fn single(
    root: &Path,
    member: Option<&ProjectName>,
    project: &Project,
    settings: &AnalysisSettings,
) -> Result<AnalysisContext, ContextError> {
    let source = read_source(
        member,
        project,
        role(project.configuration().project_type()),
    )?;
    Ok(AnalysisContext {
        root: root.into(),
        name: member.cloned(),
        sources: vec![source],
        extension_dependencies: BTreeMap::new(),
        target_platform_version: target(settings.options().target_platform_version(), project),
        analyzer_policy: settings.tools().bsl_analyzer.clone(),
    })
}

/// Multiple independent members require an explicit composition, even if one is a configuration.
fn single_member(
    workspace: &Workspace,
    requested: Option<&str>,
) -> Result<AnalysisContext, ContextError> {
    let member = match workspace.members() {
        [] => return Err(ContextError::EmptyWorkspace),
        [member] => member,
        members => {
            return Err(ContextError::ApplicationDeclarationsMissing {
                members: members.iter().map(|member| member.name().clone()).collect(),
            });
        }
    };
    if let Some(requested) = requested.filter(|requested| *requested != member.name().as_str()) {
        return Err(ContextError::UnknownApplication {
            requested: requested.into(),
            available: vec![member.name().clone()],
        });
    }
    single(
        workspace.root(),
        Some(member.name()),
        member.project(),
        workspace.analysis_settings(),
    )
}

/// A sole declaration is unambiguous; more than one requires an explicit default or selection.
fn choose_application<'a>(
    settings: &'a AnalysisSettings,
    requested: Option<&str>,
) -> Result<(&'a ProjectName, &'a Application), ContextError> {
    let applications = settings.applications();
    let name = requested.or_else(|| {
        settings
            .options()
            .default_application()
            .map(ProjectName::as_str)
    });
    if let Some(name) = name {
        return ProjectName::parse(name.into())
            .ok()
            .and_then(|name| applications.get_key_value(&name))
            .ok_or_else(|| ContextError::UnknownApplication {
                requested: name.into(),
                available: applications.keys().cloned().collect(),
            });
    }
    if applications.len() == 1
        && let Some(application) = applications.first_key_value()
    {
        return Ok(application);
    }
    Err(ContextError::AmbiguousApplication {
        available: applications.keys().cloned().collect(),
    })
}

/// Build the selected application's source set without importing unrelated workspace members.
fn application_context(
    workspace: &Workspace,
    name: &ProjectName,
    application: &Application,
) -> Result<AnalysisContext, ContextError> {
    let base = member(workspace, name, application.configuration())?;
    let mut sources = vec![read_source(
        Some(base.name()),
        base.project(),
        SourceRole::Configuration,
    )?];
    for extension in graph::ordered_extensions(name, application)
        .map_err(|error| ContextError::Settings(Box::new(error)))?
    {
        let member = member(workspace, name, &extension)?;
        sources.push(read_source(
            Some(member.name()),
            member.project(),
            SourceRole::Extension,
        )?);
    }
    for external in application.external_objects() {
        let member = member(workspace, name, external)?;
        sources.push(read_source(
            Some(member.name()),
            member.project(),
            SourceRole::ExternalObject,
        )?);
    }
    Ok(AnalysisContext {
        root: workspace.root().into(),
        name: Some(name.clone()),
        sources,
        extension_dependencies: application.extension_dependencies().clone(),
        target_platform_version: target(application.target_platform_version(), base.project()),
        analyzer_policy: workspace.analysis_settings().tools().bsl_analyzer.clone(),
    })
}

/// Retain a structured failure even when a caller passes a stale or manually assembled workspace.
fn member<'a>(
    workspace: &'a Workspace,
    application: &ProjectName,
    name: &ProjectName,
) -> Result<&'a WorkspaceMember, ContextError> {
    workspace.member(name).ok_or_else(|| {
        ContextError::Settings(Box::new(SettingsError::UnknownMember {
            application: application.clone(),
            member: name.clone(),
        }))
    })
}

/// Analysis overrides are independent of build selection and never replace XML compatibility.
fn target(
    override_version: Option<&TargetPlatformVersion>,
    project: &Project,
) -> Option<TargetPlatformVersion> {
    override_version.cloned().or_else(|| {
        project
            .configuration()
            .build_settings()
            .platform_version()
            .map(TargetPlatformVersion::from)
    })
}

/// Project type remains authoritative for standalone sources with limited context.
const fn role(project_type: ProjectType) -> SourceRole {
    match project_type {
        ProjectType::Configuration => SourceRole::Configuration,
        ProjectType::Extension => SourceRole::Extension,
        ProjectType::Processing | ProjectType::Report => SourceRole::ExternalObject,
    }
}

/// Share containment/type/identity validation with ordinary Designer source opening.
fn read_source(
    name: Option<&ProjectName>,
    project: &Project,
    role: SourceRole,
) -> Result<ContextSource, ContextError> {
    let scope = name.map_or(ProjectScope::Standalone, |name| {
        ProjectScope::Member(name.clone())
    });
    let source = designer_source::open_project(project.clone(), scope, false)
        .map_err(|error| ContextError::Source(Box::new(error)))?;
    let path = source.descriptor().to_owned();
    let input = source
        .read_xml(&path)
        .map_err(|error| ContextError::Source(Box::new(error)))?
        .ok_or_else(|| ContextError::Source(Box::new(SourceError::MissingRoot)))?;
    let parsed = metadata_parser::parse(&input, None, PropertiesMode::All).map_err(|error| {
        ContextError::Source(Box::new(SourceError::Parse {
            path: project.source().join(&path),
            source: error,
        }))
    })?;
    let field = if project.configuration().project_type() == ProjectType::Extension {
        "ConfigurationExtensionCompatibilityMode"
    } else {
        "CompatibilityMode"
    };
    let root = parsed
        .objects
        .iter()
        .find(|object| &object.metadata == source.root())
        .ok_or_else(|| invalid_root(project, &path, "root_identity_changed"))?;
    let compatibility_mode = compatibility(root, field)
        .map_err(|()| invalid_root(project, &path, "invalid_compatibility_mode"))?;
    Ok(ContextSource {
        member: name.cloned(),
        role,
        project: project.clone(),
        descriptor: path,
        metadata_name: source.root().name().into(),
        uuid: source.root().uuid().into(),
        compatibility_mode,
    })
}

/// Unknown namespaces cannot supply platform properties; duplicate or structured modes are invalid.
fn compatibility(root: &metadata_parser::ParsedObject, field: &str) -> Result<Option<String>, ()> {
    let mut values = root.properties.iter().flatten().filter(|property| {
        property.property.key.name == field
            && property.property.key.namespace.as_deref() == Some("http://v8.1c.ru/8.3/MDClasses")
    });
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    match &value.property.value {
        MetadataValue::Text(text)
        | MetadataValue::QualifiedText { text, .. }
        | MetadataValue::TypedText { text, .. } => Ok(Some(text.clone())),
        _ => Err(()),
    }
}

/// Report inconsistent root metadata without reusing a previously successful context.
fn invalid_root(project: &Project, path: &Path, reason: &'static str) -> ContextError {
    ContextError::Source(Box::new(SourceError::InvalidMetadata {
        path: project.source().join(path),
        reason,
    }))
}
