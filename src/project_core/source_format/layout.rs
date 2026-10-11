//! Physical descriptor boundaries retain native context independently of authoring filenames.

pub(super) mod migration;

#[cfg(test)]
mod tests;

pub use migration::{
    DescriptorLayoutPlan, LayoutChange, LayoutPublication, prepare_descriptor_layout,
};

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::project_core::MetadataKind;

use super::{
    Error, FileRole, NativeDocument, ObjectSource, SourceManifest, SourcePath,
    composition::{reference_path, references},
    reader::node_name,
};

/// Version of the separated reverse map; descriptor envelopes retain their own schema version.
pub const DESCRIPTOR_LAYOUT_VERSION: u32 = 2;
pub(super) const PENDING_RECEIPT: &str = ".eska-layout.pending.json";
pub(super) const PENDING_WRITE: &str = ".eska-layout.write";

/// Original enclosing context disambiguates native UUIDs without making paths an identity.
///
/// This is provenance for validated analysis slots. Current ordered composition remains in
/// the parent's typed children; each read and complete publication must check those declarations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescriptorContext {
    pub path: SourcePath,
    pub metadata_kind: MetadataKind,
    pub name: String,
    /// Native parent path resolves to its UUID through the same mandatory file map.
    pub parent: Option<SourcePath>,
}

/// Each independently saved native file keeps one authoring document; inline children stay inline.
pub(super) fn separate(
    families: BTreeMap<SourcePath, ObjectSource>,
) -> Result<BTreeMap<SourcePath, ObjectSource>, Error> {
    let mut result = BTreeMap::new();
    for source in families.into_values() {
        for document in std::iter::once(source.document).chain(source.nested) {
            let path = descriptor_path(document.descriptor.object.kind, &document.path)?;
            if result
                .insert(
                    path,
                    ObjectSource {
                        document,
                        nested: Vec::new(),
                    },
                )
                .is_some()
            {
                return Err(Error::root("source_path_collision"));
            }
        }
    }
    Ok(result)
}

/// Reuse native composition validation independently of physical source grouping.
pub(super) fn documents(
    sources: &BTreeMap<SourcePath, ObjectSource>,
) -> BTreeMap<SourcePath, &NativeDocument> {
    sources
        .values()
        .flat_map(|source| std::iter::once(&source.document).chain(&source.nested))
        .map(|document| (document.path.clone(), document))
        .collect()
}

/// Use one physical YAML per independently saved native descriptor, including common resources.
pub(super) fn descriptor_path(
    kind: MetadataKind,
    native: &SourcePath,
) -> Result<SourcePath, Error> {
    if kind == MetadataKind::Configuration {
        return SourcePath::parse("configuration.yaml");
    }
    let base = native
        .as_str()
        .strip_suffix(".xml")
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    let file = match kind {
        MetadataKind::Form | MetadataKind::CommonForm => "form.yaml",
        MetadataKind::Template | MetadataKind::CommonTemplate => "template.yaml",
        MetadataKind::Command | MetadataKind::CommonCommand => "command.yaml",
        _ => "object.yaml",
    };
    SourcePath::parse(&format!("{base}/{file}"))
}

/// Validate complete reverse contexts; neither a directory nor an unmapped file activates metadata.
pub(super) fn validate_map(manifest: &SourceManifest) -> Result<(), Error> {
    if manifest.schema_version != DESCRIPTOR_LAYOUT_VERSION {
        return if manifest.descriptor_contexts.is_empty() {
            Ok(())
        } else {
            Err(Error::root("source_layout_context_invalid"))
        };
    }
    let entries = manifest
        .files
        .iter()
        .filter_map(|file| match &file.role {
            FileRole::Descriptor { source, uuid } => Some((&file.path, (source, uuid))),
            FileRole::Preserved => None,
        })
        .collect::<BTreeMap<_, _>>();
    let contexts = manifest
        .descriptor_contexts
        .iter()
        .map(|context| (&context.path, context))
        .collect::<BTreeMap<_, _>>();
    if contexts.len() != manifest.descriptor_contexts.len() || entries.keys().ne(contexts.keys()) {
        return Err(Error::root("source_layout_context_invalid"));
    }
    let root = SourcePath::parse("Configuration.xml")?;
    let root_context = contexts
        .get(&root)
        .ok_or_else(|| Error::root("source_descriptor_missing"))?;
    if root_context.metadata_kind != MetadataKind::Configuration
        || root_context.parent.is_some()
        || root_context.name.is_empty()
    {
        return Err(Error::root("source_layout_context_invalid"));
    }
    let mut sources = BTreeSet::new();
    let mut uuids = BTreeSet::new();
    for (path, context) in &contexts {
        let (source, uuid) = entries
            .get(path)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        if !sources.insert(source.as_str().to_lowercase())
            || *source != &descriptor_path(context.metadata_kind, path)?
        {
            return Err(Error::root("source_path_collision").in_field(source.as_str()));
        }
        if !uuids.insert(uuid.key()) {
            return Err(Error::root("native_uuid_duplicate"));
        }
        if **path == root {
            continue;
        }
        let parent = context
            .parent
            .as_ref()
            .and_then(|parent| contexts.get(parent))
            .ok_or_else(|| Error::root("source_layout_context_invalid"))?;
        // The native spelling is a verified reverse location, not an alternate UUID identity.
        let base = if parent.metadata_kind == MetadataKind::Configuration {
            ""
        } else {
            parent
                .path
                .as_str()
                .strip_suffix(".xml")
                .ok_or_else(|| Error::root("source_path_invalid"))?
        };
        if reference_path(base, context.metadata_kind, &context.name)? != **path {
            return Err(Error::root("source_layout_context_invalid").in_field(path.as_str()));
        }
    }
    validate_physical_paths(manifest)?;
    // Every non-root reverse path strictly extends its parent's path, so cycles cannot pass.
    Ok(())
}

