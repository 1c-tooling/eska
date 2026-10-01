//! A type link is one nullable property with a source field and an unsigned element index.

use super::{
    EditError, EditPlan, EditableField, FieldStep, Replacement, ScalarSchema, choice_fields, patch,
    schema::{MD, READABLE},
    types,
};
use crate::project::metadata_model::PropertyKey;
use roxmltree::Node;

pub const DOMAIN: &str = "TypeLinkField";

/// Only the empty wrapper or the exact Designer pair is a reviewed writable shape.
pub(super) fn is_link(node: Node<'_, '_>) -> bool {
    if node.tag_name().name() != "LinkByType"
        || !matches!(node.tag_name().namespace(), Some(MD | READABLE))
        || node.attributes().len() != 0
        || node.children().any(|child| {
            child.is_pi()
                || (child.is_text() && child.text().is_some_and(|text| !text.trim().is_empty()))
        })
    {
        return false;
    }
    let fields: Vec<_> = node.children().filter(Node::is_element).collect();
    fields.is_empty()
        || (fields.len() == 2
            && fields[0].has_tag_name((READABLE, "DataPath"))
            && fields[1].has_tag_name((READABLE, "LinkItem"))
            && fields.iter().all(|field| {
                field.attributes().len() == 0 && field.children().all(|child| child.is_text())
            })
            && fields[1]
                .text()
                .is_some_and(|value| value.parse::<u32>().is_ok()))
}

/// The source is addressed by its existing wrapper, so setting an empty link does not invent a field address.
pub(super) fn fields(node: Node<'_, '_>, path: &[FieldStep]) -> Vec<EditableField> {
    if !is_link(node) {
        return Vec::new();
    }
    let source = node
        .children()
        .find(|child| child.has_tag_name((READABLE, "DataPath")));
    let mut result = vec![EditableField {
        path: path.to_vec(),
        value: source
            .and_then(|node| node.text())
            .unwrap_or_default()
            .to_owned(),
        schema: ScalarSchema::Reference {
            domain: DOMAIN.into(),
            nullable: true,
            reference_type: None,
        },
        language: None,
    }];
    if let Some(item) = node
        .children()
        .find(|child| child.has_tag_name((READABLE, "LinkItem")))
    {
        let mut path = path.to_vec();
        path.push(FieldStep {
            key: PropertyKey {
                namespace: Some(READABLE.into()),
                name: "LinkItem".into(),
            },
            occurrence: 0,
        });
        result.push(EditableField {
            path,
            value: item.text().unwrap_or_default().to_owned(),
            schema: ScalarSchema::Integer {
                min: 0,
                max: i64::from(u32::MAX),
            },
            language: None,
        });
    }
    result
}

/// Selecting a source preserves an existing index; clearing removes only the two value elements.
pub(super) fn replace(input: &str, node: Node<'_, '_>, value: &str) -> Result<EditPlan, EditError> {
    if !is_link(node) || (!value.is_empty() && !choice_fields::valid_path(value)) {
        return Err(EditError::InvalidValue);
    }
    let children: Vec<_> = node.children().filter(Node::is_element).collect();
    if value.is_empty() {
        return patch::replacements_plan(
            input,
            children
                .into_iter()
                .map(|child| Replacement {
                    range: types::line_range(input, child.range()),
                    text: String::new(),
                })
                .collect(),
        );
    }
    if let Some(source) = children.first() {
        return patch::text_plan(input, source.range(), value);
    }
    let (name, declaration) = types::qname(
        node,
        &PropertyKey {
            namespace: Some(READABLE.into()),
            name: "DataPath".into(),
        },
    )?;
    let index = name.rsplit_once(':').map_or_else(
        || "LinkItem".to_owned(),
        |(prefix, _)| format!("{prefix}:LinkItem"),
    );
    let value = patch::escape(value)?;
    let indent = types::indentation(input, node.range().start);
    let fragment = indent.map_or_else(|| format!("<{name}>{value}</{name}><{index}>0</{index}>"), |indent| {
        let newline = types::newline(input);
        let unit = if indent.contains('\t') { "\t" } else { "    " };
        format!(
            "{newline}{indent}{unit}<{name}>{value}</{name}>{newline}{indent}{unit}<{index}>0</{index}>{newline}{indent}"
        )
    });
    let content = if let Some(text) = node.last_child().filter(Node::is_text) {
        Replacement {
            range: text.range(),
            text: fragment,
        }
    } else {
        patch::empty_content(input, node, fragment)?
    };
    let mut changes = vec![content];
    if let Some(declaration) = declaration {
        let raw = &input[node.range()];
        let name_length = raw[1..]
            .split(|ch: char| ch.is_whitespace() || matches!(ch, '/' | '>'))
            .next()
            .ok_or(EditError::UnsupportedValue)?
            .len();
        let position = node.range().start + 1 + name_length;
        changes.push(Replacement {
            range: position..position,
            text: declaration,
        });
    }
    patch::replacements_plan(input, changes)
}
