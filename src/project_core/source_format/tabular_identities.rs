//! Table and row type identities retain native UUIDs while their names follow the real metadata parent.

use roxmltree::Node;
use serde::{Deserialize, Serialize};

use super::{
    Error, MD_NAMESPACE, MetadataNode, Namespace, NativeUuid, PropertyName, PropertyValue,
    metadata_reference::READABLE_NAMESPACE, validation::ncname, xml,
};
use crate::project_core::MetadataKind;

#[cfg(test)]
mod tests;

/// One generated native type has separate type and value identities; neither is a metadata object UUID.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedIdentity {
    pub type_id: NativeUuid,
    pub value_id: NativeUuid,
}

/// The observed table profile stores identities once; generated names are derived from metadata names.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TabularSectionIdentities {
    pub prefix: String,
    pub table: GeneratedIdentity,
    pub row: GeneratedIdentity,
}

impl TabularSectionIdentities {
    /// Only a direct catalog/document table and a real readable namespace can own these native slots.
    pub(super) fn validate(
        &self,
        node: &MetadataNode,
        parent: Option<&MetadataNode>,
        context: &[Namespace],
    ) -> Result<(), Error> {
        names(node, parent)?;
        if !ncname(&self.prefix)
            || !context.iter().any(|binding| {
                binding.prefix.as_deref() == Some(&self.prefix) && binding.uri == READABLE_NAMESPACE
            })
        {
            return Err(Error::root("source_namespace_invalid"));
        }
        Ok(())
    }

    /// All four service identities participate in native identity validation without becoming object authority.
    pub(super) const fn uuids(&self) -> [&NativeUuid; 4] {
        [
            &self.table.type_id,
            &self.table.value_id,
            &self.row.type_id,
            &self.row.value_id,
        ]
    }
}

/// Native type names depend on the actual direct parent, never on an editable duplicated service name.
pub(super) fn names(
    node: &MetadataNode,
    parent: Option<&MetadataNode>,
) -> Result<[String; 2], Error> {
    let parent = parent
        .filter(|parent| {
            node.kind == MetadataKind::TabularSection
                && matches!(parent.kind, MetadataKind::Catalog | MetadataKind::Document)
        })
        .ok_or_else(|| Error::root("source_type_definition_invalid"))?;
    let owner = name(parent)?;
    let table = name(node)?;
    let kind = parent.kind.designer_tag();
    let names = [
        format!("{kind}TabularSection.{owner}.{table}"),
        format!("{kind}TabularSectionRow.{owner}.{table}"),
    ];
    if names.iter().any(|name| !ncname(name)) {
        return Err(Error::root("source_type_definition_invalid"));
    }
    Ok(names)
}

/// Names come from the existing single typed Name field in each metadata owner.
fn name(node: &MetadataNode) -> Result<&str, Error> {
    match node.property(PropertyName::Name) {
        Some(PropertyValue::Text(name)) if !name.is_empty() => Ok(name),
        _ => Err(Error::root("source_name_missing")),
    }
}

/// Promote only the two exact native declarations; unexpected names, slots or bindings remain opaque.
pub(super) fn decode(
    node: Node<'_, '_>,
    input: &str,
    inherited: &[Namespace],
) -> Option<TabularSectionIdentities> {
    if !node.has_tag_name((MD_NAMESPACE, "InternalInfo"))
        || xml::plain_element_name(node, input)? != "InternalInfo"
        || node.attributes().len() != 0
    {
        return None;
    }
    let owner = node.parent_element()?;
    if !owner.has_tag_name((MD_NAMESPACE, "TabularSection")) {
        return None;
    }
    let collection = owner.parent_element()?;
    if !collection.has_tag_name((MD_NAMESPACE, "ChildObjects")) {
        return None;
    }
    let parent = collection.parent_element()?;
    let kind = match parent.tag_name().name() {
        "Catalog" | "Document" if parent.tag_name().namespace() == Some(MD_NAMESPACE) => {
            parent.tag_name().name()
        }
        _ => return None,
    };
    let parent_name = xml_name(parent)?;
    let table_name = xml_name(owner)?;
    let declarations = xml::typed_elements(node)?;
    let [table, row] = declarations.as_slice() else {
        return None;
    };
    let prefix = declaration_name(*table, input)?.strip_suffix(":GeneratedType")?;
    if !inherited.iter().any(|binding| {
        binding.prefix.as_deref() == Some(prefix) && binding.uri == READABLE_NAMESPACE
    }) {
        return None;
    }
    let identities = TabularSectionIdentities {
        prefix: prefix.into(),
        table: declaration(
            *table,
            input,
            prefix,
            &format!("{kind}TabularSection.{parent_name}.{table_name}"),
            "TabularSection",
        )?,
        row: declaration(
            *row,
            input,
            prefix,
            &format!("{kind}TabularSectionRow.{parent_name}.{table_name}"),
            "TabularSectionRow",
        )?,
    };
    let mut unique = std::collections::BTreeSet::new();
    if identities
        .uuids()
        .into_iter()
        .any(|uuid| !unique.insert(uuid.key()))
    {
        return None;
    }
    Some(identities)
}

/// Native names must already have a reversible plain value before removing their generated copies.
fn xml_name(node: Node<'_, '_>) -> Option<String> {
    let properties = node
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "Properties")))?;
    let name = properties
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "Name")))?;
    if name.attributes().len() != 0 || name.children().any(|child| !child.is_text()) {
        return None;
    }
    Some(name.children().filter_map(|child| child.text()).collect())
}

/// A declaration has exactly one type UUID and one value UUID, retaining its explicit prefix and role.
fn declaration(
    node: Node<'_, '_>,
    input: &str,
    prefix: &str,
    name: &str,
    category: &str,
) -> Option<GeneratedIdentity> {
    if !node.has_tag_name((READABLE_NAMESPACE, "GeneratedType"))
        || declaration_name(node, input)? != format!("{prefix}:GeneratedType")
        || node.attributes().len() != 2
        || node.attribute("name")? != name
        || node.attribute("category")? != category
    {
        return None;
    }
    let values = xml::typed_elements(node)?;
    let [type_id, value_id] = values.as_slice() else {
        return None;
    };
    Some(GeneratedIdentity {
        type_id: native_uuid(*type_id, input, prefix, "TypeId")?,
        value_id: native_uuid(*value_id, input, prefix, "ValueId")?,
    })
}

/// Generated declarations have two known attributes; local namespace declarations must remain opaque.
fn declaration_name<'a>(node: Node<'_, '_>, input: &'a str) -> Option<&'a str> {
    let head = input
        .get(node.range())?
        .strip_prefix('<')?
        .split_once('>')?
        .0;
    if head
        .split_whitespace()
        .any(|part| part == "xmlns" || part.starts_with("xmlns:") || part.starts_with("xmlns="))
    {
        return None;
    }
    head.split(|ch: char| ch.is_whitespace() || ch == '/')
        .next()
}

/// Unknown UUID syntax or element structure is preserved rather than guessed or normalized.
fn native_uuid(node: Node<'_, '_>, input: &str, prefix: &str, local: &str) -> Option<NativeUuid> {
    if !node.has_tag_name((READABLE_NAMESPACE, local))
        || xml::plain_element_name(node, input)? != format!("{prefix}:{local}")
        || node.attributes().len() != 0
        || node.children().any(|child| !child.is_text())
        || input[node.range()].contains("<![CDATA")
    {
        return None;
    }
    NativeUuid::parse(
        &node
            .children()
            .filter_map(|child| child.text())
            .collect::<String>(),
    )
    .ok()
}
