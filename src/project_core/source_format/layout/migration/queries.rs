//! Source and pinned query values share extraction rules while retaining independent input authority.

use std::collections::{BTreeMap, BTreeSet};

use super::{Projection, cst, fingerprint};
use crate::project_core::{
    MetadataKind,
    source_format::{
        Error, FileRole, ObjectSource, SourcePath,
        export::Package,
        files,
        layout::{queries, resources},
        package::manifest_text,
    },
};

#[cfg(test)]
mod tests;

/// Only query owners are decoded again; unrelated YAML and opaque payloads retain their exact bytes.
pub(super) fn project(
    package: &Package,
    projection: &mut Projection,
    mut read: impl FnMut(&SourcePath) -> Result<String, Error>,
) -> Result<(), Error> {
    let owners = query_owners(&projection.manifest)?;
    let mut originals = BTreeMap::new();
    let mut sources = BTreeMap::new();
    for owner in owners {
        let text = match projection.text.get(&owner) {
            Some(text) => text.clone(),
            None => read(&owner)?,
        };
        sources.insert(owner.clone(), ObjectSource::from_yaml(&text)?);
        originals.insert(owner, text);
    }
    let extra = queries::extract(&mut projection.manifest, &mut sources, |file| {
        let input = package
            .payload_sources
            .get(&file.path)
            .ok_or_else(queries::invalid)?;
        read(input)
    })?;
    for (owner, source) in sources {
        let original = originals.get(&owner).ok_or_else(queries::invalid)?;
        let previous = ObjectSource::from_yaml(original)?;
        if previous == source {
            continue;
        }
        let updated = cst::replace_form_queries(
            original,
            previous.form.as_ref().ok_or_else(queries::invalid)?,
            source.form.as_ref().ok_or_else(queries::invalid)?,
        )?;
        if ObjectSource::from_yaml(&updated)? != source {
            return Err(queries::invalid());
        }
        projection
            .snapshot
            .files
            .insert(owner.clone(), fingerprint(&owner, &updated));
        projection.text.insert(owner, updated);
    }
    let resources = projection
        .manifest
        .query_resources
        .iter()
        .filter_map(|resource| {
            projection
                .manifest
                .files
                .iter()
                .find(|file| file.path == resource.path)
                .filter(|file| file.is_payload())
                .map(|file| file.source_path().clone())
        })
        .collect::<BTreeSet<_>>();
    for (path, text) in extra {
        if projection.snapshot.files.contains_key(&path) && !resources.contains(&path) {
            return Err(Error::root("source_path_collision").in_field(path.as_str()));
        }
        projection.renamed.remove(&path);
        projection
            .snapshot
            .files
            .insert(path.clone(), fingerprint(&path, &text));
        projection.text.insert(path, text);
    }
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

/// Descriptor context selects form/template authoring owners without repeated full file scans.
fn query_owners(
    manifest: &crate::project_core::source_format::SourceManifest,
) -> Result<BTreeSet<SourcePath>, Error> {
    let descriptor_owners = manifest
        .files
        .iter()
        .filter_map(|file| {
            if let FileRole::Descriptor { source, .. } = &file.role {
                Some((&file.path, source))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();
    let owners = manifest
        .descriptor_contexts
        .iter()
        .filter(|context| {
            matches!(
                context.metadata_kind,
                MetadataKind::Form
                    | MetadataKind::CommonForm
                    | MetadataKind::Template
                    | MetadataKind::CommonTemplate
            )
        })
        .map(|context| {
            descriptor_owners
                .get(&context.path)
                .map(|owner| (*owner).clone())
                .ok_or_else(queries::invalid)
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(owners)
}
