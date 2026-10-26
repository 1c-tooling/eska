//! Lossless token ranges keep literal values, comments and physical line boundaries available.

use crate::spelling::{self, Key};
use eska_project_core::formatting::{Error, Language};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Word,
    Number,
    Literal,
    Comment,
    Directive,
    Annotation,
    Symbol,
}

#[derive(Clone, Debug)]
pub struct Token<'a> {
    pub text: &'a str,
    pub range: Range<usize>,
    pub kind: Kind,
    pub key: Option<Key>,
}

impl Token<'_> {
    /// Explicit punctuation predicates prevent spelling changes inside opaque source tokens.
    pub fn is(&self, value: &str) -> bool {
        self.text == value
    }
}

/// Tokenize executable syntax while preserving whole strings and comment bodies exactly.
pub fn tokens(input: &str, language: Language) -> Result<Vec<Token<'_>>, Error> {
    let mut output: Vec<Token<'_>> = Vec::new();
    let mut offset = if input.starts_with('\u{feff}') { 3 } else { 0 };
    while let Some(character) = input[offset..].chars().next() {
        if character.is_whitespace() {
            offset += character.len_utf8();
            continue;
        }
        if output.len() >= 262_144 {
            return Err(Error::new("syntax_too_large", "tokens"));
        }
        let start = offset;
        let rest = &input[start..];
        let (kind, length) = scan(rest, character, language).map_err(|error| error.at(start))?;
        offset += length;
        let text = &input[start..offset];
        let key = (language == Language::Bsl
            && kind == Kind::Word
            && !output.last().is_some_and(|token| token.is(".")))
        .then(|| spelling::key(text))
        .flatten();
        output.push(Token {
            text,
            range: start..offset,
            kind,
            key,
        });
    }
    Ok(output)
}

/// Quotes are scanned before punctuation, so escapes and multiline bodies stay indivisible.
fn scan(input: &str, first: char, language: Language) -> Result<(Kind, usize), Error> {
    if input.starts_with("//") {
        return Ok((
            Kind::Comment,
            input.find(['\r', '\n']).unwrap_or(input.len()),
        ));
    }
    if input.starts_with("/*") && language == Language::Sdbl {
        return Ok((
            Kind::Comment,
            input
                .find("*/")
                .ok_or_else(|| Error::new("invalid_syntax", "comment"))?
                + 2,
        ));
    }
    if matches!(first, '"' | '\'') {
        return Ok((Kind::Literal, quoted(input, first, language)?));
    }
    if first.is_alphabetic() || first == '_' {
        return Ok((Kind::Word, word_end(input)));
    }
    if first.is_ascii_digit() {
        return Ok((Kind::Number, number_end(input)?));
    }
    if first == '#' || first == '&' {
        let length = word_end(&input[1..]);
        if length == 0 {
            return Err(Error::new("unsupported_syntax", first.to_string()));
        }
        return Ok((
            if first == '#' {
                Kind::Directive
            } else {
                Kind::Annotation
            },
            length + 1,
        ));
    }
    if "()[],;.+-*/%=<>?:~{}".contains(first) {
        let length = if ["<=", ">=", "<>"]
            .iter()
            .any(|value| input.starts_with(value))
        {
            2
        } else {
            first.len_utf8()
        };
        return Ok((Kind::Symbol, length));
    }
    Err(Error::new("unsupported_syntax", first.to_string()))
}

/// BSL continuation bars and interleaved source comments remain part of the original literal.
fn quoted(input: &str, quote: char, language: Language) -> Result<usize, Error> {
    let bytes = input.as_bytes();
    let delimiter = if quote == '"' { b'"' } else { b'\'' };
    let mut index = 1;
    while index < bytes.len() {
        if bytes[index] == delimiter {
            index += 1;
            if quote == '"' && bytes.get(index) == Some(&delimiter) {
                index += 1;
                continue;
            }
            if quote == '\''
                && (!matches!(index - 2, 8 | 12 | 14)
                    || !input[1..index - 1]
                        .bytes()
                        .all(|byte| byte.is_ascii_digit())
                    || language != Language::Bsl)
            {
                return Err(Error::new("invalid_syntax", "date"));
            }
            return Ok(index);
        }
        if bytes[index] == b'\n' && language == Language::Bsl {
            let tail = input[index + 1..].trim_start_matches([' ', '\t', '\r']);
            if quote == '\'' || !(tail.starts_with('|') || tail.starts_with("//")) {
                return Err(Error::new("invalid_syntax", "string_continuation"));
            }
            if tail.starts_with("//") {
                index = input[index + 1..]
                    .find('\n')
                    .map_or(bytes.len(), |end| index + 1 + end);
                continue;
            }
        }
        index += 1;
    }
    Err(Error::new("invalid_syntax", "literal"))
}

/// Numeric lexemes include the fractional part, keeping decimal punctuation out of spacing rules.
fn number_end(input: &str) -> Result<usize, Error> {
    let mut length = input.bytes().take_while(u8::is_ascii_digit).count();
    if input.as_bytes().get(length) == Some(&b'.')
        && input
            .as_bytes()
            .get(length + 1)
            .is_some_and(u8::is_ascii_digit)
    {
        length += 1 + input[length + 1..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
    }
    if input[length..]
        .chars()
        .next()
        .is_some_and(|value| value.is_alphabetic() || value == '_')
    {
        return Err(Error::new("invalid_syntax", "number"));
    }
    Ok(length)
}

/// Identifier ranges borrow Unicode slices and do not allocate normalized copies.
fn word_end(input: &str) -> usize {
    input
        .find(|value: char| !(value.is_alphanumeric() || value == '_'))
        .unwrap_or(input.len())
}

/// Significant tokens, identifiers, literals and comment text must survive every rewrite.
pub fn equivalent(before: &[Token<'_>], after: &[Token<'_>]) -> bool {
    before.len() == after.len()
        && before.iter().zip(after).all(|(left, right)| {
            left.kind == right.kind
                && left.key.map_or_else(
                    || {
                        if left.kind == Kind::Directive {
                            spelling::directive(left.text) == spelling::directive(right.text)
                        } else {
                            left.text == right.text
                        }
                    },
                    |key| right.key == Some(key),
                )
        })
}
