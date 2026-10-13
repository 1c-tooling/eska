//! Splice one CST value subtree; untouched document bytes never pass through a serializer.

use std::ops::Range;

use yaml_edit::{AsYaml, Mapping, ScalarValue, SyntaxKind, YamlFile};

use super::super::{Error, PropertyValue, yaml};

pub(super) use super::super::yaml::Step;

mod form;
#[cfg(test)]
mod reference_tests;
mod reference_value;
mod type_value;

pub(super) use form::{form_layout, form_property};

/// Mutate the selected value only and preserve its surrounding YAML verbatim.
pub(super) fn replace(
    input: &str,
    steps: &[Step],
    previous: &PropertyValue,
    next: &PropertyValue,
) -> Result<String, Error> {
    let parsed = YamlFile::parse(input);
    if !parsed.ok() {
        return Err(unsupported());
    }
    let file = parsed.tree();
    let node = yaml::at(&file, steps).ok_or_else(unsupported)?;
    let mapping = node.as_mapping().ok_or_else(unsupported)?;
    let original = range(mapping)?;
    let original_text = input.get(original.clone()).ok_or_else(unsupported)?;
    if original_text.contains("\r\n") && original_text.replace("\r\n", "").contains('\n') {
        return Err(unsupported());
    }
    if matches!(
        (previous, next),
        (PropertyValue::ReferenceList(_), PropertyValue::Empty)
    ) {
        return clear_reference(input, original, original_text);
    }
    update_value(mapping, previous, next)?;
    let mut replacement = mapping.to_string();
    // yaml-edit's literal factory omits the terminating transport newline when the
    // semantic string has strip chomping. Retain the original subtree boundary.
    if scalar(previous).is_some() && matches!(next, PropertyValue::Empty | PropertyValue::Nil) {
        remove_scalar(mapping, &mut replacement)?;
    }
    let output = splice(input, original, original_text, replacement)?;
    if let (PropertyValue::ReferenceList(old), PropertyValue::ReferenceList(new)) = (previous, next)
    {
        let steps = steps
            .iter()
            .cloned()
            .chain([Step::Key("value"), Step::Key("items")])
            .collect::<Vec<_>>();
        return reference_value::membership(&output, &steps, &old.items, &new.items);
    }
    Ok(output)
}

/// Splice a named direct value, preserving neighboring keys, comments and scalar styles.
pub(super) fn replace_direct(
    input: &str,
    steps: &[Step],
    previous: &PropertyValue,
    next: &PropertyValue,
) -> Result<String, Error> {
    let (_, file) = yaml::decode_with_tree(input)?;
    let node = yaml::at(&file, steps).ok_or_else(unsupported)?;
    let original = range(&node)?;
    let original_text = input.get(original.clone()).ok_or_else(unsupported)?;
    if original_text.contains("\r\n") && original_text.replace("\r\n", "").contains('\n') {
        return Err(unsupported());
    }
    if matches!(
        (previous, next),
        (PropertyValue::ReferenceList(_), PropertyValue::Empty)
    ) {
        return clear_reference(input, original, original_text);
    }
    let Some((Step::Key(key), parent)) = steps.split_last() else {
        return Err(unsupported());
    };
    let mapping = yaml::at(&file, parent)
        .and_then(|node| node.as_mapping().cloned())
        .ok_or_else(unsupported)?;
    let mut flow_replacement = None;
    match (previous, next) {
        (PropertyValue::TypeDescription(old), PropertyValue::TypeDescription(new)) => {
            type_value::replace(node.as_mapping().ok_or_else(unsupported)?, old, new)?;
        }
        (PropertyValue::ReferenceList(old), PropertyValue::ReferenceList(new))
            if old.native == new.native =>
        {
            reference_items(
                node.as_sequence().ok_or_else(unsupported)?,
                &old.items,
                &new.items,
            )?;
        }
        (PropertyValue::Localized(old), PropertyValue::Localized(new))
            if old.len() == new.len()
                && old.iter().zip(new).all(|(a, b)| a.language == b.language) =>
        {
            localized_direct(&node, old, new)?;
        }
        (
            _,
            PropertyValue::Localized(_)
            | PropertyValue::TypeDescription(_)
            | PropertyValue::ReferenceList(_),
        )
        | (
            PropertyValue::Localized(_)
            | PropertyValue::TypeDescription(_)
            | PropertyValue::ReferenceList(_),
            _,
        ) => {
            return Err(unsupported());
        }
        (_, PropertyValue::Boolean(value)) => mapping.set(*key, ScalarValue::from(*value)),
        (_, PropertyValue::Text(value) | PropertyValue::Enumeration(value)) => {
            set_direct_string(&mapping, key, value, node.as_scalar());
        }
        (_, PropertyValue::Integer(value)) => {
            set_direct_string(&mapping, key, value.as_str(), node.as_scalar());
        }
        (_, PropertyValue::Reference(value)) => {
            set_direct_string(&mapping, key, value.as_str(), node.as_scalar());
        }
        (PropertyValue::EnumFillValue(old), PropertyValue::EnumFillValue(new))
            if old.native == new.native =>
        {
            set_direct_string(&mapping, key, new.reference.as_str(), node.as_scalar());
        }
        (_, PropertyValue::EnumFillValue(_)) | (PropertyValue::EnumFillValue(_), _) => {
            return Err(unsupported());
        }
        (_, PropertyValue::Empty | PropertyValue::Nil) => {
            if let Some(state) = node.as_mapping() {
                set_empty_state(state, next)?;
            } else {
                flow_replacement = Some(
                    serde_json::to_string(&super::super::readable::value_to_wire(next)?)
                        .map_err(|_| unsupported())?,
                );
            }
        }
    }
    let updated = yaml::at(&file, steps).ok_or_else(unsupported)?;
    let replacement = match flow_replacement {
        Some(value) => value,
        None => updated.as_node().ok_or_else(unsupported)?.to_string(),
    };
    let output = splice(input, original, original_text, replacement)?;
    if let (PropertyValue::ReferenceList(old), PropertyValue::ReferenceList(new)) = (previous, next)
    {
        return reference_value::membership(&output, steps, &old.items, &new.items);
    }
    Ok(output)
}