/// Reverse-map entries must not make one physical file both authoring YAML and opaque payload.
fn validate_physical_paths(manifest: &SourceManifest) -> Result<(), Error> {
    let mut paths = BTreeSet::from(["source.json"]);
    for file in &manifest.files {
        let path = match &file.role {
            FileRole::Descriptor { source, .. } => source.as_str(),
            FileRole::Preserved => file.path.as_str(),
        };
        if !paths.insert(path) {
            return Err(Error::root("source_path_collision").in_field(path));
        }
    }
    let mut spellings = BTreeMap::new();
    let mut physical_files = BTreeSet::new();
    for path in &paths {
        if [PENDING_RECEIPT, PENDING_WRITE].iter().any(|reserved| {
            path.split('/')
                .next()
                .is_some_and(|root| root.eq_ignore_ascii_case(reserved))
        }) || !physical_files.insert(path.to_lowercase())
        {
            return Err(Error::root("source_path_collision").in_field(*path));
        }
        for end in path
            .match_indices('/')
            .map(|(end, _)| end)
            .chain(std::iter::once(path.len()))
        {
            let prefix = &path[..end];
            if spellings
                .insert(prefix.to_lowercase(), prefix)
                .is_some_and(|previous| previous != prefix)
                || end < path.len() && physical_files.contains(&prefix.to_lowercase())
            {
                return Err(Error::root("source_path_collision").in_field(*path));
            }
        }
    }
    for directory in &manifest.directories {
        let path = directory.as_str();
        if [PENDING_RECEIPT, PENDING_WRITE]
            .iter()
            .any(|reserved| path.eq_ignore_ascii_case(reserved))
            || physical_files.contains(&path.to_lowercase())
            || spellings
                .insert(path.to_lowercase(), path)
                .is_some_and(|previous| previous != path)
        {
            return Err(Error::root("source_path_collision").in_field(path));
        }
    }
    Ok(())
}

/// Full package acceptance verifies provenance against current composition, not merely the map.
pub(super) fn validate_documents(
    manifest: &SourceManifest,
    documents: &BTreeMap<SourcePath, NativeDocument>,
) -> Result<(), Error> {
    if manifest.schema_version != DESCRIPTOR_LAYOUT_VERSION {
        return Ok(());
    }
    let mut declared = manifest.descriptor_contexts.clone();
    declared.sort_by(|left, right| left.path.cmp(&right.path));
    let references = documents
        .iter()
        .map(|(path, document)| (path.clone(), document))
        .collect();
    if declared != contexts(&references)? {
        return Err(Error::root("source_declaration_mismatch"));
    }
    Ok(())
}

/// Derive original enclosing contexts by following validated typed composition in declaration order.
pub(super) fn contexts(
    documents: &BTreeMap<SourcePath, &NativeDocument>,
) -> Result<Vec<DescriptorContext>, Error> {
    let mut pending = vec![(SourcePath::parse("Configuration.xml")?, None)];
    let mut result = BTreeMap::new();
    while let Some((path, parent)) = pending.pop() {
        let document = documents
            .get(&path)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        let node = &document.descriptor.object;
        let base = if node.kind == MetadataKind::Configuration {
            ""
        } else {
            path.as_str()
                .strip_suffix(".xml")
                .ok_or_else(|| Error::root("source_path_invalid"))?
        };
        for (kind, name) in references(node)? {
            pending.push((reference_path(base, kind, &name)?, Some(path.clone())));
        }
        if result
            .insert(
                path.clone(),
                DescriptorContext {
                    path,
                    parent,
                    metadata_kind: node.kind,
                    name: node_name(node)?.into(),
                },
            )
            .is_some()
        {
            return Err(Error::root("source_descriptor_duplicate"));
        }
    }
    if result.len() != documents.len() {
        return Err(Error::root("source_descriptor_undeclared"));
    }
    Ok(result.into_values().collect())
}
