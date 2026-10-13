//! A dialog is derived from its declared form YAML, never a second editable payload.

use std::{collections::BTreeMap, path::Path};

use crate::project_core::MetadataKind;

use super::MANAGED_FORM_LAYOUT_VERSION;
use crate::project_core::source_format::{
    Error, FileRole, ManagedFormSource, ObjectSource, SourceManifest, SourcePath, files,
};

#[cfg(test)]
mod tests;

/// Full native descriptor identity distinguishes equal form names in unrelated owners.
pub(in crate::project_core::source_format) fn native_path(
    descriptor: &SourcePath,
) -> Result<SourcePath, Error> {
    let prefix = descriptor
        .as_str()
        .strip_suffix(".xml")
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    SourcePath::parse(&format!("{prefix}/Ext/Form.xml"))
}

/// Only declared form contexts can consume a native dialog; orphan XML remains a payload.
fn locations(manifest: &SourceManifest) -> Result<BTreeMap<String, &SourcePath>, Error> {
    manifest
        .descriptor_contexts
        .iter()
        .filter(|context| {
            matches!(
                context.metadata_kind,
                MetadataKind::Form | MetadataKind::CommonForm
            )
        })
        .map(|context| {
            Ok((
                native_path(&context.path)?.as_str().to_ascii_lowercase(),
                &context.path,
            ))
        })
        .collect()
}

/// Convert all declared dialogs before publication, preserving independent BSL/resource bytes.
pub(in crate::project_core::source_format) fn attach(
    input: &Path,
    manifest: &mut SourceManifest,
    sources: &mut BTreeMap<SourcePath, ObjectSource>,
) -> Result<(), Error> {
    let locations = locations(manifest)?
        .into_iter()
        .map(|(native, descriptor)| (native, descriptor.clone()))
        .collect::<BTreeMap<_, _>>();
    let owners = manifest
        .files
        .iter()
        .filter_map(|file| match &file.role {
            FileRole::Descriptor { source, .. } => Some((file.path.clone(), source.clone())),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    for file in &mut manifest.files {
        let Some(descriptor) = locations.get(&file.path.as_str().to_ascii_lowercase()) else {
            continue;
        };
        let owner = owners
            .get(descriptor)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        let object = sources
            .get_mut(owner)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        object.form = Some(
            ManagedFormSource::from_xml(&files::read_text(&input.join(file.path.as_path()))?)
                .map_err(|error| error.in_field(file.path.as_str()))?,
        );
        file.role = FileRole::ManagedForm {
            source: owner.clone(),
            descriptor: descriptor.clone(),
        };
    }
    manifest.schema_version = MANAGED_FORM_LAYOUT_VERSION;
    Ok(())
}

/// The sole shared physical path belongs to one form descriptor and its exact native dialog.
pub(super) fn validate_map(manifest: &SourceManifest) -> Result<(), Error> {
    let locations = locations(manifest)?;
    let owners = manifest
        .files
        .iter()
        .filter_map(|file| match &file.role {
            FileRole::Descriptor { source, .. } => Some((&file.path, source)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    for file in &manifest.files {
        match &file.role {
            FileRole::ManagedForm { source, descriptor } => {
                if manifest.schema_version != MANAGED_FORM_LAYOUT_VERSION
                    || locations
                        .get(&file.path.as_str().to_ascii_lowercase())
                        .copied()
                        != Some(descriptor)
                    || owners.get(descriptor).copied() != Some(source)
                {
                    return Err(
                        Error::root("source_layout_payload_invalid").in_field(file.path.as_str())
                    );
                }
            }
            _ if manifest.schema_version == MANAGED_FORM_LAYOUT_VERSION
                && locations.contains_key(&file.path.as_str().to_ascii_lowercase()) =>
            {
                return Err(
                    Error::root("source_layout_payload_invalid").in_field(file.path.as_str())
                );
            }
            _ => {}
        }
    }
    Ok(())
}

/// Neither an undeclared body nor a missing mapped body may be silently ignored by a consumer.
pub(in crate::project_core::source_format) fn validate_source(
    source: &ObjectSource,
    expected: bool,
) -> Result<(), Error> {
    if source.form.is_some() != expected {
        return Err(Error::root("source_form_schema_invalid"));
    }
    Ok(())
}