/// Updating localized contents retains each scalar's style and the unchanged language-key order.
fn localized_direct(
    node: &yaml_edit::YamlNode,
    old: &[super::super::LocalizedText],
    new: &[super::super::LocalizedText],
) -> Result<(), Error> {
    let sequence = node.as_sequence().ok_or_else(unsupported)?;
    for (index, (before, after)) in old.iter().zip(new).enumerate() {
        if before.text != after.text {
            let entry = sequence
                .get(index)
                .and_then(|node| node.as_mapping().cloned())
                .ok_or_else(unsupported)?;
            let scalar = entry
                .get("text")
                .and_then(|node| node.as_scalar().cloned())
                .ok_or_else(unsupported)?;
            set_direct_string(&entry, "text", &after.text, Some(&scalar));
        }
    }
    Ok(())
}

/// Empty replaces the entire bound value; annotations on removed members require manual resolution.
fn clear_reference(
    input: &str,
    original: Range<usize>,
    original_text: &str,
) -> Result<String, Error> {
    splice(
        input,
        original,
        original_text,
        String::from("{\"state\": \"empty\"}"),
    )
}

/// New tokens adopt the subtree's transport; surrounding bytes and comment content stay exact.
fn splice(
    input: &str,
    original: Range<usize>,
    original_text: &str,
    mut replacement: String,
) -> Result<String, Error> {
    yaml::preserve_transport(original_text, &mut replacement);
    if comments(original_text) != comments(&replacement) {
        return Err(unsupported());
    }
    let mut output = input.to_owned();
    output.replace_range(original, &replacement);
    Ok(output)
}

/// Keep a present string's style; a state wrapper has no string style to preserve.
fn set_direct_string(
    mapping: &Mapping,
    key: &str,
    value: &str,
    scalar: Option<&yaml_edit::Scalar>,
) {
    mapping.set(key, styled_string(value, scalar));
}

/// Scalar splices and mapping edits share the same quoting and multiline boundary.
fn styled_string(value: &str, scalar: Option<&yaml_edit::Scalar>) -> ScalarValue {
    scalar.map_or_else(
        || ScalarValue::string(value),
        |scalar| {
            let style = ScalarValue::from_scalar(scalar).style();
            let style = if value.contains('\n')
                && matches!(
                    style,
                    yaml_edit::ScalarStyle::SingleQuoted | yaml_edit::ScalarStyle::Plain
                ) {
                yaml_edit::ScalarStyle::DoubleQuoted
            } else {
                style
            };
            ScalarValue::with_style(value, style)
        },
    )
}

