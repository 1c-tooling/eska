//! Read-only onboarding shares native version authority without extending bump or release behavior.

use crate::project_core::{
    Project, ProjectType, SourceFormat,
    eska_source::EskaSource,
    metadata_model::ProjectScope,
    source_format::{Error, PropertyName, PropertyValue, require_project_ready},
};

use super::{ProjectVersion, VersionError};

/// Read the authoritative version for project onboarding without materializing derivative XML.
///
/// CF/CFE ESKA projects use the typed root YAML property; external objects keep the existing
/// exported-module contract. Designer XML retains ordinary inspection. This getter grants no
/// support permission and does not add a configuration bump or release adapter.
///
/// # Errors
/// Returns unsupported/changed source, missing/invalid version or the external module contract error.
pub fn project_source_version(project: &Project) -> Result<ProjectVersion, VersionError> {
    if !matches!(
        project.configuration().source_format(),
        SourceFormat::Eska { .. }
    ) || matches!(
        project.configuration().project_type(),
        ProjectType::Processing | ProjectType::Report
    ) {
        return super::inspect(project).map(|info| info.version);
    }
    let source = EskaSource::open(project.clone(), ProjectScope::Standalone)
        .map_err(|error| source_error(project, error))?;
    let owner = source.reader().configuration();
    let Some(PropertyValue::Text(value)) = owner
        .source
        .document
        .descriptor
        .object
        .property(PropertyName::Version)
    else {
        return Err(VersionError::VersionMissing {
            path: project.source().join(owner.path.as_path()),
        });
    };
    let version = ProjectVersion::parse(value)?;
    require_project_ready(project.root()).map_err(|error| source_error(project, error))?;
    Ok(version)
}

/// Attach structured source failures to the selected project instead of displaying parser text.
fn source_error(project: &Project, error: Error) -> VersionError {
    VersionError::Source {
        path: project.source().to_owned(),
        error,
    }
}
