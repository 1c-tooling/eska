//! A bounded lossless lexer records ranges only; it does not implement another semantic analyzer.

use std::ops::Range;

use super::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Word,
    Number,
    Quoted,
    Comment,
    Symbol(char),
}

#[derive(Clone, Debug)]
pub(super) struct Token {
    pub kind: Kind,
    pub range: Range<usize>,
    pub depth: u16,
}

impl Token {
    /// Keywords are compared only at their exact lexical boundaries, never inside strings or comments.
    pub fn keyword(&self, input: &str, alternatives: &[&str]) -> bool {
        self.kind == Kind::Word
            && alternatives.contains(&input[self.range.clone()].to_uppercase().as_str())
    }
}

/// Keep all unknown symbols visible to the model and reject malformed nesting before planning any edit.
pub(super) fn tokens(input: &str) -> Result<Vec<Token>, Error> {
    let mut result = Vec::new();
    let mut offset = 0;
    let mut nesting = Vec::new();
    while offset < input.len() {
        let character = input[offset..].chars().next().ok_or(Error::InvalidSyntax)?;
        if character.is_whitespace() || character == '\u{feff}' {
            offset += character.len_utf8();
            continue;
        }
        if result.len() == 65_536 {
            return Err(Error::TooLarge);
        }
        let start = offset;
        let kind = token(input, &mut offset, character)?;
        if let Kind::Symbol(')' | ']' | '}') = kind {
            let expected = match character {
                ')' => '(',
                ']' => '[',
                _ => '{',
            };
            if nesting.pop() != Some(expected) {
                return Err(Error::InvalidSyntax);
            }
        }
        let depth = u16::try_from(nesting.len()).map_err(|_| Error::TooLarge)?;
        result.push(Token {
            kind,
            range: start..offset,
            depth,
        });
        if let Kind::Symbol('(' | '[' | '{') = kind {
            if nesting.len() == 256 {
                return Err(Error::TooLarge);
            }
            nesting.push(character);
        }
    }
    if nesting.is_empty() {
        Ok(result)
    } else {
        Err(Error::InvalidSyntax)
    }
}

/// Whitespace stays in the original text; comments and quoted strings retain their complete byte ranges.
fn token(input: &str, offset: &mut usize, character: char) -> Result<Kind, Error> {
    let rest = &input[*offset..];
    if rest.starts_with("//") {
        *offset += rest.find(['\r', '\n']).unwrap_or(rest.len());
        return Ok(Kind::Comment);
    }
    if rest.starts_with("/*") {
        *offset += rest.find("*/").ok_or(Error::InvalidSyntax)? + 2;
        return Ok(Kind::Comment);
    }
    if character == '"' {
        *offset += 1;
        while *offset < input.len() {
            let tail = &input[*offset..];
            if tail.starts_with("\"\"") {
                *offset += 2;
            } else if tail.starts_with('"') {
                *offset += 1;
                return Ok(Kind::Quoted);
            } else {
                *offset += tail.chars().next().ok_or(Error::InvalidSyntax)?.len_utf8();
            }
        }
        return Err(Error::InvalidSyntax);
    }
    if character.is_alphabetic() || character == '_' {
        *offset += rest
            .char_indices()
            .take_while(|(_, value)| value.is_alphanumeric() || *value == '_')
            .last()
            .map_or(0, |(at, value)| at + value.len_utf8());
        return Ok(Kind::Word);
    }
    if character.is_ascii_digit() {
        *offset += rest.bytes().take_while(u8::is_ascii_digit).count();
        return Ok(Kind::Number);
    }
    *offset += character.len_utf8();
    Ok(Kind::Symbol(character))
}