/// Change one scalar domain or localized text sequence without regenerating its enclosing owner.
fn update_value(
    mapping: &Mapping,
    previous: &PropertyValue,
    next: &PropertyValue,
) -> Result<(), Error> {
    if let Some((state, value)) = scalar(next) {
        if matches!(previous, PropertyValue::Empty | PropertyValue::Nil) {
            set_string(mapping, "state", state)?;
            mapping.set("value", value);
            return Ok(());
        }
        if scalar(previous).is_some_and(|(old_state, _)| old_state == state) {
            match next {
                PropertyValue::Boolean(value) => mapping.set("value", ScalarValue::from(*value)),
                PropertyValue::Text(value) | PropertyValue::Enumeration(value) => {
                    set_string(mapping, "value", value)?;
                }
                PropertyValue::Integer(value) => set_string(mapping, "value", value.as_str())?,
                PropertyValue::Reference(value) => set_string(mapping, "value", value.as_str())?,
                _ => return Err(unsupported()),
            }
            return Ok(());
        }
    }
    match (previous, next) {
        (PropertyValue::EnumFillValue(old), PropertyValue::EnumFillValue(new))
            if old.native == new.native =>
        {
            let content = mapping
                .get("value")
                .and_then(|node| node.as_mapping().cloned())
                .ok_or_else(unsupported)?;
            set_string(&content, "reference", new.reference.as_str())?;
        }
        (PropertyValue::TypeDescription(old), PropertyValue::TypeDescription(new)) => {
            let data = mapping
                .get("value")
                .and_then(|node| node.as_mapping().cloned())
                .and_then(|value| value.get("data"))
                .and_then(|node| node.as_mapping().cloned())
                .ok_or_else(unsupported)?;
            type_value::replace(&data, old, new)?;
        }
        (PropertyValue::ReferenceList(old), PropertyValue::ReferenceList(new))
            if old.native == new.native =>
        {
            let items = mapping
                .get("value")
                .and_then(|node| node.as_mapping().cloned())
                .and_then(|node| node.get("items"))
                .and_then(|node| node.as_sequence().cloned())
                .ok_or_else(unsupported)?;
            reference_items(&items, &old.items, &new.items)?;
        }
        (PropertyValue::Empty | PropertyValue::Nil, PropertyValue::Empty | PropertyValue::Nil) => {
            set_empty_state(mapping, next)?;
        }
        (_, PropertyValue::Empty | PropertyValue::Nil) if scalar(previous).is_some() => {
            set_empty_state(mapping, next)?;
        }
        (PropertyValue::Localized(old), PropertyValue::Localized(new))
            if old.len() == new.len()
                && old.iter().zip(new).all(|(a, b)| a.language == b.language) =>
        {
            let sequence = mapping
                .get("value")
                .and_then(|node| node.as_sequence().cloned())
                .ok_or_else(unsupported)?;
            for (index, (old, new)) in old.iter().zip(new).enumerate() {
                if old.text != new.text {
                    let entry = sequence
                        .get(index)
                        .and_then(|node| node.as_mapping().cloned())
                        .ok_or_else(unsupported)?;
                    set_string(&entry, "text", &new.text)?;
                }
            }
        }
        _ => return Err(unsupported()),
    }
    Ok(())
}

/// Native integers and enum spellings remain strings; booleans use an actual YAML boolean token.
fn scalar(value: &PropertyValue) -> Option<(&'static str, ScalarValue)> {
    match value {
        PropertyValue::Text(value) => Some(("text", ScalarValue::string(value))),
        PropertyValue::Boolean(value) => Some(("boolean", ScalarValue::from(*value))),
        PropertyValue::Integer(value) => Some(("integer", ScalarValue::string(value.as_str()))),
        PropertyValue::Enumeration(value) => Some(("enumeration", ScalarValue::string(value))),
        PropertyValue::Reference(value) => Some(("reference", ScalarValue::string(value.as_str()))),
        _ => None,
    }
}

/// Scalar sequence mutation retains existing styles; outer guards refuse lost or moved annotations.
fn reference_items(
    sequence: &yaml_edit::Sequence,
    old: &[super::super::MetadataReference],
    new: &[super::super::MetadataReference],
) -> Result<(), Error> {
    for (index, (previous, next)) in old.iter().zip(new).enumerate() {
        if previous != next {
            let scalar = sequence
                .get(index)
                .and_then(|node| node.as_scalar().cloned())
                .ok_or_else(unsupported)?;
            let style = ScalarValue::from_scalar(&scalar).style();
            if !sequence.set(index, ScalarValue::with_style(next.as_str(), style)) {
                return Err(unsupported());
            }
        }
    }
    Ok(())
}

