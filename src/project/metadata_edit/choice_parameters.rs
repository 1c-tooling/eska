//! Existing choice parameters expose their name attribute and individual typed values.

use super::{
    EditError, EditPlan, EditableValueType, Replacement, ScalarSchema, ValueConstraints,
    ValueDomain, choice_links, patch,
    schema::{APP, CORE, MD, READABLE, XS, XSI},
    types, value_schema,
};
use crate::project::metadata_model::PropertyKey;
use roxmltree::{Document, Node};

/// A parameter row has one name attribute and exactly one platform value wrapper.
pub(super) fn is_item(node: Node<'_, '_>) -> bool {
    node.has_tag_name((APP, "item"))
        && node.attributes().len() == 1
        && node.attribute("name").is_some()
        && record_content(node)
        && node.parent().is_some_and(|parent| {
            parent.tag_name().name() == "ChoiceParameters"
                && matches!(parent.tag_name().namespace(), Some(MD | READABLE))
        })
        && node.children().filter(Node::is_element).count() == 1
        && node
            .children()
            .any(|child| child.has_tag_name((APP, "value")))
}

/// Only existing fixed-array elements are traversed; container replacement would change cardinality.
pub(super) fn is_value(node: Node<'_, '_>) -> bool {
    (node.has_tag_name((APP, "value")) && node.parent().is_some_and(is_item))
        || (node.has_tag_name((CORE, "Value"))
            && node
                .parent()
                .is_some_and(|parent| is_value(parent) && is_array(parent)))
}

/// Namespace-aware recognition never treats an arbitrary XML record as an editable array.
pub(super) fn is_array(node: Node<'_, '_>) -> bool {
    value_schema::annotation(node)
        .is_some_and(|key| key.namespace.as_deref() == Some(CORE) && key.name == "FixedArray")
        && node.attributes().len() == 1
        && record_content(node)
        && node
            .children()
            .filter(Node::is_element)
            .all(|child| child.has_tag_name((CORE, "Value")))
}

/// Parameter values admit platform primitives and lazily enumerated project reference types.
pub(super) fn schema(node: Node<'_, '_>) -> Option<ScalarSchema> {
    if !is_value(node)
        || node
            .children()
            .any(|child| child.is_element() || child.is_pi())
        || node.attributes().len() != 1
        || is_array(node)
    {
        return None;
    }
    let key = if matches!(node.attribute((XSI, "nil")), Some("true" | "1")) {
        None
    } else {
        Some(stored_key(node)?)
    };
    let mut types = primitives();
    if let Some(key) = &key
        && let Some(reference) = reference_type(key)
    {
        types.push(reference);
    }
    Some(ScalarSchema::Value {
        domain: Some(ValueDomain::ChoiceParameter),
        key,
        types,
    })
}

/// A stored design-time reference identifies its generated type without requiring raw XML prefixes.
fn stored_key(node: Node<'_, '_>) -> Option<PropertyKey> {
    let key = value_schema::annotation(node)?;
    if key.namespace.as_deref() != Some(READABLE) || key.name != "DesignTimeRef" {
        return primitives()
            .iter()
            .any(|choice| choice.key == key)
            .then_some(key);
    }
    let (tag, rest) = node
        .text()
        .unwrap_or_default()
        .split_once('.')
        .unwrap_or_default();
    let name = rest.split('.').next()?;
    let family = types::REFERENCE_TYPES
        .iter()
        .find_map(|(family, candidate)| (*candidate == tag).then_some(*family));
    Some(family.map_or(key, |family| PropertyKey {
        namespace: Some(types::CFG.to_owned()),
        name: format!("{family}.{name}"),
    }))
}

/// These four primitive types are offered by the native filter-by-value dialog.
pub fn primitives() -> Vec<EditableValueType> {
    [
        ("string", ValueConstraints::String { max_length: 0 }),
        (
            "decimal",
            ValueConstraints::Number {
                digits: None,
                fraction_digits: None,
                nonnegative: false,
            },
        ),
        ("boolean", ValueConstraints::Boolean),
        (
            "dateTime",
            ValueConstraints::Date {
                fractions: "DateTime".to_owned(),
            },
        ),
    ]
    .into_iter()
    .map(|(name, constraints)| EditableValueType {
        key: PropertyKey {
            namespace: Some(XS.to_owned()),
            name: name.to_owned(),
        },
        constraints,
    })
    .collect()
}

