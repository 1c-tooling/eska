//! Predefined items live in a separate payload and retain their own namespace and byte ranges.
use super::{
    Diagnostic, LocatedProperty, ParseError, ParseIssue, ParsedDescriptor, ParsedObject,
    PropertiesMode, check_envelope, simple_text, values,
};
use crate::project::metadata_model::{LocalizedText, MetadataKind, MetadataObject, ObjectId};
use roxmltree::{Document, Node, ParsingOptions};
use std::collections::HashSet;

const NS: &str = "http://v8.1c.ru/8.3/xcf/predef";

/// Parse only Item/ChildItems hierarchy; property internals never become metadata children.
pub fn parse(
    input: &str,
    owner: &ObjectId,
    mode: PropertiesMode,
) -> Result<ParsedDescriptor, ParseError> {
    check_envelope(input)?;
    let doc = Document::parse_with_options(
        input,
        ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(ParseError::Xml)?;
    let root = doc.root_element();
    if !root.has_tag_name((NS, "PredefinedData")) {
        return Err(ParseError::Invalid(Diagnostic {
            range: root.range(),
            issue: ParseIssue::UnsupportedRoot,
        }));
    }
    let mut parsed = ParsedDescriptor {
        version: root.attribute("version").map(str::to_owned),
        objects: Vec::new(),
        references: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut ids = HashSet::new();
    for child in root.children().filter(Node::is_element) {
        item(child, owner, mode, &mut parsed, &mut ids)?;
    }
    Ok(parsed)
}

/// Build stable hierarchical identities and retain the original declaration order.
fn item(
    node: Node<'_, '_>,
    parent: &ObjectId,
    mode: PropertiesMode,
    parsed: &mut ParsedDescriptor,
    ids: &mut HashSet<ObjectId>,
) -> Result<ObjectId, ParseError> {
    let invalid = || {
        ParseError::Invalid(Diagnostic {
            range: node.range(),
            issue: ParseIssue::InvalidObject,
        })
    };
    if !node.has_tag_name((NS, "Item")) {
        return Err(invalid());
    }
    let mut names = node.children().filter(|n| n.has_tag_name((NS, "Name")));
    let name = names
        .next()
        .and_then(simple_text)
        .filter(|_| names.next().is_none())
        .ok_or_else(invalid)?;
    let metadata = MetadataObject::new(
        MetadataKind::PredefinedItem,
        name,
        node.attribute("id").unwrap_or_default().into(),
        Some(parent.clone()),
    )
    .map_err(|_| invalid())?;
    let id = metadata.id().clone();
    if !ids.insert(id.clone()) {
        return Err(ParseError::Invalid(Diagnostic {
            range: node.range(),
            issue: ParseIssue::DuplicateIdentity(id),
        }));
    }
    let synonyms = node
        .children()
        .find(|n| n.has_tag_name((NS, "Description")))
        .and_then(simple_text)
        .filter(|value| !value.is_empty())
        .map(|content| {
            vec![LocalizedText {
                language: "und".into(),
                content,
            }]
        })
        .unwrap_or_default();
    let properties = if mode == PropertiesMode::All {
        Some(
            node.children()
                .filter(Node::is_element)
                .filter(|n| !n.has_tag_name((NS, "ChildItems")))
                .map(|n| LocatedProperty {
                    property: values::property(n, &mut parsed.diagnostics),
                    range: n.range(),
                })
                .collect(),
        )
    } else {
        None
    };
    let index = parsed.objects.len();
    parsed.objects.push(ParsedObject {
        metadata,
        synonyms,
        range: node.range(),
        children: Vec::new(),
        properties,
    });
    let mut children = Vec::new();
    for container in node
        .children()
        .filter(|n| n.has_tag_name((NS, "ChildItems")))
    {
        for child in container.children().filter(Node::is_element) {
            children.push(item(child, &id, mode, parsed, ids)?);
        }
    }
    parsed.objects[index].children = children;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reject ambiguous identities and unknown hierarchy rather than silently dropping items.
    #[test]
    fn invalid_predefined_hierarchies_are_explicit_errors() {
        let owner = MetadataObject::new(MetadataKind::Catalog, "Owner".into(), String::new(), None)
            .unwrap();
        for body in [
            "<Item><Name>A</Name></Item><Item><Name>A</Name></Item>",
            "<Item><Name/></Item>",
            "<Item><Name>A</Name><Name>B</Name></Item>",
            "<Item xmlns='urn:foreign'><Name>A</Name></Item>",
            "<Item><Name>A</Name><ChildItems><Unknown/></ChildItems></Item>",
        ] {
            let xml = format!("<PredefinedData xmlns='{NS}'>{body}</PredefinedData>");
            assert!(
                parse(&xml, owner.id(), PropertiesMode::Summary).is_err(),
                "{body}"
            );
        }
        assert!(
            parse(
                "<PredefinedData xmlns='urn:foreign'/>",
                owner.id(),
                PropertiesMode::Summary
            )
            .is_err()
        );
    }

    /// Empty payloads are valid and the shared XML limits apply before recursive item traversal.
    #[test]
    fn empty_and_bounded_predefined_payloads() {
        let owner = MetadataObject::new(MetadataKind::Catalog, "Owner".into(), String::new(), None)
            .unwrap();
        assert!(
            parse(
                &format!("<PredefinedData xmlns='{NS}'/>"),
                owner.id(),
                PropertiesMode::Summary
            )
            .unwrap()
            .objects
            .is_empty()
        );
        let deep = format!(
            "<PredefinedData xmlns='{NS}'>{}{}</PredefinedData>",
            "<Item>".repeat(70),
            "</Item>".repeat(70)
        );
        assert!(matches!(
            parse(&deep, owner.id(), PropertiesMode::Summary),
            Err(ParseError::TooDeep)
        ));
    }
}
