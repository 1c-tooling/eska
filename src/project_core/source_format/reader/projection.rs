//! Typed facts and exact CST coordinates in native section order; no intermediate XML.

use std::{collections::BTreeMap, ops::Range};

use yaml_edit::{AsYaml, YamlFile};

use super::{MetadataDeclaration, node_name};
use crate::project_core::source_format::{
    Child, Error, MetadataNode, NativeUuid, ObjectSource, Property, PropertyName, Section,
    SourcePath,
    yaml::{self, Step},
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
        let mut steps = if index == 0 {
            vec![Step::Key("document")]
        } else {
            vec![Step::Key("nested"), Step::Index(index - 1)]
        };
        steps.extend([Step::Key("descriptor"), Step::Key("object")]);
        let parent = parents
            .get(&part.path)
            .ok_or_else(|| Error::root("source_descriptor_undeclared"))?;
        collect(
            &part.descriptor.object,
            parent.as_ref(),
            &part.path,
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
    tree: &YamlFile,
    steps: &mut Vec<Step>,
    result: &mut Vec<MetadataDeclaration>,
) -> Result<(), Error> {
    let range = range_at(tree, steps)?;
    let name_range = name_range(node, tree, steps)?;
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
                    steps.extend([
                        Step::Key("sections"),
                        Step::Index(section_index),
                        Step::Key("items"),
                        Step::Index(index),
                        Step::Key("object"),
                    ]);
                    collect(object, Some(&node.uuid), native_path, tree, steps, result)?;
                    steps.truncate(previous);
                }
            }
        }
    }
    Ok(())
}

/// Locate the typed Name scalar rather than a matching string in comments or preserved XML.
fn name_range(node: &MetadataNode, tree: &YamlFile, steps: &[Step]) -> Result<Range<usize>, Error> {
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
                    location.extend([
                        Step::Key("sections"),
                        Step::Index(section),
                        Step::Key("items"),
                        Step::Index(index),
                        Step::Key("value"),
                        Step::Key("value"),
                    ]);
                    return range_at(tree, &location);
                }
            }
        }
    }
    Err(Error::root("source_name_missing"))
}

/// CST offsets are UTF-8 bytes; LSP encoding conversion belongs to the protocol consumer.
fn range_at(tree: &YamlFile, steps: &[Step]) -> Result<Range<usize>, Error> {
    let node = yaml::at(tree, steps).ok_or_else(|| Error::root("source_location_invalid"))?;
    let range = node
        .as_node()
        .ok_or_else(|| Error::root("source_location_invalid"))?
        .text_range();
    Ok(usize::from(range.start())..usize::from(range.end()))
}
