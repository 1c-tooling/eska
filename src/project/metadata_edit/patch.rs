//! XML ranges refer to the original bytes; parsing is used only to locate and validate.

use std::ops::Range;

use roxmltree::{Document, Node};

use super::{EditError, EditPlan, Replacement};

/// Replace one scalar's content while preserving every byte outside its required range.
pub fn text_plan(input: &str, range: Range<usize>, value: &str) -> Result<EditPlan, EditError> {
    let parsed = document(input)?;
    let node = parsed
        .descendants()
        .find(|node| node.is_element() && node.range() == range)
        .ok_or(EditError::UnsupportedValue)?;
    if node
        .children()
        .any(|node| node.is_element() || node.is_pi())
    {
        return Err(EditError::UnsupportedValue);
    }
    let texts: Vec<_> = node.children().filter(Node::is_text).collect();
    // A comment splitting a text value has no unique insertion location.
    if texts.len() > 1 {
        return Err(EditError::UnsupportedValue);
    }
    let current = texts.first().and_then(Node::text).unwrap_or_default();
    if current == value {
        return Ok(EditPlan {
            original: input.to_owned(),
            updated: input.to_owned(),
            replacements: Vec::new(),
        });
    }
    let escaped = escape(value)?;
    let replacement = if let Some(text) = texts.first() {
        Replacement {
            range: text.range(),
            text: escaped,
        }
    } else {
        empty_content(input, node, escaped)?
    };
    let mut updated = input.to_owned();
    updated.replace_range(replacement.range.clone(), &replacement.text);
    // Reparse before touching the filesystem, including XML 1.0 character validity.
    let candidate = document(&updated)?;
    let changed = candidate
        .descendants()
        .find(|item| item.is_element() && item.range().start == node.range().start)
        .ok_or(EditError::InvalidXml)?;
    if changed
        .children()
        .filter(Node::is_text)
        .filter_map(|text| text.text())
        .collect::<String>()
        != value
    {
        return Err(EditError::UnsupportedValue);
    }
    Ok(EditPlan {
        original: input.to_owned(),
        updated,
        replacements: vec![replacement],
    })
}

/// Bound input and nesting using the same envelope limits as metadata reading.
fn document(input: &str) -> Result<Document<'_>, EditError> {
    crate::project::metadata_parser::check_envelope(input).map_err(|_| EditError::InvalidXml)?;
    Document::parse_with_options(
        input,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|_| EditError::InvalidXml)
}

/// Apply disjoint backend-computed ranges and validate the complete resulting XML.
pub(super) fn replacements_plan(
    input: &str,
    mut replacements: Vec<Replacement>,
) -> Result<EditPlan, EditError> {
    replacements.retain(|replacement| {
        input.get(replacement.range.clone()) != Some(replacement.text.as_str())
    });
    // An insertion at an edited range's start belongs before that range, not inside it.
    replacements.sort_by_key(|replacement| (replacement.range.start, replacement.range.end));
    let mut end = 0;
    for replacement in &replacements {
        if replacement.range.start < end || input.get(replacement.range.clone()).is_none() {
            return Err(EditError::UnsupportedValue);
        }
        end = replacement.range.end;
    }
    let mut updated = input.to_owned();
    for replacement in replacements.iter().rev() {
        updated.replace_range(replacement.range.clone(), &replacement.text);
    }
    document(&updated)?;
    Ok(EditPlan {
        original: input.to_owned(),
        updated,
        replacements,
    })
}

/// Expand only the closing slash of an empty element, retaining its original qualified name.
fn empty_content(input: &str, node: Node<'_, '_>, value: String) -> Result<Replacement, EditError> {
    let range = node.range();
    let source = &input[range.clone()];
    if source.ends_with("/>") {
        let name = source[1..]
            .split(|character: char| character.is_whitespace() || matches!(character, '/' | '>'))
            .next()
            .filter(|name| !name.is_empty())
            .ok_or(EditError::UnsupportedValue)?;
        Ok(Replacement {
            range: range.end - 2..range.end,
            text: format!(">{value}</{name}>"),
        })
    } else {
        let end = source.rfind("</").ok_or(EditError::UnsupportedValue)? + range.start;
        Ok(Replacement {
            range: end..end,
            text: value,
        })
    }
}

/// Escape text rather than interpolating markup; preserve CR explicitly against normalization.
fn escape(value: &str) -> Result<String, EditError> {
    let mut result = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '\r' => result.push_str("&#13;"),
            '\t'
            | '\n'
            | '\u{20}'..='\u{d7ff}'
            | '\u{e000}'..='\u{fffd}'
            | '\u{10000}'..='\u{10ffff}' => {
                result.push(character);
            }
            _ => return Err(EditError::InvalidValue),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Select a property with the real parser so comments and repeated names remain significant.
    fn edit(input: &str, value: &str) -> EditPlan {
        let parsed = Document::parse(input).expect("input");
        let range = parsed
            .root_element()
            .first_element_child()
            .expect("field")
            .range();
        text_plan(input, range, value).expect("plan")
    }

    #[test]
    fn changes_only_content_bytes_with_bom_crlf_prefixes_entities_and_comments() {
        let input = "\u{feff}<?xml version=\"1.0\"?>\r\n<r xmlns:p='urn:test'>\r\n\t<p:v z='2' a=\"1\"><!--before-->A&amp;B<!--after--></p:v>\r\n\t<p:v>unchanged</p:v>\r\n</r>\r\n";
        let plan = edit(input, "Новый <текст> &\r\nстрока");
        assert_eq!(
            plan.output(),
            input.replace("A&amp;B", "Новый &lt;текст&gt; &amp;&#13;\nстрока")
        );
        assert_eq!(plan.replacements().len(), 1);
        assert!(edit(input, "A&B").is_empty());
    }

    #[test]
    fn expands_empty_elements_and_replaces_cdata_without_serializing_siblings() {
        for (input, expected) in [
            ("<r><v x='1' /></r>", "<r><v x='1' >a&lt;b</v></r>"),
            (
                "<r><v><!--keep--></v></r>",
                "<r><v><!--keep-->a&lt;b</v></r>",
            ),
            ("<r><v><![CDATA[old]]></v></r>", "<r><v>a&lt;b</v></r>"),
        ] {
            assert_eq!(edit(input, "a<b").output(), expected);
        }
    }

    #[test]
    fn rejects_ambiguous_text_and_invalid_characters() {
        for input in ["<r><v>a<!--inside-->b</v></r>", "<r><v><child/></v></r>"] {
            let parsed = Document::parse(input).expect("input");
            assert!(
                text_plan(
                    input,
                    parsed
                        .root_element()
                        .first_element_child()
                        .expect("field")
                        .range(),
                    "new"
                )
                .is_err()
            );
        }
        let input = "<r><v>old</v></r>";
        let range = Document::parse(input)
            .expect("input")
            .root_element()
            .first_element_child()
            .expect("field")
            .range();
        assert!(matches!(
            text_plan(input, range, "\u{0}"),
            Err(EditError::InvalidValue)
        ));
    }
}
