//! Project a lossless property preview to one UTF-16 edit without guessing a semantic range.

use serde_json::{Value, json};

use super::{positions::Positions, protocol};

/// Common unchanged bytes remain outside the edit; CRLF pairs and Unicode scalars stay whole.
pub fn between(original: &str, updated: &str) -> Result<Vec<Value>, Value> {
    if original == updated {
        return Ok(Vec::new());
    }
    let mut start = original
        .chars()
        .zip(updated.chars())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum::<usize>();
    let suffix = original[start..]
        .chars()
        .rev()
        .zip(updated[start..].chars().rev())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum::<usize>();
    let mut end = original.len() - suffix;
    let mut updated_end = updated.len() - suffix;
    if splits_crlf(original, start) {
        start -= 1;
    }
    if splits_crlf(original, end) {
        end += 1;
        updated_end += 1;
    }
    let invalid = || protocol::error_value(-32603, "source_edit_range_invalid", Value::Null);
    let (first, last) = (
        u32::try_from(start).map_err(|_| invalid())?,
        u32::try_from(end).map_err(|_| invalid())?,
    );
    let positions = Positions::new(original, &[first, last]).map_err(|_| invalid())?;
    let range = positions.range(first, last).map_err(|_| invalid())?;
    let text = updated.get(start..updated_end).ok_or_else(invalid)?;
    Ok(vec![json!({"range":range,"newText":text})])
}

/// A UTF-16 line/column cannot address the byte boundary inside one CRLF sequence.
fn splits_crlf(text: &str, position: usize) -> bool {
    position > 0
        && text.as_bytes().get(position - 1) == Some(&b'\r')
        && text.as_bytes().get(position) == Some(&b'\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The returned standard edit recreates all bytes, including CRLF insertion/deletion and emoji.
    #[test]
    fn edits_preserve_scalar_and_line_boundaries_and_no_op_is_empty() {
        for (original, updated) in [
            ("# Я😀\r\nvalue: 'Old'\r\n", "# Я😀\r\nvalue: 'New'\r\n"),
            ("a\r\nb", "a\nb"),
            ("a\nb", "a\r\nb"),
            ("Я😀", "Я🚀"),
            ("a", "ab"),
            ("ab", "a"),
            ("", "a"),
            ("a", ""),
        ] {
            let values = between(original, updated).unwrap();
            let edit = &values[0];
            let start: super::super::positions::Position =
                serde_json::from_value(edit["range"]["start"].clone()).unwrap();
            let end: super::super::positions::Position =
                serde_json::from_value(edit["range"]["end"].clone()).unwrap();
            let mut actual = original.to_owned();
            actual.replace_range(
                start.offset(original).unwrap() as usize..end.offset(original).unwrap() as usize,
                edit["newText"].as_str().unwrap(),
            );
            assert_eq!(actual, updated);
        }
        assert!(between("# keep\r\n", "# keep\r\n").unwrap().is_empty());
    }
}
