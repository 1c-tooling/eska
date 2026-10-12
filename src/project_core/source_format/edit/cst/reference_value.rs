//! Concrete block-list membership edits avoid the CST factory's annotated-last-entry bug.

use yaml_edit::{AsYaml, ScalarValue, SyntaxKind};

use super::{Step, comments, range, unsupported};
use crate::project_core::source_format::{Error, MetadataReference, yaml};

/// Existing scalar edits are complete; only new or removed trailing member lines are spliced now.
pub(super) fn membership(
    input: &str,
    steps: &[Step],
    old: &[MetadataReference],
    new: &[MetadataReference],
) -> Result<String, Error> {
    if old.len() == new.len() {
        return Ok(input.into());
    }
    if old.is_empty() || new.is_empty() {
        return Err(unsupported());
    }
    let (_, file) = yaml::decode_with_tree(input)?;
    let node = yaml::at(&file, steps).ok_or_else(unsupported)?;
    let sequence = node.as_sequence().ok_or_else(unsupported)?;
    if sequence.is_flow_style() || sequence.len() != old.len() {
        return Err(unsupported());
    }
    let mut output = input.to_owned();
    if new.len() > old.len() {
        let last = sequence.get(old.len() - 1).ok_or_else(unsupported)?;
        let scalar = last.as_scalar().ok_or_else(unsupported)?;
        let span = range(scalar)?;
        let line = member_line(input, &last)?;
        let prefix = input.get(line.start..span.start).ok_or_else(unsupported)?;
        let end = input.get(line.clone()).ok_or_else(unsupported)?;
        let eol = if end.ends_with("\r\n") { "\r\n" } else { "\n" };
        let terminal = end.ends_with('\n');
        let style = ScalarValue::from_scalar(scalar).style();
        let mut addition = if terminal { String::new() } else { eol.into() };
        for (offset, value) in new.iter().skip(old.len()).enumerate() {
            if offset != 0 {
                addition.push_str(eol);
            }
            addition.push_str(prefix);
            addition.push_str(&ScalarValue::with_style(value.as_str(), style).to_yaml_string());
        }
        if terminal {
            addition.push_str(eol);
        }
        output.insert_str(line.end, &addition);
    } else {
        for index in (new.len()..old.len()).rev() {
            let item = sequence.get(index).ok_or_else(unsupported)?;
            let line = member_line(input, &item)?;
            if !comments(input.get(line.clone()).ok_or_else(unsupported)?).is_empty() {
                return Err(unsupported());
            }
            let entry = item
                .as_node()
                .ok_or_else(unsupported)?
                .ancestors()
                .find(|node| node.kind() == SyntaxKind::SEQUENCE_ENTRY)
                .ok_or_else(unsupported)?;
            let span = entry.text_range();
            if !comments(
                input
                    .get(usize::from(span.start())..usize::from(span.end()))
                    .ok_or_else(unsupported)?,
            )
            .is_empty()
            {
                return Err(unsupported());
            }
            output.replace_range(line, "");
        }
    }
    if comments(input) != comments(&output) {
        return Err(unsupported());
    }
    Ok(output)
}

/// Only one-line block scalars have an unambiguous removable or appendable member line.
fn member_line(input: &str, item: &yaml_edit::YamlNode) -> Result<std::ops::Range<usize>, Error> {
    let scalar = item.as_scalar().ok_or_else(unsupported)?;
    let span = range(scalar)?;
    if input
        .get(span.clone())
        .ok_or_else(unsupported)?
        .contains('\n')
    {
        return Err(unsupported());
    }
    let start = input
        .get(..span.start)
        .ok_or_else(unsupported)?
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    let prefix = input.get(start..span.start).ok_or_else(unsupported)?;
    let Some(after_dash) = prefix.trim_start_matches(' ').strip_prefix('-') else {
        return Err(unsupported());
    };
    if after_dash.is_empty() || !after_dash.bytes().all(|ch| ch == b' ') {
        return Err(unsupported());
    }
    let end = input
        .get(span.end..)
        .ok_or_else(unsupported)?
        .find('\n')
        .map_or(input.len(), |offset| span.end + offset + 1);
    Ok(start..end)
}
