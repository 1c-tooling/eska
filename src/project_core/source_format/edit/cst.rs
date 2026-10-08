//! Splice one CST value subtree; untouched document bytes never pass through a serializer.

use std::ops::Range;

use yaml_edit::{AsYaml, Mapping, ScalarValue, SyntaxKind, YamlFile};

use super::super::{Error, PropertyValue, yaml};

pub(super) use super::super::yaml::Step;

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
    match (previous, next) {
        (PropertyValue::Text(_), PropertyValue::Text(value)) => {
            set_string(mapping, "value", value)?;
        }
        (PropertyValue::Empty | PropertyValue::Nil, PropertyValue::Text(value)) => {
            set_string(mapping, "state", "text")?;
            mapping.set("value", ScalarValue::string(value));
        }
        (
            PropertyValue::Empty | PropertyValue::Nil | PropertyValue::Text(_),
            PropertyValue::Empty | PropertyValue::Nil,
        ) => {
            set_string(
                mapping,
                "state",
                if matches!(next, PropertyValue::Empty) {
                    "empty"
                } else {
                    "nil"
                },
            )?;
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
    let mut replacement = mapping.to_string();
    // yaml-edit's literal factory omits the terminating transport newline when the
    // semantic string has strip chomping. Retain the original subtree boundary.
    if original_text.ends_with('\n') && !replacement.ends_with('\n') {
        replacement.push('\n');
    }
    if matches!(previous, PropertyValue::Text(_))
        && matches!(next, PropertyValue::Empty | PropertyValue::Nil)
    {
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
    }
    // Newly manufactured CST tokens use LF. Only this subtree adopts its original EOL;
    // mixed surrounding text and unrelated scalar escape spellings remain untouched.
    if original_text.contains("\r\n") {
        replacement = replacement.replace("\r\n", "\n").replace('\n', "\r\n");
    }
    if comments(original_text) != comments(&replacement) {
        return Err(unsupported());
    }
    let mut output = input.to_owned();
    output.get(original.clone()).ok_or_else(unsupported)?;
    output.replace_range(original, &replacement);
    Ok(output)
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
