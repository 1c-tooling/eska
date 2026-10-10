//! Capture selected UTF-16 coordinates from current UTF-8 bytes without retaining their text.

#[cfg(test)]
mod tests;

use std::ops::Range;

/// Zero-based coordinates count UTF-16 code units and preserve the input's BOM and line endings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct TextPosition {
    pub line: u32,
    pub character: u32,
}

/// Both endpoints address the same exact source input; the end is exclusive.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct TextRange {
    pub start: TextPosition,
    pub end: TextPosition,
}

/// Bad boundaries and missing captured endpoints are distinct from an empty source range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoordinateError {
    InvalidBoundary { offset: usize },
    NotCaptured { offset: usize },
    ReversedRange { start: usize, end: usize },
    Overflow,
}

/// Only requested boundaries survive parsing, in sorted order; there is no copy of source text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextCoordinates(Vec<(usize, TextPosition)>);

impl TextCoordinates {
    /// Validate boundaries first, then compute all requested coordinates in one source pass.
    ///
    /// # Errors
    /// Returns a split/out-of-bounds UTF-8 offset or coordinates outside the representable range.
    pub fn new(text: &str, offsets: &[usize]) -> Result<Self, CoordinateError> {
        if let Some(&offset) = offsets
            .iter()
            .find(|&&offset| !text.is_char_boundary(offset))
        {
            return Err(CoordinateError::InvalidBoundary { offset });
        }
        let mut offsets = offsets.to_vec();
        offsets.sort_unstable();
        offsets.dedup();
        let mut targets = offsets.into_iter().peekable();
        let mut positions = Vec::with_capacity(targets.len());
        let mut position = TextPosition::default();
        let mut chars = text.char_indices().peekable();
        while let Some((offset, character)) = chars.next() {
            if targets.peek().is_none() {
                break;
            }
            if targets.peek() == Some(&offset) {
                positions.push((offset, position));
                targets.next();
            }
            let crlf_start =
                character == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n');
            if character == '\n' || (character == '\r' && !crlf_start) {
                position.line = position
                    .line
                    .checked_add(1)
                    .ok_or(CoordinateError::Overflow)?;
                position.character = 0;
            } else if !crlf_start {
                position.character = position
                    .character
                    .checked_add(
                        u32::try_from(character.len_utf16())
                            .map_err(|_| CoordinateError::Overflow)?,
                    )
                    .ok_or(CoordinateError::Overflow)?;
            }
        }
        if let Some(offset) = targets.next() {
            // Every valid boundary not encountered above is the single end-of-input offset.
            positions.push((offset, position));
        }
        Ok(Self(positions))
    }

    /// Count retained endpoints without exposing implementation tables or evaluating queries.
    #[must_use]
    pub const fn stored_boundaries(&self) -> usize {
        self.0.len()
    }

    /// Return only captured endpoints; callers cannot invent coordinates for an unrecorded span.
    ///
    /// # Errors
    /// Returns a reversed range or an endpoint which was not supplied to `new`.
    pub fn range(&self, range: Range<usize>) -> Result<TextRange, CoordinateError> {
        if range.start > range.end {
            return Err(CoordinateError::ReversedRange {
                start: range.start,
                end: range.end,
            });
        }
        Ok(TextRange {
            start: self.position(range.start)?,
            end: self.position(range.end)?,
        })
    }

    /// Binary search addresses exactly the captured input, without reading a file or source buffer.
    fn position(&self, offset: usize) -> Result<TextPosition, CoordinateError> {
        self.0
            .binary_search_by_key(&offset, |&(boundary, _)| boundary)
            .map(|index| self.0[index].1)
            .map_err(|_| CoordinateError::NotCaptured { offset })
    }
}
