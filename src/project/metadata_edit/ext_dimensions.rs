//! Extra dimension limits protect existing predefined accounts without changing their rows.

use roxmltree::{Document, Node};

use super::{EditError, schema::MD};
use crate::project::metadata_model::PropertyKey;

const PREDEF: &str = "http://v8.1c.ru/8.3/xcf/predef";

/// Designer disables the count when no characteristic type plan is selected.
pub(super) fn read_only_reason(node: Node<'_, '_>) -> Option<&'static str> {
    (node.has_tag_name((MD, "MaxExtDimensionCount"))
        && value(properties(node)?, "ExtDimensionTypes").is_empty())
    .then_some("ext_dimension_types_missing")
}

/// Clearing the selected plan requires explicitly reducing the count first.
pub(super) fn validate_dependents(
    node: Node<'_, '_>,
    candidate: &Document<'_>,
) -> Result<(), EditError> {
    if !matches!(
        node.tag_name().name(),
        "MaxExtDimensionCount" | "ExtDimensionTypes"
    ) || properties(node).is_none()
    {
        return Ok(());
    }
    let properties = owner_properties(candidate).ok_or(EditError::InvalidXml)?;
    let count = count(properties)?;
    if count > 0 && value(properties, "ExtDimensionTypes").is_empty() {
        return Err(incompatible("MaxExtDimensionCount"));
    }
    Ok(())
}

/// Only changed owner settings constrain the separate, freshly read predefined payload.
pub struct PredefinedDimensions {
    count: Option<usize>,
    plan: Option<String>,
}

impl PredefinedDimensions {
    /// The same comparison is used for preview, publication and undo/redo.
    pub(crate) fn changed(before: &str, after: &str) -> Result<Option<Self>, EditError> {
        let before = Document::parse(before).map_err(|_| EditError::InvalidXml)?;
        let after = Document::parse(after).map_err(|_| EditError::InvalidXml)?;
        let (Some(old), Some(new)) = (owner_properties(&before), owner_properties(&after)) else {
            return Ok(None);
        };
        let changed_count =
            value(old, "MaxExtDimensionCount") != value(new, "MaxExtDimensionCount");
        let changed_plan = value(old, "ExtDimensionTypes") != value(new, "ExtDimensionTypes");
        if !changed_count && !changed_plan {
            return Ok(None);
        }
        Ok(Some(Self {
            count: changed_count.then(|| count(new)).transpose()?,
            plan: changed_plan.then(|| value(new, "ExtDimensionTypes").to_owned()),
        }))
    }

    /// Neither reducing the limit nor changing the plan may discard or reinterpret existing rows.
    pub(crate) fn validate(&self, input: &str) -> Result<(), EditError> {
        let document = Document::parse(input).map_err(|_| EditError::InvalidXml)?;
        if !document
            .root_element()
            .has_tag_name((PREDEF, "PredefinedData"))
        {
            return Err(EditError::InvalidXml);
        }
        for types in document.descendants().filter(|node| {
            node.has_tag_name((PREDEF, "ExtDimensionTypes"))
                && node
                    .parent()
                    .is_some_and(|parent| parent.has_tag_name((PREDEF, "Item")))
        }) {
            let rows: Vec<_> = types
                .children()
                .filter(|node| node.has_tag_name((PREDEF, "ExtDimensionType")))
                .collect();
            if self.count.is_some_and(|count| rows.len() > count)
                || self.plan.as_ref().is_some_and(|plan| {
                    rows.iter().any(|row| {
                        plan.is_empty()
                            || row
                                .attribute("name")
                                .and_then(|name| name.strip_prefix(plan))
                                .and_then(|suffix| suffix.strip_prefix('.'))
                                .is_none_or(str::is_empty)
                    })
                })
            {
                return Err(incompatible("Predefined"));
            }
        }
        Ok(())
    }
}

/// Only a chart's direct metadata properties control the limit.
fn properties<'a>(node: Node<'a, 'a>) -> Option<Node<'a, 'a>> {
    let properties = node.parent()?;
    (node.tag_name().namespace() == Some(MD)
        && properties.has_tag_name((MD, "Properties"))
        && properties.parent()?.has_tag_name((MD, "ChartOfAccounts")))
    .then_some(properties)
}

/// Descriptor-level checks never match inline properties or a foreign XML namespace.
fn owner_properties<'a>(document: &'a Document<'_>) -> Option<Node<'a, 'a>> {
    let root = document.root_element();
    if !root.has_tag_name((MD, "MetaDataObject")) {
        return None;
    }
    root.children()
        .find(|node| node.has_tag_name((MD, "ChartOfAccounts")))?
        .children()
        .find(|node| node.has_tag_name((MD, "Properties")))
}

/// Missing text denotes an unselected reference, not an invented reference target.
fn value<'a>(properties: Node<'a, 'a>, name: &str) -> &'a str {
    properties
        .children()
        .find(|node| node.has_tag_name((MD, name)))
        .and_then(|node| node.text())
        .unwrap_or_default()
}

/// The native editor accepts 0–50; XML import alone does not enforce this limit.
fn count(properties: Node<'_, '_>) -> Result<usize, EditError> {
    value(properties, "MaxExtDimensionCount")
        .parse::<usize>()
        .ok()
        .filter(|count| *count <= 50)
        .ok_or_else(|| incompatible("MaxExtDimensionCount"))
}

/// Both UI and JSON clients receive the property that must be corrected first.
fn incompatible(name: &str) -> EditError {
    EditError::IncompatibleProperty(PropertyKey {
        namespace: Some(MD.to_owned()),
        name: name.to_owned(),
    })
}
