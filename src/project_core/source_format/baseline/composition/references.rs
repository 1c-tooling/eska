//! A supplied reference update must equal the same UUID's verified qualified-name transition.

use std::collections::BTreeMap;

use crate::project_core::source_format::{
    AcceptedBase, Error, MetadataNode, MetadataReference, Namespace, NativeDocument, Operation,
    Property, PropertyValue, Section, XmlFragment, export::Package, fragment, targets,
    validation::scope,
};

/// Native paths and displayed names cannot independently choose the target of a preserved link.
pub(super) fn renames(old: &Package, new: &Package) -> Result<BTreeMap<String, String>, Error> {
    let before = targets::qualified_names(&old.documents)?;
    let after = targets::qualified_names(&new.documents)?;
    Ok(before
        .into_iter()
        .filter_map(|(uuid, name)| {
            let next = after.get(&uuid)?;
            (name != *next && !name.is_empty()).then(|| (name, next.clone()))
        })
        .collect())
}

/// Select only supplied, exact reference substitutions; arbitrary preserved XML stays protected.
pub(super) fn rebind(
    base: &AcceptedBase,
    selected: &mut NativeDocument,
    current: &NativeDocument,
    renames: &BTreeMap<String, String>,
) -> Result<(), Error> {
    rebind_node(
        base,
        &mut selected.descriptor.object,
        &current.descriptor.object,
        &selected.descriptor.xml.namespaces,
        renames,
    )
}

/// Inline nodes retain their original parent and permission while their reference targets move.
fn rebind_node(
    base: &AcceptedBase,
    node: &mut MetadataNode,
    current: &MetadataNode,
    inherited: &[Namespace],
    renames: &BTreeMap<String, String>,
) -> Result<(), Error> {
    let context = scope(inherited, &node.namespaces)?;
    for (before, after) in node.sections.iter_mut().zip(&current.sections) {
        if let (Section::Properties(before), Section::Properties(after)) = (&mut *before, after) {
            for (before, after) in before.iter_mut().zip(after) {
                if let (
                    Property::Preserved { fragment: original },
                    Property::Preserved { fragment: supplied },
                ) = (&mut *before, after)
                    && original != supplied
                    && let Some(updated) = replacement(original, &context, renames)?
                    && updated == *supplied
                {
                    if !base.permission(&node.uuid, Operation::Resource).allowed {
                        return Err(Error::root("source_import_structure_denied")
                            .in_field(node.uuid.as_str()));
                    }
                    *original = updated;
                }
                if let (
                    Property::Typed { name, value },
                    Property::Typed {
                        name: supplied_name,
                        value: supplied,
                    },
                ) = (before, after)
                    && name == supplied_name
                    && let Some(updated) = reference_value(value, renames)?
                    && updated == *supplied
                {
                    if !base
                        .permission(&node.uuid, Operation::Property(*name))
                        .allowed
                    {
                        return Err(Error::root("source_import_structure_denied")
                            .in_field(node.uuid.as_str()));
                    }
                    *value = updated;
                }
            }
        }
        if let (Section::Children(before), Section::Children(after)) = (before, after) {
            for child in before {
                if let crate::project_core::source_format::Child::Inline { object } = child
                    && let Some(current) = after.iter().find_map(|child| {
                        if let crate::project_core::source_format::Child::Inline { object: next } =
                            child
                        {
                            (object.uuid == next.uuid).then_some(next.as_ref())
                        } else {
                            None
                        }
                    })
                {
                    rebind_node(base, object, current, &context, renames)?;
                }
            }
        }
    }
    Ok(())
}

/// Whole text values may name a metadata target or one method of a renamed common module.
fn replacement(
    original: &XmlFragment,
    context: &[Namespace],
    renames: &BTreeMap<String, String>,
) -> Result<Option<XmlFragment>, Error> {
    let text = fragment::wrap(original, context)?;
    let document = roxmltree::Document::parse_with_options(
        &text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|_| Error::root("source_xml_invalid"))?;
    let start = text
        .find('>')
        .ok_or_else(|| Error::root("source_xml_invalid"))?
        + 1;
    let mut patches = Vec::new();
    for node in document.descendants().filter(roxmltree::Node::is_text) {
        let value = node.text().unwrap_or_default();
        for (old, new) in renames {
            let Some(suffix) = value.strip_prefix(old) else {
                continue;
            };
            if !(suffix.is_empty()
                || (old.starts_with("CommonModule.")
                    && suffix.strip_prefix('.').is_some_and(|method| {
                        !method.is_empty()
                            && method
                                .chars()
                                .all(|character| character.is_alphanumeric() || character == '_')
                    })))
            {
                continue;
            }
            let range = node.range();
            if range.start >= start
                && range.end <= start + original.xml.len()
                && &text[range.clone()] == value
            {
                patches.push((
                    range.start - start..range.end - start,
                    format!("{new}{suffix}"),
                ));
            }
            break;
        }
    }
    if patches.is_empty() {
        return Ok(None);
    }
    let mut updated = original.clone();
    for (range, value) in patches.into_iter().rev() {
        updated.xml.replace_range(range, &value);
    }
    Ok(Some(updated))
}

/// Typed single/list references preserve their original native recipe and verified UUID targets.
fn reference_value(
    original: &PropertyValue,
    renames: &BTreeMap<String, String>,
) -> Result<Option<PropertyValue>, Error> {
    let mut updated = original.clone();
    match &mut updated {
        PropertyValue::Reference(value) => {
            if let Some(next) = renames.get(value.as_str()) {
                *value = MetadataReference::parse(next)?;
            }
        }
        PropertyValue::ReferenceList(list) => {
            for value in &mut list.items {
                if let Some(next) = renames.get(value.as_str()) {
                    *value = MetadataReference::parse(next)?;
                }
            }
        }
        _ => return Ok(None),
    }
    Ok((updated != *original).then_some(updated))
}
