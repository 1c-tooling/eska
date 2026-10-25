//! Compact monotonic maps distinguish literal bytes from escapes and omitted source syntax.

use std::ops::Range;

/// A boundary can refer to either side of omitted markup or continuation indentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Affinity {
    Before,
    After,
}

/// UTF-16 coordinates follow editor/LSP conventions and reject split surrogate pairs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

/// Encoded spans have only two valid boundaries; direct spans preserve byte offsets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mapping {
    pub query: Range<usize>,
    pub source: Range<usize>,
    pub direct: bool,
}

/// Coalesce ordinary text rather than retaining one allocation per source character.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMap {
    spans: Vec<Mapping>,
    empty_anchor: usize,
    end_anchor: usize,
}

impl SourceMap {
    /// Expose the checked mapping for transport without guessing positions in source gaps.
    #[must_use]
    pub fn spans(&self) -> &[Mapping] {
        &self.spans
    }

    /// Translate a decoded boundary, choosing a side when source-only syntax intervenes.
    #[must_use]
    pub fn query_to_source(&self, query: &str, offset: usize, affinity: Affinity) -> Option<usize> {
        if !query.is_char_boundary(offset) {
            return None;
        }
        if query.is_empty() && offset == 0 {
            return Some(self.empty_anchor);
        }
        if offset == query.len() && affinity == Affinity::After {
            return Some(self.end_anchor);
        }
        let start = self.spans.partition_point(|span| span.query.end < offset);
        let mut candidates = self.spans[start..]
            .iter()
            .take_while(|span| span.query.start <= offset)
            .filter_map(|span| translate(&span.query, &span.source, span.direct, offset));
        match affinity {
            Affinity::Before => candidates.next(),
            Affinity::After => candidates.last(),
        }
    }

    /// Source-only quotes, comments, indentation and entity interiors have no SDBL position.
    #[must_use]
    pub fn source_to_query(&self, source: &str, offset: usize) -> Option<usize> {
        if !source.is_char_boundary(offset) {
            return None;
        }
        if self.spans.is_empty() && offset == self.empty_anchor {
            return Some(0);
        }
        if offset == self.end_anchor {
            return Some(self.spans.last().map_or(0, |span| span.query.end));
        }
        let start = self.spans.partition_point(|span| span.source.end < offset);
        self.spans[start..]
            .iter()
            .take_while(|span| span.source.start <= offset)
            .find_map(|span| translate(&span.source, &span.query, span.direct, offset))
    }
}

/// A scalar boundary inside an escape is deliberately not rounded to a nearby character.
const fn translate(
    from: &Range<usize>,
    to: &Range<usize>,
    direct: bool,
    offset: usize,
) -> Option<usize> {
    if offset < from.start || offset > from.end {
        None
    } else if direct {
        Some(to.start + offset - from.start)
    } else if offset == from.start {
        Some(to.start)
    } else if offset == from.end {
        Some(to.end)
    } else {
        None
    }
}

/// Count CRLF as one line break while keeping the buffer's original byte offsets.
#[must_use]
pub fn byte_position(input: &str, offset: usize) -> Option<Position> {
    if !input.is_char_boundary(offset) {
        return None;
    }
    let mut line = 0;
    let mut character = 0;
    let mut chars = input.char_indices().peekable();
    while let Some((index, current)) = chars.next() {
        if index == offset {
            return Some(Position { line, character });
        }
        match current {
            '\r' => {
                if chars.peek().is_some_and(|(_, next)| *next == '\n') {
                    let (lf, _) = chars.next()?;
                    if offset == lf {
                        return None;
                    }
                }
                line += 1;
                character = 0;
            }
            '\n' => {
                line += 1;
                character = 0;
            }
            _ => character += current.len_utf16(),
        }
    }
    (offset == input.len()).then_some(Position { line, character })
}

