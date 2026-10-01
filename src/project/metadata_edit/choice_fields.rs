//! Scope-aware source fields for existing choice parameter links.

use roxmltree::{Document, Node};

use super::{
    EditError, EditingDocument, FieldStep, choice_links, references,
    schema::{MD, READABLE, XS, XSI},
};
use crate::project::{
    metadata_model::{LocalizedText, MetadataKind, MetadataObject},
    metadata_parser::ParsedObject,
};

mod standard;

pub const DOMAIN: &str = "ChoiceParameterField";

/// Standard fields retain their real owner identity instead of inventing tree objects.
pub struct ChoiceField {
    pub value: String,
    pub metadata: MetadataObject,
    pub synonyms: Vec<LocalizedText>,
    pub standard_attribute: Option<String>,
}

/// Only the reviewed string-valued `DataPath` writer is mutable.
pub(super) fn is_path(node: Node<'_, '_>) -> bool {
    if !node.has_tag_name((READABLE, "DataPath"))
        || !node.parent().is_some_and(choice_links::is_link)
        || node.children().any(|child| child.is_element())
        || node.attributes().len() != 1
    {
        return false;
    }
    node.attribute((XSI, "type")).is_some_and(|value| {
        let (prefix, name) = value
            .split_once(':')
            .map_or((None, value), |(a, b)| (Some(a), b));
        name == "string" && node.lookup_namespace_uri(prefix) == Some(XS)
    })
}

/// Membership is checked by the workspace; lexical validation rejects arbitrary payloads first.
pub(super) fn valid_path(value: &str) -> bool {
    let owner = value
        .rsplit_once(".StandardAttribute.")
        .map_or(value, |(owner, field)| {
            if field.is_empty() || !field.chars().all(|ch| ch.is_ascii_alphanumeric()) {
                ""
            } else {
                owner
            }
        });
    references::parts(owner).is_some_and(|parts| !parts.is_empty())
}

impl EditingDocument<'_> {
    /// Derive current descriptor-local sources from fresh bytes, including implicit standard fields.
    pub fn choice_fields(
        &self,
        path: &[FieldStep],
        compatibility: &str,
    ) -> Result<Vec<ChoiceField>, EditError> {
        let parsed = self.parsed(self.input)?;
        let document = Document::parse(self.input).map_err(|_| EditError::InvalidXml)?;
        let current = parsed
            .objects
            .iter()
            .find(|object| object.metadata.id() == self.id)
            .ok_or(EditError::UnsupportedValue)?;
        let current_node = object_node(&document, current)?;
        let properties = child(current_node, "Properties").ok_or(EditError::UnsupportedValue)?;
        let node = path
            .iter()
            .try_fold(properties, |parent, step| {
                parent
                    .children()
                    .filter(|node| {
                        node.tag_name().name() == step.key.name
                            && node.tag_name().namespace() == step.key.namespace.as_deref()
                    })
                    .nth(step.occurrence)
            })
            .filter(|node| is_path(*node))
            .ok_or(EditError::UnsupportedValue)?;
        let link = node.parent().ok_or(EditError::InvalidXml)?;
        let list = link.parent().ok_or(EditError::InvalidXml)?;
        let used: Vec<_> = list
            .children()
            .filter(|other| *other != link)
            .flat_map(|other| other.children())
            .filter(|node| is_path(*node))
            .filter_map(|node| node.text())
            .collect();
        let standard = list
            .parent()
            .filter(|node| node.has_tag_name((READABLE, "StandardAttribute")))
            .and_then(|node| node.attribute("name"));
        let is_field = field_kind(current.metadata.kind());
        let scope_id = if is_field {
            current
                .metadata
                .parent()
                .ok_or(EditError::UnsupportedValue)?
        } else {
            current.metadata.id()
        };
        let scope = parsed
            .objects
            .iter()
            .find(|object| object.metadata.id() == scope_id)
            .ok_or(EditError::UnsupportedValue)?;
        let mut scopes = vec![scope];
        if scope.metadata.kind() == MetadataKind::TabularSection {
            let parent = parsed
                .objects
                .iter()
                .find(|object| Some(object.metadata.id()) == scope.metadata.parent())
                .ok_or(EditError::UnsupportedValue)?;
            scopes.push(parent);
        }
        let mut result = Vec::new();
        for scope in scopes {
            let scope_node = object_node(&document, scope)?;
            if !standard::supported(scope_node.tag_name().name()) {
                continue;
            }
            for object in parsed.objects.iter().filter(|object| {
                object.metadata.parent() == Some(scope.metadata.id())
                    && field_kind(object.metadata.kind())
                    && object.metadata.id() != self.id
            }) {
                let object_node = object_node(&document, object)?;
                result.push(target(object, xml_path(object_node)?, None));
            }
            for name in standard::fields(scope_node, compatibility) {
                if scope.metadata.id() == self.id && standard == Some(name) {
                    continue;
                }
                result.push(target(
                    scope,
                    format!("{}.StandardAttribute.{name}", xml_path(scope_node)?),
                    Some(name.to_owned()),
                ));
            }
        }
        result.retain(|candidate| {
            !used
                .iter()
                .any(|value| crate::project::metadata_rename::same_name(value, &candidate.value))
        });
        result.sort_by(|left, right| left.value.cmp(&right.value));
        Ok(result)
    }
}

