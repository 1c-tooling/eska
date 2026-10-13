//! Address existing UI/language map keys and layout collections without regenerating their owner.

use std::ops::Range;

use yaml_edit::{AsYaml, Mapping, ScalarValue, YamlNode};

use super::{comments, range, splice, styled_string, unsupported};
use crate::project_core::source_format::{
    Error, FormId, FormLayout, FormPropertyName, FormPropertyValue, yaml,
};

#[cfg(test)]
mod tests;

/// Mutate only changed language scalars or a boolean while preserving the complete property's trivia.
pub(in crate::project_core::source_format::edit) fn form_property(
    input: &str,
    element: Option<&FormId>,
    property: FormPropertyName,
    previous: &FormPropertyValue,
    next: &FormPropertyValue,
) -> Result<String, Error> {
    let (_, file) = yaml::decode_with_tree(input)?;
    let form = yaml::at(&file, &[yaml::Step::Key("form")]).ok_or_else(unsupported)?;
    let owner = if let Some(id) = element {
        form.as_mapping()
            .and_then(|form| form.get("elements"))
            .and_then(|elements| {
                elements
                    .as_mapping()
                    .and_then(|elements| elements.get(id.as_str()))
            })
            .ok_or_else(unsupported)?
    } else {
        form
    };
    let properties = owner
        .as_mapping()
        .and_then(|owner| owner.get("properties"))
        .and_then(|properties| properties.as_mapping().cloned())
        .ok_or_else(unsupported)?;
    let key = property.as_str();
    let node = properties.get(key).ok_or_else(unsupported)?;
    let mut patches = Vec::new();
    match (previous, next) {
        (FormPropertyValue::Bool(_), FormPropertyValue::Bool(value)) => {
            patches.push(scalar_patch(&properties, key, ScalarValue::from(*value))?);
        }
        (FormPropertyValue::Localized(old), FormPropertyValue::Localized(new)) => {
            let localized = node.as_mapping().ok_or_else(unsupported)?;
            let text = localized
                .get("text")
                .and_then(|text| text.as_mapping().cloned())
                .ok_or_else(unsupported)?;
            for (language, previous) in &old.text {
                let value = new.text.get(language).ok_or_else(unsupported)?;
                if previous != value {
                    let scalar = text
                        .get(language)
                        .and_then(|node| node.as_scalar().cloned())
                        .ok_or_else(unsupported)?;
                    patches.push(scalar_patch(
                        &text,
                        language,
                        styled_string(value, Some(&scalar)),
                    )?);
                }
            }
            if old.formatted != new.formatted {
                patches.push(scalar_patch(
                    localized,
                    "formatted",
                    ScalarValue::from(new.formatted.ok_or_else(unsupported)?),
                )?);
            }
        }
        _ => return Err(unsupported()),
    }
    apply_scalars(input, patches)
}

/// Original scalar spans are captured before any live CST handle can shift following offsets.
struct ScalarPatch {
    parent: Mapping,
    key: String,
    span: Range<usize>,
    value: ScalarValue,
}

/// A native value edit can replace an existing scalar but cannot add a new map entry.
fn scalar_patch(parent: &Mapping, key: &str, value: ScalarValue) -> Result<ScalarPatch, Error> {
    let node = parent.get(key).ok_or_else(unsupported)?;
    node.as_scalar().ok_or_else(unsupported)?;
    Ok(ScalarPatch {
        parent: parent.clone(),
        key: key.into(),
        span: range(&node)?,
        value,
    })
}

/// Only selected scalars are spliced, so literal factory transport cannot consume neighboring entries.
fn apply_scalars(input: &str, mut patches: Vec<ScalarPatch>) -> Result<String, Error> {
    patches.sort_by_key(|patch| std::cmp::Reverse(patch.span.start));
    let mut output = input.to_owned();
    for patch in patches {
        let original = checked_text(input, patch.span.clone())?;
        patch.parent.set(&patch.key, patch.value);
        let replacement = patch
            .parent
            .get(&patch.key)
            .ok_or_else(unsupported)?
            .as_node()
            .ok_or_else(unsupported)?
            .to_string();
        output = splice(&output, patch.span, original, replacement)?;
    }
    Ok(output)
}

/// Capture every original span before mutation, then replace only changed existing ordered slots.
pub(in crate::project_core::source_format::edit) fn form_layout(
    input: &str,
    previous: &FormLayout,
    next: &FormLayout,
) -> Result<String, Error> {
    let (_, file) = yaml::decode_with_tree(input)?;
    let layout = yaml::at(&file, &[yaml::Step::Key("form"), yaml::Step::Key("layout")])
        .and_then(|node| node.as_mapping().cloned())
        .ok_or_else(unsupported)?;
    let mut patches = Vec::new();
    if previous.roots != next.roots {
        let node = layout.get("roots").ok_or_else(unsupported)?;
        patches.push((range(&node)?, node, &next.roots));
    }
    if previous.children.keys().ne(next.children.keys()) {
        return Err(unsupported());
    }
    if previous.children != next.children {
        let children = layout
            .get("children")
            .and_then(|node| node.as_mapping().cloned())
            .ok_or_else(unsupported)?;
        for (id, values) in &next.children {
            if previous.children.get(id) != Some(values) {
                let node = children.get(id.as_str()).ok_or_else(unsupported)?;
                patches.push((range(&node)?, node, values));
            }
        }
    }
    patches.sort_by_key(|patch| std::cmp::Reverse(patch.0.start));
    let mut output = input.to_owned();
    for (span, node, values) in patches {
        let original = checked_text(input, span.clone())?;
        let replacement = sequence(input, &node, values)?;
        output = splice(&output, span, original, replacement)?;
    }
    Ok(output)
}

/// Existing positions keep their scalar style and comments; lossy collection edits fail before publication.
fn sequence(input: &str, node: &YamlNode, values: &[FormId]) -> Result<String, Error> {
    let sequence = node.as_sequence().ok_or_else(unsupported)?;
    if !sequence.is_flow_style() {
        for index in values.len()..sequence.len() {
            let item = sequence.get(index).ok_or_else(unsupported)?;
            let line = super::reference_value::member_line(input, &item)?;
            if !comments(input.get(line).ok_or_else(unsupported)?).is_empty() {
                return Err(unsupported());
            }
        }
    }
    // Removing the final block entry through the CST factory emits an absent YAML value.
    // An explicit empty collection preserves the domain shape; splice still refuses lost comments.
    if values.is_empty() {
        return Ok("[]".into());
    }
    let previous = sequence.len();
    for (index, value) in values.iter().take(previous).enumerate() {
        let scalar = sequence
            .get(index)
            .and_then(|node| node.as_scalar().cloned())
            .ok_or_else(unsupported)?;
        if scalar.as_string() != value.as_str()
            && !sequence.set(
                index,
                ScalarValue::with_style(value.as_str(), ScalarValue::from_scalar(&scalar).style()),
            )
        {
            return Err(unsupported());
        }
    }
    for index in (values.len()..previous).rev() {
        sequence.remove(index).ok_or_else(unsupported)?;
    }
    for value in values.iter().skip(previous) {
        sequence.push(ScalarValue::string(value.as_str()));
    }
    Ok(sequence.as_node().ok_or_else(unsupported)?.to_string())
}

/// Mixed transports within a changed subtree are ambiguous; all untouched bytes remain caller-owned.
fn checked_text(input: &str, span: Range<usize>) -> Result<&str, Error> {
    let text = input.get(span).ok_or_else(unsupported)?;
    if text.contains("\r\n") && text.replace("\r\n", "").contains('\n') {
        return Err(unsupported());
    }
    Ok(text)
}