/// Convert an editor position without accepting out-of-line columns or half a surrogate pair.
#[must_use]
pub fn position_byte(input: &str, position: Position) -> Option<usize> {
    let mut line = 0;
    let mut character = 0;
    let mut chars = input.char_indices().peekable();
    while let Some((index, current)) = chars.next() {
        if position == (Position { line, character }) {
            return Some(index);
        }
        match current {
            '\r' | '\n' => {
                if line == position.line {
                    return None;
                }
                if current == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
                    chars.next();
                }
                line += 1;
                character = 0;
            }
            _ => character += current.len_utf16(),
        }
    }
    (position == Position { line, character }).then_some(input.len())
}

/// Build text and its source map together so escape decoding cannot drift from positions.
pub(super) struct MappedText {
    pub text: String,
    pub map: SourceMap,
}

impl MappedText {
    /// An empty value still has a valid insertion position inside its source delimiters.
    pub const fn new(anchor: usize) -> Self {
        Self {
            text: String::new(),
            map: SourceMap {
                spans: Vec::new(),
                empty_anchor: anchor,
                end_anchor: anchor,
            },
        }
    }

    /// Empty final lines still map after their continuation prefix or CDATA delimiters.
    pub const fn finish(mut self, anchor: usize) -> Self {
        self.map.end_anchor = anchor;
        if self.text.is_empty() {
            self.map.empty_anchor = anchor;
        }
        self
    }

    /// Merge only contiguous byte-identical runs; omitted markup remains a real gap.
    pub fn push(
        &mut self,
        character: char,
        source: Range<usize>,
        direct: bool,
    ) -> Result<(), super::QueryError> {
        if self.map.spans.len() == 65_536
            && !(direct
                && self
                    .map
                    .spans
                    .last()
                    .is_some_and(|last| last.direct && last.source.end == source.start))
        {
            return Err(super::QueryError::TooLarge);
        }
        let start = self.text.len();
        self.text.push(character);
        self.map.end_anchor = source.end;
        if direct
            && let Some(last) = self.map.spans.last_mut()
            && last.direct
            && last.source.end == source.start
        {
            last.source.end = source.end;
            last.query.end = self.text.len();
            return Ok(());
        }
        self.map.spans.push(Mapping {
            query: start..self.text.len(),
            source,
            direct,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_round_trips_boundaries_with_bom_crlf_cyrillic_and_non_bmp() {
        let input = "\u{feff}А😀\r\n\nБ\rВ";
        for (byte, _) in input
            .char_indices()
            .chain(std::iter::once((input.len(), '\0')))
        {
            if let Some(position) = byte_position(input, byte) {
                assert_eq!(position_byte(input, position), Some(byte));
            }
        }
        assert_eq!(byte_position(input, "\u{feff}А😀\r".len()), None);
        assert_eq!(
            position_byte(
                input,
                Position {
                    line: 0,
                    character: 3
                }
            ),
            None
        );
        assert_eq!(
            position_byte(
                input,
                Position {
                    line: 0,
                    character: 5
                }
            ),
            None
        );
        assert_eq!(
            position_byte(
                input,
                Position {
                    line: 8,
                    character: 0
                }
            ),
            None
        );
    }

    #[test]
    fn escapes_and_gaps_are_not_guessed_and_ordinary_runs_are_compact() {
        let source = "ab&amp;<!--keep-->😀";
        let mut value = MappedText::new(0);
        value.push('a', 0..1, true).expect("a");
        value.push('b', 1..2, true).expect("b");
        value.push('&', 2..7, false).expect("entity");
        value.push('😀', 18..22, true).expect("non-BMP");
        assert_eq!(value.map.spans().len(), 3);
        assert_eq!(
            value.map.query_to_source(&value.text, 3, Affinity::Before),
            Some(7)
        );
        assert_eq!(
            value.map.query_to_source(&value.text, 3, Affinity::After),
            Some(18)
        );
        assert_eq!(value.map.source_to_query(source, 4), None);
        assert_eq!(value.map.source_to_query(source, 10), None);
        assert_eq!(
            value.map.query_to_source(&value.text, 4, Affinity::After),
            None
        );
    }
}
