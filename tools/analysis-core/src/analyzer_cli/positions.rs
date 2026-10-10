//! Map all requested byte boundaries in one pass; diagnostic volume must not rescan whole files.

use eska::project_core::source_format::TextCoordinates;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::protocol;

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    line: u32,
    character: u32,
}

impl Position {
    /// LSP columns beyond a line clamp to its end; a split surrogate or absent line is invalid.
    pub fn offset(self, text: &str) -> Result<u32, Value> {
        let invalid = || protocol::error_value(-32602, "position_invalid", Value::Null);
        if self.line > i32::MAX as u32 || self.character > i32::MAX as u32 {
            return Err(invalid());
        }
        let mut start = 0;
        let mut rest = text;
        for _ in 0..self.line {
            let end = rest.find(['\r', '\n']).ok_or_else(invalid)?;
            let next = end
                + if rest[end..].starts_with("\r\n") {
                    2
                } else {
                    1
                };
            start += next;
            rest = &rest[next..];
        }
        let end = rest.find(['\r', '\n']).unwrap_or(rest.len());
        let mut column = 0;
        for (offset, character) in rest[..end].char_indices() {
            if column == self.character {
                return u32::try_from(start + offset).map_err(|_| invalid());
            }
            column += u32::try_from(character.len_utf16()).map_err(|_| invalid())?;
            if column > self.character {
                return Err(invalid());
            }
        }
        u32::try_from(start + end).map_err(|_| invalid())
    }
}

pub struct Positions(TextCoordinates);

impl Positions {
    /// Only valid UTF-8 boundaries of the same text may become zero-based UTF-16 positions.
    pub fn new(text: &str, offsets: &[u32]) -> Result<Self, Value> {
        let offsets = offsets
            .iter()
            .map(|offset| *offset as usize)
            .collect::<Vec<_>>();
        TextCoordinates::new(text, &offsets)
            .map(Self)
            .map_err(|_| invalid())
    }

    /// A range is end-exclusive; both endpoints must have been validated against the current bytes.
    pub fn range(&self, start: u32, end: u32) -> Result<Value, Value> {
        let range = self
            .0
            .range(start as usize..end as usize)
            .map_err(|_| invalid())?;
        Ok(json!(range))
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

    /// One converter preserves BOM/non-BMP boundaries across CRLF, LF, CR and the final empty line.
    #[test]
    fn utf16_cursor_uses_current_bytes_and_clamps_only_past_line_end() {
        let text = "\u{feff}Я😀\r\nZ\rR\n";
        for (line, character, offset) in [
            (0, 0, 0),
            (0, 1, 3),
            (0, 2, 5),
            (0, 4, 9),
            (0, 99, 9),
            (1, 1, 12),
            (2, 0, 13),
            (3, 0, 15),
        ] {
            assert_eq!(Position { line, character }.offset(text).unwrap(), offset);
            let actual = Positions::new(text, &[offset])
                .unwrap()
                .range(offset, offset)
                .unwrap();
            assert_eq!(actual["start"]["line"], line);
            if character != 99 {
                assert_eq!(actual["start"]["character"], character);
            }
        }
        for (line, character) in [(0, 3), (4, 0), (0, u32::MAX), (u32::MAX, 0)] {
            assert!(Position { line, character }.offset(text).is_err());
        }
        assert_eq!(Position::default().offset("").unwrap(), 0);
    }
}
