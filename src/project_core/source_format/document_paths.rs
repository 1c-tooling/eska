//! Shared wire coordinates follow declared native identity across both descriptor envelopes.

use super::{
    Child, MetadataNode, NativeUuid, ObjectSource, Property, Section, SourcePath, yaml::Step,
};

/// A borrowed native node is paired with coordinates in the exact selected wire envelope.
pub(super) struct NodeLocation<'a> {
    pub node: &'a MetadataNode,
    pub schema: u32,
    pub steps: Vec<Step>,
}

/// Native path and UUID select modeled metadata, never UUID-like text inside opaque fragments.
pub(super) fn locate<'a>(
    source: &'a ObjectSource,
    native: &SourcePath,
    uuid: &NativeUuid,
) -> Option<NodeLocation<'a>> {
    std::iter::once(&source.document)
        .chain(&source.nested)
        .enumerate()
        .find_map(|(index, part)| {
            if &part.path != native {
                return None;
            }
            find(
                &part.descriptor.object,
                uuid,
                part.descriptor.schema_version,
                root_steps(index),
            )
        })
}

/// Main and legacy family descriptors retain their existing outer authoring wrapper.
pub(super) fn root_steps(index: usize) -> Vec<Step> {
    let mut steps = if index == 0 {
        vec![Step::Key("document")]
    } else {
        vec![Step::Key("nested"), Step::Index(index - 1)]
    };
    steps.extend([Step::Key("descriptor"), Step::Key("object")]);
    steps
}

/// Inline child order is authoritative once; native section slots contain no duplicate children.
pub(super) fn child_steps(schema: u32, section: usize, index: usize) -> Vec<Step> {
    let mut steps = child_entry_steps(schema, section, index);
    steps.push(Step::Key("object"));
    steps
}

/// Inline, external and opaque children share an ordered entry before any nested object mapping.
pub(super) fn child_entry_steps(schema: u32, section: usize, index: usize) -> Vec<Step> {
    if schema == super::READABLE_SCHEMA_VERSION {
        vec![Step::Key("children"), Step::Index(index)]
    } else {
        section_steps(section, index)
    }
}

/// A typed property is addressed by name in v2; preserved syntax stays in its native ordinal slot.
pub(super) fn property_steps(
    schema: u32,
    section: usize,
    index: usize,
    property: &Property,
) -> Vec<Step> {
    if schema == super::READABLE_SCHEMA_VERSION {
        if let Property::Typed { name, .. } = property {
            return vec![Step::Key("properties"), Step::Key(name.as_str())];
        }
        let mut steps = vec![Step::Key("native")];
        steps.extend(section_steps(section, index));
        steps
    } else {
        section_steps(section, index)
    }
}

/// Metadata materialization and YAML order use the same explicit native section positions.
fn section_steps(section: usize, index: usize) -> Vec<Step> {
    vec![
        Step::Key("sections"),
        Step::Index(section),
        Step::Key("items"),
        Step::Index(index),
    ]
}

/// Recursive identity lookup retains the owning descriptor's schema and real child ancestry.
fn find<'a>(
    node: &'a MetadataNode,
    uuid: &NativeUuid,
    schema: u32,
    steps: Vec<Step>,
) -> Option<NodeLocation<'a>> {
    if node.uuid.key() == uuid.key() {
        return Some(NodeLocation {
            node,
            schema,
            steps,
        });
    }
    for (section, contents) in node.sections.iter().enumerate() {
        if let Section::Children(items) = contents {
            for (index, child) in items.iter().enumerate() {
                if let Child::Inline { object } = child {
                    let mut child_location = steps.clone();
                    child_location.extend(child_steps(schema, section, index));
                    if let Some(found) = find(object, uuid, schema, child_location) {
                        return Some(found);
                    }
                }
            }
        }
    }
    None
}
