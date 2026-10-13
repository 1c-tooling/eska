//! Separated slots use mandatory reverse contexts; current YAML remains the composition authority.

use std::collections::BTreeMap;

use super::{
    Error, FileRole, MetadataKind, NativeUuid, ObjectId, OwnerSnapshot, OwnerSpec, SourceManifest,
    SourcePath, node_name, validate_owner,
};
use crate::project_core::source_format::DescriptorContext;

/// Build pending slots without opening any non-root YAML or opaque resource.
pub(super) fn owners(
    manifest: &SourceManifest,
    configuration: &OwnerSnapshot,
    mut groups: BTreeMap<SourcePath, BTreeMap<SourcePath, NativeUuid>>,
    mut root_spec: OwnerSpec,
) -> Result<BTreeMap<SourcePath, OwnerSpec>, Error> {
    let entries = manifest
        .files
        .iter()
        .filter_map(|file| match &file.role {
            FileRole::Descriptor { source, uuid } => Some((&file.path, (source, uuid))),
            FileRole::Preserved
            | FileRole::Relocated { .. }
            | FileRole::OptionalModule { .. }
            | FileRole::ManagedForm { .. } => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut children: BTreeMap<&SourcePath, Vec<&DescriptorContext>> = BTreeMap::new();
    for context in &manifest.descriptor_contexts {
        if let Some(parent) = &context.parent {
            children.entry(parent).or_default().push(context);
        } else if context.name != node_name(&configuration.source.document.descriptor.object)? {
            return Err(Error::root("source_declaration_mismatch"));
        }
    }
    root_spec.external = Some(external(children.get(&root_spec.main)));
    validate_owner(&configuration.source, &root_spec)?;
    let root_parent = if root_spec.kind == MetadataKind::Configuration {
        None
    } else {
        Some(ObjectId::from_parts(
            None,
            root_spec.kind.as_str(),
            node_name(&configuration.source.document.descriptor.object)?.trim(),
        ))
    };
    let mut pending = children
        .get(&root_spec.main)
        .into_iter()
        .flatten()
        .map(|context| (*context, root_parent.clone()))
        .collect::<Vec<_>>();
    let mut result = BTreeMap::new();
    while let Some((context, logical_parent)) = pending.pop() {
        let (source, _) = entries
            .get(&context.path)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        let parent_uuid = context
            .parent
            .as_ref()
            .and_then(|parent| entries.get(parent))
            .map(|(_, uuid)| (*uuid).clone())
            .ok_or_else(|| Error::root("source_layout_context_invalid"))?;
        let descriptors = groups
            .remove(*source)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        let id = ObjectId::from_parts(
            logical_parent.as_ref(),
            context.metadata_kind.as_str(),
            context.name.trim(),
        );
        if let Some(children) = children.get(&context.path) {
            pending.extend(children.iter().map(|child| (*child, Some(id.clone()))));
        }
        result.insert(
            (*source).clone(),
            OwnerSpec {
                main: context.path.clone(),
                kind: context.metadata_kind,
                name: Some(context.name.clone()),
                parent: Some(parent_uuid),
                logical_parent,
                descriptors,
                rights: None,
                predefined: None,
                external: Some(external(children.get(&context.path))),
            },
        );
    }
    if !groups.is_empty() {
        return Err(Error::root("source_descriptor_undeclared"));
    }
    result.insert(configuration.path.clone(), root_spec);
    Ok(result)
}

/// This index checks the set of targets; declaration order remains in the parent's source text.
fn external(
    children: Option<&Vec<&DescriptorContext>>,
) -> BTreeMap<SourcePath, (MetadataKind, String)> {
    children
        .into_iter()
        .flatten()
        .map(|context| {
            (
                context.path.clone(),
                (context.metadata_kind, context.name.clone()),
            )
        })
        .collect()
}
