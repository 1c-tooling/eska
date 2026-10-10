//! Map all requested byte boundaries in one pass; diagnostic volume must not rescan whole files.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Value, json};

use super::protocol;

#[derive(Clone, Copy, Default, Serialize)]
struct Position {
    line: u32,
    character: u32,
}

pub struct Positions(BTreeMap<u32, Position>);

impl Positions {
    /// Only valid UTF-8 boundaries of the same text may become zero-based UTF-16 positions.
    pub fn new(text: &str, offsets: &[u32]) -> Result<Self, Value> {
        if offsets
            .iter()
            .any(|offset| !text.is_char_boundary(*offset as usize))
        {
            return Err(invalid());
        }
        let mut targets = offsets
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .peekable();
        let mut positions = BTreeMap::new();
        let mut position = Position::default();
        let mut chars = text.char_indices().peekable();
        while let Some((offset, character)) = chars.next() {
            if targets.peek().is_none() {
                break;
            }
            let offset = u32::try_from(offset).map_err(|_| invalid())?;
            if targets.peek() == Some(&offset) {
                positions.insert(offset, position);
                targets.next();
            }
            if character == '\n' {
                position.line += 1;
                position.character = 0;
            } else if character != '\r' || chars.peek().is_none_or(|(_, next)| *next != '\n') {
                position.character +=
                    u32::try_from(character.len_utf16()).map_err(|_| invalid())?;
            }
        }
        if let Some(offset) = targets.next() {
            positions.insert(offset, position);
        }
        Ok(Self(positions))
    }

    /// A range is end-exclusive; both endpoints must have been validated against the current bytes.
    pub fn range(&self, start: u32, end: u32) -> Result<Value, Value> {
        let start = self.0.get(&start).ok_or_else(invalid)?;
        let end = self.0.get(&end).ok_or_else(invalid)?;
        Ok(json!({"start":start,"end":end}))
    }
}

/// Bad kernel ranges never produce plausible coordinates or invoke a panicking converter.
fn invalid() -> Value {
    protocol::error_value(-32603, "diagnostic_range_invalid", Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unicode scalar, UTF-8 byte and UTF-16 positions differ, including BOM and CRLF boundaries.
    #[test]
    fn exact_unicode_crlf_and_eof_positions_are_end_exclusive() {
        let text = "\u{feff}Я😀\r\nZ\r\n";
        let positions = Positions::new(text, &[0, 3, 5, 9, 10, 11, 12, 14, 5]).unwrap();
        assert_eq!(
            positions.range(5, 9).unwrap(),
            json!({"start":{"line":0,"character":2},"end":{"line":0,"character":4}})
        );
        assert_eq!(
            positions.range(9, 10).unwrap(),
            json!({"start":{"line":0,"character":4},"end":{"line":0,"character":4}})
        );
        assert_eq!(
            positions.range(11, 14).unwrap(),
            json!({"start":{"line":1,"character":0},"end":{"line":2,"character":0}})
        );
        assert_eq!(
            Positions::new("", &[0]).unwrap().range(0, 0).unwrap(),
            json!({"start":{"line":0,"character":0},"end":{"line":0,"character":0}})
        );
    }

    /// Mid-codepoint, oversized and unrecorded endpoints cannot be serialized as valid ranges.
    #[test]
    fn invalid_boundaries_do_not_floor_or_clamp_to_old_text() {
        for offset in [1, 6, 15] {
            assert!(Positions::new("\u{feff}Я😀\r\nZ\r\n", &[offset]).is_err());
        }
        assert!(Positions::new("x", &[0]).unwrap().range(0, 1).is_err());
    }
}
