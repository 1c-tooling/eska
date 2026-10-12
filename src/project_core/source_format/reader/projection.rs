//! Typed facts and exact CST coordinates in native section order; no intermediate XML.

use std::{collections::BTreeMap, ops::Range};

use yaml_edit::YamlFile;

use super::{MetadataDeclaration, node_name};
use crate::project_core::source_format::{
    Child, Error, MetadataNode, NativeUuid, ObjectSource, Property, PropertyName, Section,
    SourcePath, document_paths,
    yaml::{Step, range_at},
};

/// Flatten this owner in source order, preserving logical parents across external descriptors.
pub(super) fn declarations(
    source: &ObjectSource,
    tree: &YamlFile,
    parents: &BTreeMap<SourcePath, Option<NativeUuid>>,
) -> Result<Vec<MetadataDeclaration>, Error> {
    let mut result = Vec::new();
    for (index, part) in std::iter::once(&source.document)
        .chain(&source.nested)
        .enumerate()
    {
        let mut steps = document_paths::root_steps(index);
        let parent = parents
            .get(&part.path)
            .ok_or_else(|| Error::root("source_descriptor_undeclared"))?;
        collect(
            &part.descriptor.object,
            parent.as_ref(),
            &part.path,
            part.descriptor.schema_version,
            tree,
            &mut steps,
            &mut result,
        )?;
    }
    Ok(result)
}

/// Inline hierarchy follows the model; serialized field order and indentation are irrelevant.
fn collect(
    node: &MetadataNode,
    parent: Option<&NativeUuid>,
    native_path: &SourcePath,
    schema: u32,
    tree: &YamlFile,
    steps: &mut Vec<Step>,
    result: &mut Vec<MetadataDeclaration>,
) -> Result<(), Error> {
    let range = range_at(tree, steps)?;
    let name_range = name_range(node, schema, tree, steps)?;
    result.push(MetadataDeclaration {
        kind: node.kind,
        uuid: node.uuid.clone(),
        name: node_name(node)?.into(),
        parent: parent.cloned(),
        native_path: native_path.clone(),
        range,
        name_range,
    });
    for (section_index, section) in node.sections.iter().enumerate() {
        if let Section::Children(children) = section {
            for (index, child) in children.iter().enumerate() {
                if let Child::Inline { object } = child {
                    let previous = steps.len();
                    steps.extend(document_paths::child_steps(schema, section_index, index));
                    collect(
                        object,
                        Some(&node.uuid),
                        native_path,
                        schema,
                        tree,
                        steps,
                        result,
                    )?;
                    steps.truncate(previous);
                }
            }
        }
    }
    Ok(())
}

/// Locate the typed Name scalar rather than a matching string in comments or preserved XML.
fn name_range(
    node: &MetadataNode,
    schema: u32,
    tree: &YamlFile,
    steps: &[Step],
) -> Result<Range<usize>, Error> {
    for (section, values) in node.sections.iter().enumerate() {
        if let Section::Properties(properties) = values {
            for (index, property) in properties.iter().enumerate() {
                if matches!(
                    property,
                    Property::Typed {
                        name: PropertyName::Name,
                        ..
                    }
                ) {
                    let mut location = steps.to_vec();
                    location.extend(document_paths::property_steps(
                        schema, section, index, property,
                    ));
                    if schema != crate::project_core::source_format::READABLE_SCHEMA_VERSION {
                        location.extend([Step::Key("value"), Step::Key("value")]);
                    }
                    return range_at(tree, &location);
                }
            }
        }
    }
    Err(Error::root("source_name_missing"))
}
