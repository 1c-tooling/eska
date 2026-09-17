//! Normalize metadata property values while retaining unknown qualified fields.

use roxmltree::Node;

use super::{Diagnostic, MD_NAMESPACE, ParseIssue, simple_text};
use crate::project::metadata_model::{
    LocalizedText, MetadataProperty, MetadataValue, PropertyKey, ValueIssue,
};

const CORE_NAMESPACE: &str = "http://v8.1c.ru/8.1/data/core";

/// Extract a localized string without discarding malformed entries or duplicate languages.
pub(super) fn localized(node: Node<'_, '_>) -> Option<Vec<LocalizedText>> {
    if node
        .children()
        .filter(Node::is_text)
        .any(|node| !node.text().unwrap_or_default().trim().is_empty())
    {
        return None;
    }
    let mut result = Vec::new();
    for item in node.children().filter(Node::is_element) {
        if !item.has_tag_name((CORE_NAMESPACE, "item")) {
            return None;
        }
        if item
            .children()
            .filter(Node::is_text)
            .any(|text| !text.text().unwrap_or_default().trim().is_empty())
        {
            return None;
        }
        let mut language = None;
        let mut content = None;
        for field in item.children().filter(Node::is_element) {
            let target = if field.has_tag_name((CORE_NAMESPACE, "lang")) {
                &mut language
            } else if field.has_tag_name((CORE_NAMESPACE, "content")) {
                &mut content
            } else {
                return None;
            };
            if target.is_some() {
                return None;
            }
            *target = Some(simple_text(field)?);
        }
        let language = language.filter(|language| !language.trim().is_empty())?;
        if result
            .iter()
            .any(|text: &LocalizedText| text.language == language)
        {
            return None;
        }
        result.push(LocalizedText {
            language,
            content: content?,
        });
    }
    Some(result)
}

/// Preserve qualified keys and annotations rather than guessing scalar types from text.
pub(super) fn property(node: Node<'_, '_>, diagnostics: &mut Vec<Diagnostic>) -> MetadataProperty {
    let tag = node.tag_name();
    let value = if node.has_tag_name((MD_NAMESPACE, "Synonym")) {
        localized(node).map_or(
            MetadataValue::Unsupported(ValueIssue::InvalidLocalizedText),
            MetadataValue::Localized,
        )
    } else if let Some(text) = simple_text(node) {
        MetadataValue::Text(text)
    } else if node
        .children()
        .filter(Node::is_text)
        .any(|node| !node.text().unwrap_or_default().trim().is_empty())
    {
        MetadataValue::Unsupported(ValueIssue::MixedContent)
    } else {
        MetadataValue::Record(
            node.children()
                .filter(Node::is_element)
                .map(|node| property(node, diagnostics))
                .collect(),
        )
    };
    if matches!(value, MetadataValue::Unsupported(_))
        && !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.range == node.range())
    {
        diagnostics.push(Diagnostic {
            range: node.range(),
            issue: ParseIssue::UnsupportedValue,
        });
    }
    MetadataProperty {
        key: PropertyKey {
            namespace: tag.namespace().map(str::to_owned),
            name: tag.name().to_owned(),
        },
        qualifiers: node
            .attributes()
            .map(|attribute| {
                (
                    PropertyKey {
                        namespace: attribute.namespace().map(str::to_owned),
                        name: attribute.name().to_owned(),
                    },
                    attribute.value().to_owned(),
                )
            })
            .collect(),
        value,
    }
}
