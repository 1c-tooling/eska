//! A bounded literal scanner handles quoting, not BSL semantics or query evaluation.

use std::ops::Range;

use super::{Query, QueryError, QueryIdentity, SourceComment, mapping::MappedText};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Word,
    String,
    Punctuation(u8),
    Other,
}

struct Token {
    kind: Kind,
    range: Range<usize>,
}

enum Piece {
    Text(Range<usize>),
    Newline(Range<usize>),
    Comment(Range<usize>),
}

struct Literal {
    range: Range<usize>,
    pieces: Vec<Piece>,
}

/// Decode the whole literal under a source cursor; constructed expressions remain unavailable.
///
/// # Errors
/// Rejects oversized snapshots, invalid cursors, incomplete literals and expressions requiring evaluation.
pub fn read_bsl(input: &str, offset: usize) -> Result<Query, QueryError> {
    super::check_size(input)?;
    if !input.is_char_boundary(offset) {
        return Err(QueryError::InvalidPosition);
    }
    let tokens = tokens(input)?;
    let selected = tokens
        .iter()
        .position(|token| token.kind == Kind::String && token.range.contains(&offset))
        .ok_or(QueryError::NotFound)?;
    if !whole_expression(input, &tokens, selected) {
        return Err(QueryError::UnsupportedExpression);
    }
    let literal = literal(input, tokens[selected].range.start)?;
    let mut mapped = MappedText::new(literal.range.start + 1);
    let mut comments = Vec::new();
    for piece in literal.pieces {
        match piece {
            Piece::Text(range) => decode_part(input, range, &mut mapped)?,
            Piece::Newline(range) => mapped.push('\n', range.clone(), &input[range] == "\n")?,
            Piece::Comment(range) => {
                if comments.len() == super::MAX_SOURCE_COMMENTS {
                    return Err(QueryError::TooLarge);
                }
                if range.contains(&offset) {
                    return Err(QueryError::NotFound);
                }
                comments.push(SourceComment {
                    text: input[range.clone()].to_owned(),
                    source: range,
                    query_offset: mapped.text.len(),
                });
            }
        }
    }
    let mapped = mapped.finish(literal.range.end - 1);
    Ok(Query {
        identity: QueryIdentity::BslLiteral(literal.range.clone()),
        text: mapped.text,
        source: literal.range,
        map: mapped.map,
        comments,
    })
}

/// Skip source trivia and dates so quotes in comments cannot masquerade as editable literals.
fn tokens(input: &str) -> Result<Vec<Token>, QueryError> {
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < input.len() {
        let rest = &input[offset..];
        let current = rest.chars().next().ok_or(QueryError::InvalidLiteral)?;
        if current.is_whitespace() || current == '\u{feff}' {
            offset += current.len_utf8();
            continue;
        }
        if rest.starts_with("//") {
            offset += rest.find(['\r', '\n']).unwrap_or(rest.len());
            continue;
        }
        let (kind, end) = match current {
            '"' => (Kind::String, literal(input, offset)?.range.end),
            '\'' => {
                let end = rest[1..]
                    .find(['\'', '\r', '\n'])
                    .ok_or(QueryError::InvalidLiteral)?
                    + 1;
                if rest.as_bytes()[end] != b'\'' {
                    return Err(QueryError::InvalidLiteral);
                }
                (Kind::Other, offset + end + 1)
            }
            '#' | '&' => (
                Kind::Other,
                offset + rest.find(['\r', '\n']).unwrap_or(rest.len()),
            ),
            '_' => (Kind::Word, word_end(input, offset)),
            character if character.is_alphabetic() => (Kind::Word, word_end(input, offset)),
            character if character.is_ascii_punctuation() => {
                (Kind::Punctuation(input.as_bytes()[offset]), offset + 1)
            }
            _ => (Kind::Other, offset + current.len_utf8()),
        };
        if result.len() == 131_072 {
            return Err(QueryError::TooLarge);
        }
        result.push(Token {
            kind,
            range: offset..end,
        });
        offset = end;
    }
    Ok(result)
}

/// Identifier spelling is used only for a small syntactic boundary check, never symbol resolution.
fn word_end(input: &str, offset: usize) -> usize {
    input[offset..]
        .char_indices()
        .find_map(|(index, character)| {
            (!(character == '_' || character.is_alphanumeric())).then_some(offset + index)
        })
        .unwrap_or(input.len())
}

