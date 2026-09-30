//! Replace an existing type item and its dependent qualifiers without rewriting its siblings.

use std::fmt::Write;

use roxmltree::Node;

use super::{
    EditError, EditPlan, Replacement, patch,
    schema::{CORE, XS},
};
use crate::project::metadata_model::PropertyKey;

pub const CFG: &str = "http://v8.1c.ru/8.1/data/enterprise/current-config";

/// Supported generated type families map directly to existing metadata object kinds.
pub const REFERENCE_TYPES: &[(&str, &str)] = &[
    ("CatalogRef", "Catalog"),
    ("DocumentRef", "Document"),
    ("EnumRef", "Enum"),
    ("ChartOfAccountsRef", "ChartOfAccounts"),
    ("ChartOfCalculationTypesRef", "ChartOfCalculationTypes"),
    (
        "ChartOfCharacteristicTypesRef",
        "ChartOfCharacteristicTypes",
    ),
    ("ExchangePlanRef", "ExchangePlan"),
    ("BusinessProcessRef", "BusinessProcess"),
    ("TaskRef", "Task"),
];

pub const PRIMITIVES: &[(&str, &str)] = &[
    (XS, "string"),
    (XS, "decimal"),
    (XS, "boolean"),
    (XS, "dateTime"),
    (CORE, "ValueStorage"),
    (CORE, "UUID"),
];

/// Interpret only namespace-resolved type identities, not the XML prefix spelling.
pub(super) fn key(node: Node<'_, '_>) -> Option<PropertyKey> {
    let text = node.text()?.trim();
    let (prefix, name) = text
        .split_once(':')
        .map_or((None, text), |(prefix, name)| (Some(prefix), name));
    if name.is_empty() || name.contains([':', ' ', '\n', '\r', '\t']) {
        return None;
    }
    Some(PropertyKey {
        namespace: Some(node.lookup_namespace_uri(prefix)?.to_owned()),
        name: name.to_owned(),
    })
}

/// Recognize primitive/reference items whose replacement does not change the list cardinality.
pub(super) fn supported(key: &PropertyKey) -> bool {
    PRIMITIVES
        .iter()
        .any(|(namespace, name)| key.namespace.as_deref() == Some(namespace) && key.name == *name)
        || key.namespace.as_deref() == Some(CFG)
            && key.name.split_once('.').is_some_and(|(family, name)| {
                REFERENCE_TYPES.iter().any(|(prefix, _)| *prefix == family)
                    && !name.is_empty()
                    && !name.contains('.')
            })
}

/// Qualifier names and defaults follow the platform's XML `TypeDescription` representation.
fn qualifier(key: &PropertyKey) -> Option<(&'static str, &'static [(&'static str, &'static str)])> {
    if key.namespace.as_deref() != Some(XS) {
        return None;
    }
    match key.name.as_str() {
        "string" => Some((
            "StringQualifiers",
            &[("Length", "0"), ("AllowedLength", "Variable")],
        )),
        "decimal" => Some((
            "NumberQualifiers",
            &[
                ("Digits", "10"),
                ("FractionDigits", "0"),
                ("AllowedSign", "Any"),
            ],
        )),
        "dateTime" => Some(("DateQualifiers", &[("DateFractions", "DateTime")])),
        _ => None,
    }
}

