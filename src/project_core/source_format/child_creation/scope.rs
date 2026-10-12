//! Creation retains the accepted parent and its namespace context, including existing tabular sections.

use crate::project_core::{
    MetadataKind,
    source_format::{
        Child, Descriptor, Error, MetadataNode, Namespace, NativeUuid, Section, validation,
    },
};

/// A supported descriptor owns either its root children or the columns of a direct tabular section.
pub(super) fn owner<'a>(descriptor: &'a Descriptor, uuid: &NativeUuid) -> Option<&'a MetadataNode> {
    let root = &descriptor.object;
    if root.uuid.key() == uuid.key() {
        return matches!(
            root.kind,
            MetadataKind::Enum | MetadataKind::Catalog | MetadataKind::Document
        )
        .then_some(root);
    }
    if !matches!(root.kind, MetadataKind::Catalog | MetadataKind::Document) {
        return None;
    }
    inline_children(root)
        .find(|node| node.kind == MetadataKind::TabularSection && node.uuid.key() == uuid.key())
}

/// Namespace shadowing belongs to the real native parent; working service fields cannot replace it.
pub(super) fn context(
    descriptor: &Descriptor,
    owner: &MetadataNode,
) -> Result<Vec<Namespace>, Error> {
    let inherited = validation::scope(&descriptor.xml.namespaces, &descriptor.object.namespaces)?;
    if owner.uuid.key() == descriptor.object.uuid.key() {
        Ok(inherited)
    } else {
        validation::scope(&inherited, &owner.namespaces)
    }
}

/// Source mapping resolves the selected child's direct parent without inventing an accepted child span.
pub(in crate::project_core::source_format) fn created_context(
    descriptor: &Descriptor,
    child: &NativeUuid,
) -> Result<Option<(MetadataKind, Vec<Namespace>)>, Error> {
    let root = &descriptor.object;
    let parent = std::iter::once(root)
        .chain(inline_children(root))
        .find(|node| {
            owner(descriptor, &node.uuid).is_some()
                && inline_children(node).any(|node| node.uuid.key() == child.key())
        });
    let Some(parent) = parent else {
        return Ok(None);
    };
    let Some(kind) = super::child_kind(parent.kind) else {
        return Ok(None);
    };
    Ok(Some((kind, context(descriptor, parent)?)))
}

/// Only direct modeled children carry parentage; references and opaque slots do not become owners.
fn inline_children(node: &MetadataNode) -> impl Iterator<Item = &MetadataNode> {
    node.sections
        .iter()
        .filter_map(|section| {
            if let Section::Children(children) = section {
                Some(children)
            } else {
                None
            }
        })
        .flatten()
        .filter_map(|child| {
            if let Child::Inline { object } = child {
                Some(object.as_ref())
            } else {
                None
            }
        })
}