/// Accept direct literal values and Query constructor arguments, independent of variable names.
fn whole_expression(input: &str, tokens: &[Token], selected: usize) -> bool {
    let mut before = selected;
    let mut after = selected + 1;
    while before > 0
        && tokens[before - 1].kind == Kind::Punctuation(b'(')
        && tokens
            .get(after)
            .is_some_and(|token| token.kind == Kind::Punctuation(b')'))
    {
        if before >= 3
            && word(input, &tokens[before - 2], &["запрос", "query"])
            && word(input, &tokens[before - 3], &["новый", "new"])
        {
            return true;
        }
        before -= 1;
        after += 1;
    }
    let Some(previous) = before.checked_sub(1).and_then(|index| tokens.get(index)) else {
        return false;
    };
    let direct =
        previous.kind == Kind::Punctuation(b'=') || word(input, previous, &["возврат", "return"]);
    direct
        && tokens
            .get(after)
            .is_none_or(|token| token.kind == Kind::Punctuation(b';'))
}

/// Bilingual keywords are compared as Unicode text, without locale-dependent lowercasing.
fn word(input: &str, token: &Token, names: &[&str]) -> bool {
    token.kind == Kind::Word && names.contains(&input[token.range.clone()].to_lowercase().as_str())
}

/// Record literal pieces and source-only comment lines while scanning just one quoted value.
fn literal(input: &str, start: usize) -> Result<Literal, QueryError> {
    let mut pieces = Vec::new();
    let mut offset = start + 1;
    let mut part = offset;
    while offset < input.len() {
        let rest = &input[offset..];
        match rest.as_bytes()[0] {
            b'"' if rest.starts_with("\"\"") => offset += 2,
            b'"' => {
                pieces.push(Piece::Text(part..offset));
                return Ok(Literal {
                    range: start..offset + 1,
                    pieces,
                });
            }
            b'\r' | b'\n' => {
                pieces.push(Piece::Text(part..offset));
                let end = offset + if rest.starts_with("\r\n") { 2 } else { 1 };
                pieces.push(Piece::Newline(offset..end));
                offset = continuation(input, end, &mut pieces)?;
                part = offset;
            }
            _ => {
                offset += rest
                    .chars()
                    .next()
                    .ok_or(QueryError::InvalidLiteral)?
                    .len_utf8();
            }
        }
        if pieces.len() > 65_536 {
            return Err(QueryError::TooLarge);
        }
    }
    Err(QueryError::InvalidLiteral)
}

/// A missing continuation marker is unsupported rather than guessed from indentation.
fn continuation(
    input: &str,
    mut offset: usize,
    pieces: &mut Vec<Piece>,
) -> Result<usize, QueryError> {
    while offset < input.len() {
        let start = offset;
        let rest = &input[offset..];
        offset += rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let rest = &input[offset..];
        if rest.starts_with('|') {
            return Ok(offset + 1);
        }
        if rest.starts_with("//") {
            let end = offset + rest.find(['\r', '\n']).unwrap_or(rest.len());
            pieces.push(Piece::Comment(start..end));
            offset = end;
        }
        let rest = &input[offset..];
        if rest.starts_with("\r\n") {
            offset += 2;
        } else if rest.starts_with(['\r', '\n']) {
            offset += 1;
        } else {
            return Err(QueryError::InvalidLiteral);
        }
        if pieces.len() > 65_536 {
            return Err(QueryError::TooLarge);
        }
    }
    Err(QueryError::InvalidLiteral)
}

/// Double quotes decode only inside a verified piece, not across comment or line boundaries.
fn decode_part(
    input: &str,
    range: Range<usize>,
    mapped: &mut MappedText,
) -> Result<(), QueryError> {
    let mut offset = range.start;
    while offset < range.end {
        let current = input[offset..range.end]
            .chars()
            .next()
            .ok_or(QueryError::InvalidLiteral)?;
        let length = if current == '"' {
            2
        } else {
            current.len_utf8()
        };
        if current == '"' && !input[offset..range.end].starts_with("\"\"") {
            return Err(QueryError::InvalidLiteral);
        }
        mapped.push(current, offset..offset + length, current != '"')?;
        offset += length;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
