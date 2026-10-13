//! Current dialogs and accepted dialogs share conversion rules, never input authority or trust pins.

use std::collections::BTreeMap;

use super::{Projection, cst, fingerprint};
use crate::project_core::source_format::{
    Error, FileRole, MANAGED_FORM_LAYOUT_VERSION, ManagedFormSource, SourcePath,
    export::Package,
    files,
    layout::{forms, resources},
    package::manifest_text,
};

#[cfg(test)]
mod tests;

/// Attach only declared native dialogs while retaining owner YAML, native fingerprints and other payload bytes.
pub(super) fn project(
    package: &Package,
    projection: &mut Projection,
    mut read: impl FnMut(&SourcePath) -> Result<String, Error>,
) -> Result<(), Error> {
    let locations = forms::bindings(&projection.manifest)?;
    let owners = projection
        .manifest
        .files
        .iter()
        .filter_map(|file| {
            if let FileRole::Descriptor { source, .. } = &file.role {
                Some((file.path.clone(), source.clone()))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();
    for file in &mut projection.manifest.files {
        let Some(descriptor) = locations.get(&file.path) else {
            continue;
        };
        let owner = owners
            .get(descriptor)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        let input = package
            .payload_sources
            .get(&file.path)
            .ok_or_else(|| Error::root("source_file_missing"))?;
        let body = ManagedFormSource::from_xml(&read(input)?)
            .map_err(|error| error.in_field(input.as_str()))?;
        let original = projection
            .text
            .get(owner)
            .cloned()
            .map_or_else(|| read(owner), Ok)?;
        let updated = cst::attach_form(&original, &body)?;
        let payload = file.source_path().clone();
        projection
            .snapshot
            .files
            .remove(&payload)
            .ok_or_else(|| Error::root("source_file_missing"))?;
        projection.renamed.remove(&payload);
        projection
            .snapshot
            .files
            .insert(owner.clone(), fingerprint(owner, &updated));
        projection.text.insert(owner.clone(), updated);
        file.role = FileRole::ManagedForm {
            source: owner.clone(),
            descriptor: descriptor.clone(),
        };
    }
    projection.manifest.schema_version = MANAGED_FORM_LAYOUT_VERSION;
    projection.manifest.validate()?;
    let marker = SourcePath::parse(files::MANIFEST)?;
    let text = manifest_text(&projection.manifest)?;
    projection
        .snapshot
        .files
        .insert(marker.clone(), fingerprint(&marker, &text));
    projection.text.insert(marker, text);
    projection.snapshot.directories = resources::source_directories(&projection.manifest)?;
    projection.snapshot.revision =
        files::revision(&projection.snapshot.files, &projection.snapshot.directories);
    Ok(())
}
