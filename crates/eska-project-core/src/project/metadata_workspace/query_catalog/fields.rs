//! Query columns differ from record-object properties, particularly for correspondence accounting.

use super::{QueryFieldChoice, QuerySessionError, standard_name};
use crate::project::{
    metadata_edit::choice_fields::standard,
    metadata_model::MetadataKind,
    metadata_parser::{ParsedDescriptor, ParsedObject},
    query::QueryError,
};
use roxmltree::{Document, Node};
use std::collections::BTreeMap;

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

/// Merge actual declared fields and owner-dependent standard columns without scanning any other descriptor.
pub(super) fn append(
    document: &Document<'_>,
    parsed: &ParsedDescriptor,
    selected: &ParsedObject,
    category: MetadataKind,
    fields: &mut BTreeMap<String, QueryFieldChoice>,
) -> Result<(), QuerySessionError> {
    let node = object_node(document, selected)?;
    let correspondence = category == MetadataKind::AccountingRegister
        && matches!(property(node, "Correspondence"), Some("true" | "1"));
    for object in parsed.objects.iter().filter(|object| {
        object.metadata.parent() == Some(selected.metadata.id())
            && matches!(
                object.metadata.kind(),
                MetadataKind::Attribute
                    | MetadataKind::Dimension
                    | MetadataKind::Resource
                    | MetadataKind::Requisite
            )
    }) {
        let name = object.metadata.name();
        let split = correspondence
            && matches!(
                object.metadata.kind(),
                MetadataKind::Dimension | MetadataKind::Resource
            )
            && matches!(
                property(object_node(document, object)?, "Balance"),
                Some("false" | "0")
            );
        if split {
            for (russian, english) in [("Дт", "Dr"), ("Кт", "Cr")] {
                insert(
                    fields,
                    &format!("{name}{russian}"),
                    &format!("{name}{english}"),
                );
            }
        } else {
            insert(fields, name, name);
        }
    }
    // Unknown compatibility must not enable version-dependent columns.
    for english in standard::fields(node, "") {
        if english == "Account" && correspondence {
            insert(fields, "СчетДт", "AccountDr");
            insert(fields, "СчетКт", "AccountCr");
        } else if let Some(russian) = standard_name(english) {
            insert(fields, russian, english);
        }
    }
    if matches!(
        selected.metadata.kind(),
        MetadataKind::TabularSection | MetadataKind::Enum
    ) {
        insert(fields, "Ссылка", "Ref");
    }
    Ok(())
}

/// Equivalent base/extension columns share one semantic key while preserving both query spellings.
fn insert(fields: &mut BTreeMap<String, QueryFieldChoice>, russian: &str, english: &str) {
    fields
        .entry(english.to_lowercase())
        .or_insert_with(|| QueryFieldChoice {
            russian: russian.to_owned(),
            english: english.to_owned(),
        });
}

/// A field must map to its real namespace-aware object, never to a similarly named nested property.
fn object_node<'a>(
    document: &'a Document<'_>,
    object: &ParsedObject,
) -> Result<Node<'a, 'a>, QuerySessionError> {
    document
        .descendants()
        .find(|node| node.is_element() && node.range() == object.range)
        .ok_or_else(|| QueryError::InvalidXml.into())
}

/// Direct owner properties decide the physical query-column layout.
fn property<'a>(object: Node<'a, '_>, name: &str) -> Option<&'a str> {
    object
        .children()
        .find(|node| node.has_tag_name((MD, "Properties")))?
        .children()
        .find(|node| node.has_tag_name((MD, name)))?
        .text()
}