/// The workspace additionally checks the target declaration and its enum/predefined membership.
pub fn reference_type(key: &PropertyKey) -> Option<EditableValueType> {
    (key.namespace.as_deref() == Some(types::CFG) && types::supported(key)).then(|| {
        EditableValueType {
            key: key.clone(),
            constraints: ValueConstraints::Reference,
        }
    })
}

/// Parameter names use a reviewed attribute range; quote style and all neighboring bytes survive.
pub(super) fn rename(input: &str, node: Node<'_, '_>, value: &str) -> Result<EditPlan, EditError> {
    if !is_item(node) || !choice_links::valid_name(value) {
        return Err(EditError::InvalidValue);
    }
    let name = node
        .attribute_node("name")
        .ok_or(EditError::UnsupportedValue)?;
    if name.value() == value {
        return patch::replacements_plan(input, Vec::new());
    }
    patch::replacements_plan(
        input,
        vec![Replacement {
            range: name.range_value(),
            text: value.to_owned(),
        }],
    )
}

/// Native parameter names prohibit duplicates, while ancestor/descendant names remain distinct.
pub(super) fn validate_dependents(
    node: Node<'_, '_>,
    candidate: &Document<'_>,
) -> Result<(), EditError> {
    if !is_item(node) {
        return Ok(());
    }
    let updated = candidate
        .descendants()
        .find(|item| is_item(*item) && item.range().start == node.range().start)
        .ok_or(EditError::InvalidXml)?;
    let name = updated.attribute("name").ok_or(EditError::InvalidXml)?;
    if updated
        .parent()
        .ok_or(EditError::InvalidXml)?
        .children()
        .filter(|item| item.has_tag_name((APP, "item")) && *item != updated)
        .any(|item| {
            item.attribute("name")
                .is_some_and(|other| crate::project::metadata_rename::same_name(name, other))
        })
    {
        return Err(EditError::InvalidValue);
    }
    Ok(())
}

/// Reviewed record wrappers contain elements, formatting whitespace and optional preserved comments.
fn record_content(node: Node<'_, '_>) -> bool {
    node.children().all(|child| {
        !child.is_pi()
            && (!child.is_text() || child.text().is_none_or(|text| text.trim().is_empty()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Foreign names, unknown annotations and array wrappers never fall through to scalar editors.
    #[test]
    fn parameter_shapes_are_closed_and_nested_array_leaves_keep_their_domain() {
        let input = format!(
            "<ChoiceParameters xmlns='{MD}' xmlns:a='{APP}' xmlns:v='{CORE}' xmlns:x='{XS}' xmlns:s='{XSI}'><a:item name='Array'><a:value s:type='v:FixedArray'><v:Value s:type='v:FixedArray'><v:Value s:nil='true'/></v:Value></a:value></a:item></ChoiceParameters>"
        );
        let document = Document::parse(&input).unwrap();
        let leaf = document
            .descendants()
            .find(|node| node.attribute((XSI, "nil")).is_some())
            .unwrap();
        assert!(matches!(
            schema(leaf),
            Some(ScalarSchema::Value {
                domain: Some(ValueDomain::ChoiceParameter),
                key: None,
                ..
            })
        ));
        assert!(schema(leaf.parent().unwrap()).is_none());
        for invalid in [
            input.replace(APP, "urn:foreign"),
            input.replace("s:nil='true'", "s:type='x:unknown'"),
            input.replace("s:nil='true'", "s:type='x:string' extra='keep'"),
            input.replace("s:type='v:FixedArray'", "s:type='v:Array'"),
            input.replace("<a:value s:type", "<a:value unexpected='1' s:type"),
        ] {
            let document = Document::parse(&invalid).unwrap();
            assert!(
                !document.descendants().any(|node| schema(node).is_some()),
                "{invalid}"
            );
        }
    }
}