/// Compute the type and qualifier edits together; callers validate target metadata beforehand.
pub(super) fn replace(
    input: &str,
    node: Node<'_, '_>,
    target: &PropertyKey,
) -> Result<EditPlan, EditError> {
    let current = key(node)
        .filter(supported)
        .ok_or(EditError::UnsupportedValue)?;
    if !supported(target) {
        return Err(EditError::InvalidValue);
    }
    if &current == target {
        return patch::replacements_plan(input, Vec::new());
    }
    let parent = node.parent().ok_or(EditError::UnsupportedValue)?;
    let children: Vec<_> = parent.children().filter(Node::is_element).collect();
    if children.iter().any(|child| {
        child != &node && child.has_tag_name((CORE, "Type")) && key(*child).as_ref() == Some(target)
    }) {
        return Err(EditError::InvalidValue);
    }
    if children.iter().any(|child| {
        child.tag_name().namespace() != Some(CORE)
            || !matches!(
                child.tag_name().name(),
                "Type"
                    | "TypeSet"
                    | "NumberQualifiers"
                    | "StringQualifiers"
                    | "DateQualifiers"
                    | "BinaryDataQualifiers"
            )
    }) {
        return Err(EditError::UnsupportedValue);
    }
    let (qname, declaration) = qname(node, target)?;
    let mut edits = patch::text_plan(input, node.range(), &qname)?.replacements;
    if let Some(declaration) = declaration {
        let start = node.range().start;
        let mut reader = quick_xml::Reader::from_str(&input[node.range()]);
        reader.read_event().map_err(|_| EditError::InvalidXml)?;
        let end = start
            + usize::try_from(reader.buffer_position()).map_err(|_| EditError::InvalidXml)?
            - 1;
        edits.push(Replacement {
            range: end..end,
            text: declaration,
        });
    }
    validate_shape(parent)?;
    qualifier_edits(input, node, target, &current, &children, &mut edits)?;
    patch::replacements_plan(input, edits)
}

/// Change dependent groups as ordered minimal replacements, retaining unrelated qualifiers.
fn qualifier_edits(
    input: &str,
    node: Node<'_, '_>,
    target: &PropertyKey,
    current: &PropertyKey,
    children: &[Node<'_, '_>],
    edits: &mut Vec<Replacement>,
) -> Result<(), EditError> {
    let old = qualifier(current)
        .and_then(|(name, _)| {
            children
                .iter()
                .find(|child| child.has_tag_name((CORE, name)))
        })
        .copied();
    let new = qualifier(target);
    if let Some((name, _)) = new
        && children
            .iter()
            .any(|child| child.has_tag_name((CORE, name)))
    {
        return Err(EditError::UnsupportedValue);
    }
    let others: Vec<_> = children
        .iter()
        .copied()
        .filter(|child| Some(*child) != old)
        .collect();
    let anchor = new.and_then(|(name, _)| {
        others
            .iter()
            .rev()
            .find(|child| order(child.tag_name().name()) < order(name))
            .copied()
    });
    // Reuse the old group's position if it already occupies the new group's ordered slot.
    let in_place = old.zip(new).is_some_and(|(old, (name, _))| {
        children
            .iter()
            .filter(|child| Some(**child) != Some(old))
            .all(|child| {
                if child.range().start < old.range().start {
                    order(child.tag_name().name()) < order(name)
                } else {
                    order(child.tag_name().name()) > order(name)
                }
            })
    });
    if let Some(old) = old {
        if in_place {
            if let Some((name, fields)) = new {
                edits.push(Replacement {
                    range: old.range(),
                    text: qualifier_xml(input, old, name, fields)?,
                });
            }
        } else {
            edits.push(Replacement {
                range: line_range(input, old.range()),
                text: String::new(),
            });
        }
    }
    if !in_place && let Some((name, fields)) = new {
        let anchor = anchor.ok_or(EditError::UnsupportedValue)?;
        let separator = indentation(input, node.range().start)
            .map_or_else(String::new, |indent| format!("{}{indent}", newline(input)));
        edits.push(Replacement {
            range: anchor.range().end..anchor.range().end,
            text: format!("{separator}{}", qualifier_xml(input, node, name, fields)?),
        });
    }
    Ok(())
}

/// The platform serializes qualifier groups in this order after the type members.
fn order(name: &str) -> u8 {
    match name {
        "Type" => 0,
        "NumberQualifiers" => 1,
        "StringQualifiers" => 2,
        "DateQualifiers" => 3,
        "BinaryDataQualifiers" => 4,
        _ => 255,
    }
}