/// Features offered by the installed EDT field provider are distinct from commands and forms.
const fn field_kind(kind: MetadataKind) -> bool {
    matches!(
        kind,
        MetadataKind::Attribute | MetadataKind::Dimension | MetadataKind::Resource
    )
}

/// Reuse parsed identity and synonyms without depending on session caches.
fn target(object: &ParsedObject, value: String, standard_attribute: Option<String>) -> ChoiceField {
    ChoiceField {
        value,
        metadata: object.metadata.clone(),
        synonyms: object.synonyms.clone(),
        standard_attribute,
    }
}

/// Match the parser's semantic owner to the DOM used for writer and dependency checks.
fn object_node<'a>(
    document: &'a Document<'_>,
    object: &ParsedObject,
) -> Result<Node<'a, 'a>, EditError> {
    document
        .descendants()
        .find(|node| node.is_element() && node.range() == object.range)
        .ok_or(EditError::UnsupportedValue)
}

/// XML tag names preserve external report/processor spelling that differs from tree kinds.
fn xml_path(node: Node<'_, '_>) -> Result<String, EditError> {
    let mut parts = Vec::new();
    for object in node
        .ancestors()
        .filter(|node| node.is_element() && node.tag_name().namespace() == Some(MD))
    {
        if let Some(name) = child(object, "Properties")
            .and_then(|props| child(props, "Name"))
            .and_then(|name| name.text())
        {
            parts.push(format!("{}.{name}", object.tag_name().name()));
        }
    }
    parts.reverse();
    if parts.is_empty() {
        return Err(EditError::UnsupportedValue);
    }
    Ok(parts.join("."))
}

/// Inspect only direct metadata properties, never similarly named nested fields.
fn child<'a>(node: Node<'a, '_>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|child| child.has_tag_name((MD, name)))
}

/// Disabling a previously available source field must not silently break existing links in its owner.
pub(super) fn validate_dependents(
    node: Node<'_, '_>,
    candidate: &Document<'_>,
) -> Result<(), EditError> {
    let Some(owner) = node
        .ancestors()
        .find(|node| node.has_tag_name((MD, "Properties")))
        .and_then(|properties| properties.parent())
        .filter(|owner| standard::supported(owner.tag_name().name()))
    else {
        return Ok(());
    };
    let updated = candidate
        .descendants()
        .find(|item| item.is_element() && item.range().start == owner.range().start)
        .ok_or(EditError::InvalidXml)?;
    let previous = standard::fields(owner, "DontUse");
    let current = standard::fields(updated, "DontUse");
    let owner_path = xml_path(owner)?;
    for field in previous.iter().filter(|field| !current.contains(field)) {
        let value = format!("{owner_path}.StandardAttribute.{field}");
        if updated
            .descendants()
            .filter(|node| is_path(*node))
            .any(|node| {
                node.text()
                    .is_some_and(|text| crate::project::metadata_rename::same_name(text, &value))
            })
        {
            return Err(EditError::IncompatibleProperty(
                crate::project::metadata_model::PropertyKey {
                    namespace: Some(MD.to_owned()),
                    name: "ChoiceParameterLinks".to_owned(),
                },
            ));
        }
    }
    Ok(())
}
