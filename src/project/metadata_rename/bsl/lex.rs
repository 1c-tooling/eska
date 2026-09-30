//! Byte-preserving lexical boundaries for BSL reference analysis, independent of external analyzers.

use std::ops::Range;

/// A punctuation-heavy source must not turn the text byte limit into unbounded token allocation.
const MAX_TOKENS: usize = 1_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Word,
    Literal,
    Comment,
    Directive,
    Symbol(char),
}

#[derive(Debug)]
pub(super) struct Token<'a> {
    pub kind: Kind,
    pub text: &'a str,
    pub range: Range<usize>,
}

impl Token<'_> {
    /// Keyword comparison follows BSL's case-insensitive spelling in either language.
    pub fn word(&self, choices: &[&str]) -> bool {
        self.kind == Kind::Word && choices.iter().any(|word| super::same_name(self.text, word))
    }
}

/// Keep comments and literals for preview, but never expose their contents as executable tokens.
pub(super) fn tokenize(input: &str) -> Option<Vec<Token<'_>>> {
    let mut tokens = Vec::new();
    let mut offset = usize::from(input.starts_with('\u{feff}')) * 3;
    while let Some(character) = input[offset..].chars().next() {
        if character.is_whitespace() {
            offset += character.len_utf8();
            continue;
        }
        let start = offset;
        if tokens.len() >= MAX_TOKENS {
            return None;
        }
        let rest = &input[start..];
        let (kind, length) = if rest.starts_with("//") {
            (Kind::Comment, rest.find(['\r', '\n']).unwrap_or(rest.len()))
        } else if matches!(character, '#' | '&') {
            (
                Kind::Directive,
                rest.find(['\r', '\n']).unwrap_or(rest.len()),
            )
        } else if matches!(character, '"' | '\'') {
            (Kind::Literal, quoted(rest, character)?)
        } else if identifier_start(character) {
            (Kind::Word, word_end(rest))
        } else if character.is_ascii_digit() {
            // Consume invalid digit-prefixed names together instead of inventing a new identifier.
            (Kind::Literal, word_end(rest))
        } else if ".,;()[]+-*/%=<>?:~|".contains(character) {
            (Kind::Symbol(character), character.len_utf8())
        } else {
            return None;
        };
        offset += length;
        tokens.push(Token {
            kind,
            text: &input[start..offset],
            range: start..offset,
        });
    }
    Some(tokens)
}

/// Doubled quotes belong to one literal, including multiline BSL strings with continuation bars.
fn quoted(input: &str, quote: char) -> Option<usize> {
    let bytes = input.as_bytes();
    let delimiter = u8::try_from(quote).ok()?;
    let mut offset = 1;
    while offset < bytes.len() {
        if bytes[offset] == delimiter {
            offset += 1;
            if quote == '"' && bytes.get(offset) == Some(&delimiter) {
                offset += 1;
            } else {
                return Some(offset);
            }
        } else {
            offset += 1;
        }
    }
    None
}

/// Identifiers remain borrowed slices so Unicode offsets always refer to original UTF-8 bytes.
fn word_end(input: &str) -> usize {
    input
        .find(|character: char| !identifier_part(character))
        .unwrap_or(input.len())
}

/// Match the identifier alphabet conservatively; platform name validation is a separate boundary.
fn identifier_start(character: char) -> bool {
    character == '_' || character.is_alphabetic()
}

/// Non-ASCII digits stay within a lexical word even when a compiler would reject its spelling.
pub(super) fn identifier_part(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}