/// Empty and nil have no scalar value field and retain their original state-token style.
fn set_empty_state(mapping: &Mapping, next: &PropertyValue) -> Result<(), Error> {
    set_string(
        mapping,
        "state",
        if matches!(next, PropertyValue::Empty) {
            "empty"
        } else {
            "nil"
        },
    )
}

/// Removing only key/value syntax keeps comments attached to the old scalar entry.
fn remove_scalar(mapping: &Mapping, replacement: &mut String) -> Result<(), Error> {
    // Keep comments and trivia when removing the value field. Detaching a MappingEntry
    // would discard comments which the CST associates with that entry.
    if mapping.is_flow_style() {
        return Err(unsupported());
    }
    let entry = mapping.find_entry_by_key("value").ok_or_else(unsupported)?;
    let key = range(&entry.key_node().ok_or_else(unsupported)?)?;
    let value = range(&entry.value_node().ok_or_else(unsupported)?)?;
    let start = range(mapping)?.start;
    let removal = key.start.checked_sub(start).ok_or_else(unsupported)?
        ..value.end.checked_sub(start).ok_or_else(unsupported)?;
    replacement.get(removal.clone()).ok_or_else(unsupported)?;
    replacement.replace_range(removal, "");
    Ok(())
}

/// Keep existing scalar style; the strict semantic reader verifies escaping after the splice.
fn set_string(mapping: &Mapping, key: &str, value: &str) -> Result<(), Error> {
    let current = mapping.get(key).ok_or_else(unsupported)?;
    let scalar = current.as_scalar().ok_or_else(unsupported)?;
    mapping.set(
        key,
        ScalarValue::with_style(value, ScalarValue::from_scalar(scalar).style()),
    );
    Ok(())
}

/// Public CST handles expose byte ranges without introducing a second rowan dependency.
fn range(value: &impl AsYaml) -> Result<Range<usize>, Error> {
    let range = value.as_node().ok_or_else(unsupported)?.text_range();
    Ok(usize::from(range.start())..usize::from(range.end()))
}

/// Refuse a library rewrite which drops or relocates comment content into a scalar value.
fn comments(input: &str) -> Vec<&str> {
    yaml_edit::lex(input)
        .into_iter()
        .filter_map(|(kind, text)| (kind == SyntaxKind::COMMENT).then_some(text))
        .collect()
}

