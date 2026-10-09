//! Read-only checks for source profiles, output destinations and managed-infobase scope.

use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

use super::{ArtifactType, BuildError, BuildPlan, manifest};
use crate::project_core::{ProjectType, native};

/// Validate every filesystem input and output invariant without creating files or invoking ibcmd.
///
/// # Errors
/// Returns a structured source, descriptor, or output error.
pub fn preflight(plan: &BuildPlan) -> Result<(), BuildError> {
    let parent = plan
        .output()
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| BuildError::OutputParentMissing(plan.output().to_owned()))?;
    if !plan.has_explicit_output() {
        ensure_existing_output_ancestor_is_inside_project(plan, parent)?;
    }
    validate_existing_output(plan.output())?;
    let infobase_parent = plan
        .infobase_root()
        .parent()
        .ok_or_else(|| BuildError::OutputParentMissing(plan.infobase_root().to_owned()))?;
    ensure_existing_output_ancestor_is_inside_project(plan, infobase_parent)?;
    if let Some(path) = plan.base_configuration() {
        let metadata = fs::metadata(path).map_err(|source| BuildError::BaseConfigurationRead {
            path: path.to_owned(),
            source,
        })?;
        let has_cf_extension = path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("cf"));
        if !metadata.is_file() || !has_cf_extension {
            return Err(BuildError::BaseConfigurationInvalid(path.to_owned()));
        }
    }
    import_source(plan).map(|_| ())
}

/// Validate both destinations required by a manifested build without creating files.
///
/// # Errors
/// Returns the same output errors as execution for either the artifact or its manifest.
pub fn preflight_manifest(plan: &BuildPlan) -> Result<(), BuildError> {
    preflight(plan)?;
    validate_existing_output(&manifest::path_for_artifact(plan.output()))
}

/// Resolve the source argument expected by ibcmd for each native artifact kind.
pub(super) fn import_source(plan: &BuildPlan) -> Result<PathBuf, BuildError> {
    if matches!(
        plan.artifact_type(),
        ArtifactType::Configuration | ArtifactType::Extension
    ) {
        fs::read_dir(plan.source()).map_err(|source| BuildError::DescriptorDirectory {
            path: plan.source().to_owned(),
            source,
        })?;
        return Ok(plan.source().to_owned());
    }
    let expected = match plan.artifact_type() {
        ArtifactType::Processing => ProjectType::Processing,
        ArtifactType::Report => ProjectType::Report,
        ArtifactType::Configuration | ArtifactType::Extension => unreachable!(),
    };
    let entries =
        fs::read_dir(plan.source()).map_err(|source| BuildError::DescriptorDirectory {
            path: plan.source().to_owned(),
            source,
        })?;
    let mut descriptors = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| BuildError::DescriptorDirectory {
            path: plan.source().to_owned(),
            source,
        })?;
        let path = entry.path();
        if !entry
            .file_type()
            .map_err(|source| BuildError::DescriptorRead {
                path: path.clone(),
                source,
            })?
            .is_file()
            || path.extension() != Some(OsStr::new("xml"))
        {
            continue;
        }
        let contents = fs::read_to_string(&path).map_err(|source| BuildError::DescriptorRead {
            path: path.clone(),
            source,
        })?;
        let project_type = native::project_type(&contents)
            .map_err(|_| BuildError::DescriptorInvalid { path: path.clone() })?;
        if project_type == Some(expected) {
            descriptors.push(path);
        }
    }
    match descriptors.as_slice() {
        [_descriptor] => Ok(plan.source().to_owned()),
        [] => Err(BuildError::DescriptorMissing(plan.source().to_owned())),
        _ => Err(BuildError::DescriptorsMultiple(plan.source().to_owned())),
    }
}

/// Validate the nearest existing configured-output ancestor before creating directories.
fn ensure_existing_output_ancestor_is_inside_project(
    plan: &BuildPlan,
    parent: &Path,
) -> Result<(), BuildError> {
    let root = fs::canonicalize(plan.output_scope_root()).map_err(|source| {
        BuildError::CreateDirectory {
            path: plan.output_scope_root().to_owned(),
            source,
        }
    })?;
    let mut ancestor = parent;
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| BuildError::OutputParentMissing(parent.to_owned()))?;
    }
    let resolved = fs::canonicalize(ancestor).map_err(|source| BuildError::CreateDirectory {
        path: ancestor.to_owned(),
        source,
    })?;
    if resolved.starts_with(&root) {
        Ok(())
    } else {
        Err(BuildError::ConfiguredOutputOutsideProject {
            root,
            output: resolved,
        })
    }
}

/// Prevent a configured relative artifacts directory from escaping through a symlink.
pub(super) fn ensure_configured_output_is_inside_project(
    plan: &BuildPlan,
    parent: &Path,
) -> Result<(), BuildError> {
    let root = fs::canonicalize(plan.output_scope_root()).map_err(|source| {
        BuildError::CreateDirectory {
            path: plan.output_scope_root().to_owned(),
            source,
        }
    })?;
    let parent = fs::canonicalize(parent).map_err(|source| BuildError::CreateDirectory {
        path: parent.to_owned(),
        source,
    })?;
    if parent.starts_with(&root) {
        Ok(())
    } else {
        Err(BuildError::ConfiguredOutputOutsideProject {
            root,
            output: parent,
        })
    }
}

/// Refuse links and special files as replacement targets.
fn validate_existing_output(output: &Path) -> Result<(), BuildError> {
    match fs::symlink_metadata(output) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(_) => Err(BuildError::InvalidExistingOutput(output.to_owned())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(BuildError::Publish {
            path: output.to_owned(),
            source: error,
        }),
    }
}