/// Refuse opaque groups instead of discarding their attributes, comments or unknown children.
pub(super) fn validate_shape(parent: Node<'_, '_>) -> Result<(), EditError> {
    let mut names = std::collections::BTreeSet::new();
    let mut types = std::collections::BTreeSet::new();
    for child in parent.children().filter(Node::is_element) {
        if child.tag_name().namespace() != Some(CORE) {
            return Err(EditError::UnsupportedValue);
        }
        if child.tag_name().name() == "Type" {
            let value = key(child).ok_or(EditError::UnsupportedValue)?;
            if !supported(&value) || !types.insert((value.namespace, value.name)) {
                return Err(EditError::UnsupportedValue);
            }
            continue;
        }
        let expected: &[&str] = match child.tag_name().name() {
            "NumberQualifiers" => &["Digits", "FractionDigits", "AllowedSign"],
            "StringQualifiers" | "BinaryDataQualifiers" => &["Length", "AllowedLength"],
            "DateQualifiers" => &["DateFractions"],
            _ => return Err(EditError::UnsupportedValue),
        };
        if !names.insert(child.tag_name().name())
            || child.attributes().len() != 0
            || child
                .descendants()
                .any(|node| node.is_comment() || node.is_pi())
        {
            return Err(EditError::UnsupportedValue);
        }
        let leaves: Vec<_> = child.children().filter(Node::is_element).collect();
        if leaves.len() != expected.len()
            || leaves.iter().zip(expected).any(|(leaf, name)| {
                !leaf.has_tag_name((CORE, *name))
                    || leaf.attributes().len() != 0
                    || leaf.children().any(|node| node.is_element())
            })
        {
            return Err(EditError::UnsupportedValue);
        }
        validate_numbers(child)?;
    }
    Ok(())
}

/// Precision constrains the scale, so validating the two fields independently is insufficient.
pub(super) fn validate_numbers(node: Node<'_, '_>) -> Result<(), EditError> {
    if !node.has_tag_name((CORE, "NumberQualifiers")) {
        return Ok(());
    }
    let number = |name| {
        node.children()
            .find(|child| child.has_tag_name((CORE, name)))
            .and_then(|child| child.text())
            .and_then(|text| text.parse::<u32>().ok())
    };
    match (number("Digits"), number("FractionDigits")) {
        (Some(digits), Some(fraction)) if (1..=38).contains(&digits) && fraction <= digits => {
            Ok(())
        }
        _ => Err(EditError::InvalidValue),
    }
}

/// Reuse a visible namespace prefix, or introduce a declaration only on the changed item.
fn qname(node: Node<'_, '_>, key: &PropertyKey) -> Result<(String, Option<String>), EditError> {
    let namespace = key.namespace.as_deref().ok_or(EditError::InvalidValue)?;
    if let Some(prefix) = node.lookup_prefix(namespace) {
        return Ok((format!("{prefix}:{}", key.name), None));
    }
    if node.default_namespace() == Some(namespace) {
        return Ok((key.name.clone(), None));
    }
    for prefix in ["t", "t1", "t2", "t3"] {
        if node.lookup_namespace_uri(Some(prefix)).is_none() {
            return Ok((
                format!("{prefix}:{}", key.name),
                Some(format!(" xmlns:{prefix}=\"{namespace}\"")),
            ));
        }
    }
    Err(EditError::UnsupportedValue)
}

/// Render only a newly required qualifier group using the document's existing prefix and layout.
fn qualifier_xml(
    input: &str,
    anchor: Node<'_, '_>,
    name: &str,
    fields: &[(&str, &str)],
) -> Result<String, EditError> {
    let prefix = anchor
        .lookup_prefix(CORE)
        .map(|prefix| format!("{prefix}:"))
        .or_else(|| (anchor.default_namespace() == Some(CORE)).then(String::new))
        .ok_or(EditError::UnsupportedValue)?;
    let indent = indentation(input, anchor.range().start);
    let unit = indent.map_or("", |indent| if indent.contains('\t') { "\t" } else { "  " });
    let mut output = format!("<{prefix}{name}>");
    for (field, value) in fields {
        if let Some(indent) = indent {
            let _ = write!(output, "{}{indent}{unit}", newline(input));
        }
        let _ = write!(output, "<{prefix}{field}>{value}</{prefix}{field}>");
    }
    if let Some(indent) = indent {
        let _ = write!(output, "{}{indent}", newline(input));
    }
    let _ = write!(output, "</{prefix}{name}>");
    Ok(output)
}