/// Unsupported layouts fail before creating a publication plan.
fn unsupported() -> Error {
    Error::root("source_edit_value_unsupported")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Direct value edits retain surrounding annotations and original scalar transport styles.
    #[test]
    fn direct_scalars_and_empty_states_preserve_neighboring_bytes() {
        for (input, previous, next) in [
            (
                "value: 'old' # inline\nother: keep\n",
                PropertyValue::Text("old".into()),
                PropertyValue::Text("new 'quoted'".into()),
            ),
            (
                "value: \"old\" # inline\r\nother: keep\r\n",
                PropertyValue::Text("old".into()),
                PropertyValue::Text("new\nline\t\\\"".into()),
            ),
            (
                "value: |-\n  old\nother: keep # neighbor\n",
                PropertyValue::Text("old".into()),
                PropertyValue::Text("new\nsecond".into()),
            ),
            (
                "value: true # inline\nother: keep\n",
                PropertyValue::Boolean(true),
                PropertyValue::Boolean(false),
            ),
            (
                "value: '0009' # inline\nother: keep\n",
                PropertyValue::Integer(super::super::super::ExactUnsigned::parse("0009").unwrap()),
                PropertyValue::Integer(
                    super::super::super::ExactUnsigned::parse("000123456789012345678901234567890")
                        .unwrap(),
                ),
            ),
            (
                "value: 'old' # inline\nother: keep\n",
                PropertyValue::Text("old".into()),
                PropertyValue::Nil,
            ),
            (
                "value: {state: nil} # inline\nother: keep\n",
                PropertyValue::Nil,
                PropertyValue::Text("true".into()),
            ),
            (
                "value: {state: empty} # inline\nother: keep\n",
                PropertyValue::Empty,
                PropertyValue::Nil,
            ),
        ] {
            let updated = replace_direct(input, &[Step::Key("value")], &previous, &next).unwrap();
            assert_eq!(
                yaml::decode(&updated).unwrap()["value"],
                super::super::super::readable::value_to_wire(&next).unwrap()
            );
            assert_eq!(comments(input), comments(&updated));
            assert_eq!(updated.split("other:").nth(1), input.split("other:").nth(1));
        }
    }

    /// Language composition is fixed; changing one localized text preserves the adjacent language.
    #[test]
    fn direct_localized_edits_preserve_order_and_literal_text() {
        let input = "value:\n  - language: ru\n    text: 'Старое' # local\n  - language: en\n    text: 'Keep' # neighbor\nother: keep\n";
        let previous = PropertyValue::Localized(vec![
            super::super::super::LocalizedText {
                language: "ru".into(),
                text: "Старое".into(),
            },
            super::super::super::LocalizedText {
                language: "en".into(),
                text: "Keep".into(),
            },
        ]);
        let mut next = previous.clone();
        let PropertyValue::Localized(items) = &mut next else {
            unreachable!()
        };
        items[0].text = "Новое\nмногострочное".into();
        let updated = replace_direct(input, &[Step::Key("value")], &previous, &next).unwrap();
        assert_eq!(
            yaml::decode(&updated).unwrap()["value"],
            super::super::super::readable::value_to_wire(&next).unwrap()
        );
        assert!(updated.ends_with("  - language: en\n    text: 'Keep' # neighbor\nother: keep\n"));
        assert_eq!(comments(input), comments(&updated));
        let mixed = input.replacen("language: ru\n", "language: ru\r\n", 1);
        assert_eq!(
            replace_direct(&mixed, &[Step::Key("value")], &previous, &next)
                .unwrap_err()
                .code,
            "source_edit_value_unsupported"
        );
    }

    #[test]
    fn removed_text_keeps_entry_comments_and_outside_trivia() {
        let input = "# outside\nvalue:\n  state: 'text' # state\n  # before value\n  value: 'old' # value\nother: 'untouched'\n";
        let updated = replace(
            input,
            &[Step::Key("value")],
            &PropertyValue::Text("old".into()),
            &PropertyValue::Nil,
        )
        .unwrap();
        assert_eq!(
            updated,
            "# outside\nvalue:\n  state: 'nil' # state\n  # before value\n   # value\nother: 'untouched'\n"
        );
        assert_eq!(
            super::super::super::yaml::decode(&updated).unwrap()["value"],
            serde_json::json!({"state":"nil"})
        );
    }

    #[test]
    fn literals_flow_and_escaped_strings_round_trip_without_touching_other_bytes() {
        for (input, old, new) in [
            (
                "value:\n  state: text\n  value: |-\n    old\nother: 'keep' # comment\n",
                "old",
                "new\nsecond",
            ),
            (
                "value: {state: text, value: 'old'} # flow\nother: 'keep'\n",
                "old",
                "new 'quoted'",
            ),
            (
                "value:\r\n  state: \"text\"\r\n  value: \"old\" # comment\r\nother: 'keep'\r\n",
                "old",
                "new\nline\t\\\"",
            ),
            (
                "value:\n  state: text\n  value: old\nother: 'keep'\n",
                "old",
                "true",
            ),
        ] {
            let updated = replace(
                input,
                &[Step::Key("value")],
                &PropertyValue::Text(old.into()),
                &PropertyValue::Text(new.into()),
            )
            .unwrap();
            let value = super::super::super::yaml::decode(&updated).unwrap();
            assert_eq!(value["value"]["value"], new, "{updated}");
            assert_eq!(updated.split("other:").nth(1), input.split("other:").nth(1));
            if input.contains("\r\n") {
                assert!(!updated.replace("\r\n", "").contains('\n'));
            }
        }
    }

    #[test]
    fn comment_loss_and_mixed_value_line_endings_never_produce_an_edit() {
        let mixed = "value:\r\n  state: text\r\n  value: old\nother: keep\n";
        assert_eq!(
            replace(
                mixed,
                &[Step::Key("value")],
                &PropertyValue::Text("old".into()),
                &PropertyValue::Text("new".into())
            )
            .unwrap_err()
            .code,
            "source_edit_value_unsupported"
        );
        let literal = "value:\n  state: text\n  value: |- # header comment\n    old\nother: keep\n";
        let result = replace(
            literal,
            &[Step::Key("value")],
            &PropertyValue::Text("old".into()),
            &PropertyValue::Text("new\nline".into()),
        );
        match result {
            Ok(updated) => assert!(updated.contains("# header comment")),
            Err(error) => assert_eq!(error.code, "source_edit_value_unsupported"),
        }
    }
}
