//! Unchanged inactive reference prefixes are diagnostic transport, never a second editable value.

use serde_json::Value;

use super::super::{
    AcceptedBase, Child, Error, MetadataNode, Property, PropertyValue, Section, SourcePath,
};

/// Allow authoring only Empty while retaining the accepted readable prefix slot in the diagnostic view.
pub(in crate::project_core::source_format) fn cleared_reference_bindings(
    base: &AcceptedBase,
    expected: &mut Value,
    current: Option<&Value>,
) -> Result<(), Error> {
    if let Some(document) = expected.get_mut("document") {
        document_bindings(
            base,
            document,
            current.and_then(|value| value.get("document")),
        )?;
    }
    if let Some(nested) = expected.get_mut("nested").and_then(Value::as_array_mut) {
        for document in nested {
            let working = current
                .and_then(|value| value.get("nested"))
                .and_then(Value::as_array)
                .and_then(|parts| {
                    parts
                        .iter()
                        .find(|part| part.get("path") == document.get("path"))
                });
            document_bindings(base, document, working)?;
        }
    }
    Ok(())
}

/// The accepted native descriptor supplies the only binding authority, including for family layouts.
fn document_bindings(
    base: &AcceptedBase,
    expected: &mut Value,
    current: Option<&Value>,
) -> Result<(), Error> {
    if expected["descriptor"]["schema_version"].as_u64()
        != Some(u64::from(super::super::READABLE_SCHEMA_VERSION))
    {
        return Ok(());
    }
    let path = SourcePath::parse(
        expected["path"]
            .as_str()
            .ok_or_else(|| Error::root("source_schema_invalid"))?,
    )?;
    let accepted = base
        .package
        .documents
        .get(&path)
        .ok_or_else(|| Error::root("source_baseline_corrupt"))?;
    let working = current
        .and_then(|part| part.get("descriptor"))
        .and_then(|part| part.get("object"));
    node_bindings(
        &accepted.descriptor.object,
        &mut expected["descriptor"]["object"],
        working,
    )
}

/// Unchanged accepted prefixes or a canonical bare name are valid; edited helper details still differ.
fn node_bindings(
    accepted: &MetadataNode,
    expected: &mut Value,
    current: Option<&Value>,
) -> Result<(), Error> {
    for (section_index, section) in accepted.sections.iter().enumerate() {
        if let Section::Properties(properties) = section {
            for (property_index, property) in properties.iter().enumerate() {
                let Property::Typed {
                    name,
                    value: PropertyValue::ReferenceList(references),
                } = property
                else {
                    continue;
                };
                if expected["properties"][name.as_str()] != serde_json::json!({"state":"empty"}) {
                    continue;
                }
                let slot = serde_json::json!({"name":name, "references":references.native});
                let working = current
                    .and_then(|node| node.get("native"))
                    .and_then(|native| native.get("sections"))
                    .and_then(|sections| sections.get(section_index))
                    .and_then(|section| section.get("items"))
                    .and_then(|items| items.get(property_index));
                if working == Some(&slot) {
                    let selected =
                        &mut expected["native"]["sections"][section_index]["items"][property_index];
                    if *selected == Value::String(name.as_str().into()) {
                        *selected = slot;
                    }
                }
            }
        }
        if let Section::Children(children) = section {
            for child in children {
                if let Child::Inline { object } = child {
                    let uuid = object.uuid.as_str();
                    let working = current
                        .and_then(|node| node.get("children"))
                        .and_then(Value::as_array)
                        .and_then(|values| {
                            values.iter().find_map(|value| {
                                let node = value.get("object")?;
                                node.get("uuid")
                                    .and_then(Value::as_str)
                                    .filter(|id| id.eq_ignore_ascii_case(uuid))
                                    .map(|_| node)
                            })
                        });
                    if let Some(node) = expected
                        .get_mut("children")
                        .and_then(Value::as_array_mut)
                        .and_then(|values| {
                            values.iter_mut().find_map(|value| {
                                if value
                                    .get("object")?
                                    .get("uuid")?
                                    .as_str()?
                                    .eq_ignore_ascii_case(uuid)
                                {
                                    value.get_mut("object")
                                } else {
                                    None
                                }
                            })
                        })
                    {
                        node_bindings(object, node, working)?;
                    }
                }
            }
        }
    }
    Ok(())
}