/// A node on its own line owns that line's indentation; inline nodes own only their XML bytes.
fn line_range(input: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    if let Some(indent) = indentation(input, range.start) {
        let after = &input[range.end..];
        let ending = if after.starts_with("\r\n") {
            2
        } else {
            usize::from(after.starts_with('\n'))
        };
        if ending > 0 {
            return range.start - indent.len()..range.end + ending;
        }
    }
    range
}

/// Detect indentation without converting tabs, spaces or mixed line endings elsewhere.
fn indentation(input: &str, start: usize) -> Option<&str> {
    let prefix = &input[..start];
    let indent = &prefix[prefix.rfind('\n').map_or(0, |index| index + 1)..];
    indent
        .chars()
        .all(|character| matches!(character, ' ' | '\t'))
        .then_some(indent)
}

/// Newly inserted lines follow the first observed source line ending.
fn newline(input: &str) -> &str {
    if input
        .find('\n')
        .is_some_and(|index| index > 0 && input.as_bytes()[index - 1] == b'\r')
    {
        "\r\n"
    } else {
        "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preserve the selected type item's actual XML prefix in test inputs.
    fn plan(input: &str, namespace: &str, name: &str) -> Result<EditPlan, EditError> {
        let document = roxmltree::Document::parse(input).expect("input");
        let node = document
            .descendants()
            .find(|node| node.has_tag_name((CORE, "Type")))
            .expect("type");
        replace(
            input,
            node,
            &PropertyKey {
                namespace: Some(namespace.to_owned()),
                name: name.to_owned(),
            },
        )
    }

    #[test]
    fn switching_primitive_changes_only_its_type_and_qualifier_group() {
        let input = format!(
            "\u{feff}<r xmlns:v='{CORE}' xmlns:x='{XS}'>\r\n\t<Type>\r\n\t\t<v:Type>x:decimal</v:Type>\r\n\t\t<v:Type>x:boolean</v:Type>\r\n\t\t<v:NumberQualifiers>\r\n\t\t\t<v:Digits>12</v:Digits>\r\n\t\t\t<v:FractionDigits>2</v:FractionDigits>\r\n\t\t\t<v:AllowedSign>Any</v:AllowedSign>\r\n\t\t</v:NumberQualifiers>\r\n\t</Type><other a='1'>unchanged</other>\r\n</r>"
        );
        let changed = plan(&input, XS, "string").expect("type change");
        let expected = input.replace("x:decimal", "x:string").replace("NumberQualifiers", "StringQualifiers")
            .replace("<v:Digits>12</v:Digits>", "<v:Length>0</v:Length>")
            .replace("<v:FractionDigits>2</v:FractionDigits>\r\n\t\t\t<v:AllowedSign>Any</v:AllowedSign>", "<v:AllowedLength>Variable</v:AllowedLength>");
        assert_eq!(changed.output(), expected);
        assert_eq!(
            changed
                .history()
                .replay(changed.output(), true)
                .expect("undo")
                .output(),
            input
        );
        assert!(
            plan(&input, XS, "boolean").is_err(),
            "duplicate existing type"
        );
    }

    #[test]
    fn adds_namespace_locally_and_retains_all_unrelated_bytes() {
        let input = format!(
            "<r xmlns:v='{CORE}' xmlns:x='{XS}'><Type><v:Type>x:boolean</v:Type></Type></r>"
        );
        let changed = plan(&input, CFG, "CatalogRef.Items").expect("reference");
        assert_eq!(
            changed.output(),
            input.replace(
                "<v:Type>x:boolean",
                &format!("<v:Type xmlns:t=\"{CFG}\">t:CatalogRef.Items")
            )
        );
        assert_eq!(
            changed
                .history()
                .replay(changed.output(), true)
                .expect("undo")
                .output(),
            input
        );
    }
}
