//! Catalog hierarchy controls use the same prerequisites as the Designer property editor.

use roxmltree::{Document, Node};

use super::{EditError, schema::MD};
use crate::project::metadata_model::PropertyKey;

/// Disabled controls stay readable and tell both clients which prerequisite is missing.
pub(super) fn read_only_reason(node: Node<'_, '_>) -> Option<&'static str> {
    let properties = properties(node)?;
    match node.tag_name().name() {
        "HierarchyType" | "LimitLevelCount" | "LevelCount"
            if !enabled(properties, "Hierarchical") =>
        {
            Some("hierarchy_disabled")
        }
        "LevelCount" if !enabled(properties, "LimitLevelCount") => Some("level_limit_disabled"),
        _ => None,
    }
}

/// Enabling a hierarchy limit cannot silently repair an invalid stored level count.
pub(super) fn validate_dependents(
    node: Node<'_, '_>,
    candidate: &Document<'_>,
) -> Result<(), EditError> {
    if !matches!(
        node.tag_name().name(),
        "Hierarchical" | "LimitLevelCount" | "LevelCount"
    ) {
        return Ok(());
    }
    let Some(old) = properties(node) else {
        return Ok(());
    };
    let current = candidate
        .descendants()
        .find(|item| {
            item.has_tag_name((MD, "Properties")) && item.range().start == old.range().start
        })
        .ok_or(EditError::InvalidXml)?;
    if enabled(current, "Hierarchical")
        && enabled(current, "LimitLevelCount")
        && let Some(levels) = current
            .children()
            .find(|node| node.has_tag_name((MD, "LevelCount")))
        && levels
            .text()
            .and_then(|text| text.parse::<i64>().ok())
            .is_none_or(|levels| !(2..=10).contains(&levels))
    {
        return Err(EditError::IncompatibleProperty(PropertyKey {
            namespace: Some(MD.to_owned()),
            name: "LevelCount".to_owned(),
        }));
    }
    Ok(())
}

/// Inline attributes and foreign namespaces cannot acquire their owner's hierarchy constraints.
fn properties<'a, 'input>(node: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
    let properties = node.parent()?;
    (node.tag_name().namespace() == Some(MD)
        && properties.has_tag_name((MD, "Properties"))
        && properties.parent()?.has_tag_name((MD, "Catalog")))
    .then_some(properties)
}

/// Both XML Schema boolean true spellings enable a prerequisite.
fn enabled(properties: Node<'_, '_>, name: &str) -> bool {
    properties
        .children()
        .find(|node| node.has_tag_name((MD, name)))
        .and_then(|node| node.text())
        .is_some_and(|text| matches!(text, "true" | "1"))
}
