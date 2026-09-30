//! Typed scalar writers change only content and the XML Schema annotation of the selected value.

use roxmltree::Node;

use super::{
    EditError, EditPlan, Replacement, patch,
    schema::{MD, READABLE, XS, XSI},
    types,
};
use crate::project::metadata_model::PropertyKey;

/// Min/max bounds use an optional numeric string, as emitted by Designer and EDT's value mapper.
pub(super) fn number_bound(node: Node<'_, '_>) -> bool {
    matches!(node.tag_name().namespace(), Some(MD | READABLE))
        && matches!(node.tag_name().name(), "MinValue" | "MaxValue")
        && !node
            .children()
            .any(|child| child.is_element() || child.is_pi())
        && (node
            .attribute((XSI, "nil"))
            .is_some_and(|nil| matches!(nil, "true" | "1"))
            || node.attribute((XSI, "type")).is_some_and(|name| {
                let (prefix, local) = name
                    .split_once(':')
                    .map_or((None, name), |(prefix, local)| (Some(prefix), local));
                local == "string" && node.lookup_namespace_uri(prefix) == Some(XS)
            }))
}

/// Clearing a numeric bound restores undefined; entering one replaces nil with the string type.
pub(super) fn replace_bound(
    input: &str,
    node: Node<'_, '_>,
    value: &str,
) -> Result<EditPlan, EditError> {
    if !number_bound(node) || (!value.is_empty() && !super::schema::decimal(value)) {
        return Err(EditError::InvalidValue);
    }
    if value.is_empty()
        && node
            .attribute((XSI, "nil"))
            .is_some_and(|nil| matches!(nil, "true" | "1"))
    {
        return patch::replacements_plan(input, Vec::new());
    }
    let key = (!value.is_empty()).then(|| PropertyKey {
        namespace: Some(XS.to_owned()),
        name: "string".to_owned(),
    });
    replace(input, node, key.as_ref(), value)
}

/// Preserve element spelling, unrelated attributes and comments while replacing its semantic value.
fn replace(
    input: &str,
    node: Node<'_, '_>,
    key: Option<&PropertyKey>,
    value: &str,
) -> Result<EditPlan, EditError> {
    let text = patch::text_plan(input, node.range(), value)?;
    let mut replacements = text.replacements().to_vec();
    let (name, annotation, declaration) = if let Some(key) = key {
        let (value, declaration) = if let Some(existing) =
            node.attribute((XSI, "type")).filter(|name| {
                let (prefix, local) = name
                    .split_once(':')
                    .map_or((None, *name), |(prefix, local)| (Some(prefix), local));
                local == key.name && node.lookup_namespace_uri(prefix) == key.namespace.as_deref()
            }) {
            (existing.to_owned(), None)
        } else {
            types::qname(node, key)?
        };
        ("type", value, declaration)
    } else {
        ("nil", "true".to_owned(), None)
    };
    let old = node.attribute_node((XSI, name));
    let opposite = node.attribute_node((XSI, if name == "type" { "nil" } else { "type" }));
    let current = old.or(opposite).ok_or(EditError::UnsupportedValue)?;
    let raw = &input[current.range()];
    let prefix = raw.split_once(':').ok_or(EditError::UnsupportedValue)?.0;
    if old.is_none() || current.value() != annotation {
        replacements.push(Replacement {
            range: current.range(),
            text: format!("{prefix}:{name}=\"{annotation}\""),
        });
    }
    if old.is_some()
        && let Some(opposite) = opposite
    {
        replacements.push(Replacement {
            range: opposite.range(),
            text: String::new(),
        });
    }
    if let Some(declaration) = declaration {
        replacements.push(Replacement {
            range: current.range().end..current.range().end,
            text: declaration,
        });
    }
    patch::replacements_plan(input, replacements)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercise the production writer with namespace rebinding and byte-sensitive surrounding text.
    fn edit(input: &str, value: &str) -> EditPlan {
        let document = roxmltree::Document::parse(input).unwrap();
        let node = document.root_element().first_element_child().unwrap();
        replace_bound(input, node, value).unwrap()
    }

    #[test]
    fn number_bounds_set_clear_without_rewriting_neighbors() {
        let input = "\u{feff}<Properties xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:s='http://www.w3.org/2001/XMLSchema-instance' xmlns:x='http://www.w3.org/2001/XMLSchema'>\r\n\t<MinValue s:nil='true' custom='keep'/><Comment>untouched</Comment>\r\n</Properties>";
        let plan = edit(input, "-10.25");
        assert_eq!(
            plan.output(),
            input
                .replace("s:nil='true'", "s:type=\"x:string\"")
                .replace("custom='keep'/>", "custom='keep'>-10.25</MinValue>")
        );
        assert!(edit(plan.output(), "-10.25").is_empty());
        let cleared = edit(plan.output(), "");
        assert_eq!(
            cleared.output(),
            plan.output()
                .replace("s:type=\"x:string\"", "s:nil=\"true\"")
                .replace("-10.25", "")
        );
        assert!(edit(cleared.output(), "").is_empty());
        assert!(edit(&input.replace("nil='true'", "nil='1'"), "").is_empty());
    }

    #[test]
    fn bounds_introduce_only_a_missing_type_namespace_and_preserve_comments() {
        let input = "<Properties xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:s='http://www.w3.org/2001/XMLSchema-instance'><MaxValue s:nil='true'><!--keep--></MaxValue></Properties>";
        let plan = edit(input, "10");
        assert!(plan.output().contains("<!--keep-->10</MaxValue>"));
        assert!(plan.output().contains("http://www.w3.org/2001/XMLSchema\""));
        let document = roxmltree::Document::parse(plan.output()).unwrap();
        assert!(number_bound(
            document.root_element().first_element_child().unwrap()
        ));
        let empty = input.replace("><!--keep--></MaxValue>", "/>");
        assert!(edit(&empty, "10").output().contains(">10</MaxValue>"));
    }
}
