//! Bounded expression validation covers syntax without inferring metadata, types or values.

use crate::{
    lex::{Kind, Token},
    spelling::Key,
};
use eska_project_core::formatting::Error;

/// Comments do not participate in expression grammar, but remain in the lossless source model.
pub fn significant<'a>(tokens: &'a [Token<'a>]) -> Vec<&'a Token<'a>> {
    tokens
        .iter()
        .filter(|token| token.kind != Kind::Comment)
        .collect()
}

/// A complete expression must consume its entire slice, including any nested calls or indices.
pub fn expression(tokens: &[&Token<'_>]) -> Result<(), Error> {
    let mut parser = Parser {
        tokens,
        at: 0,
        depth: 0,
    };
    parser.expression(0)?;
    if parser.at == tokens.len() {
        Ok(())
    } else {
        Err(invalid())
    }
}

/// Parameter declarations accept explicit value semantics and optional constant defaults.
pub fn parameters(tokens: &[&Token<'_>]) -> Result<(), Error> {
    let mut parser = Parser {
        tokens,
        at: 0,
        depth: 0,
    };
    while parser.at < tokens.len() {
        if parser
            .current()
            .is_some_and(|token| token.key == Some(Key::Val))
        {
            parser.at += 1;
        }
        parser.name()?;
        if parser.symbol("=") {
            let signed = parser.symbol("+") || parser.symbol("-");
            let value = parser.current().ok_or_else(invalid)?;
            if !(matches!(value.kind, Kind::Literal | Kind::Number)
                || !signed
                    && matches!(
                        value.key,
                        Some(Key::True | Key::False | Key::Null | Key::Undefined)
                    ))
            {
                return Err(invalid());
            }
            parser.at += 1;
        }
        if parser.at == tokens.len() {
            return Ok(());
        }
        if !parser.symbol(",") || parser.at == tokens.len() {
            return Err(invalid());
        }
    }
    Ok(())
}

struct Parser<'a, 'b> {
    tokens: &'a [&'b Token<'b>],
    at: usize,
    depth: u16,
}

impl Parser<'_, '_> {
    /// Precedence parsing rejects adjacent values and incomplete operators without evaluation.
    fn expression(&mut self, minimum: u8) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(Error::new("syntax_too_large", "expression_depth"));
        }
        self.primary()?;
        while let Some(priority) = self.current().and_then(|token| precedence(token)) {
            if priority < minimum {
                break;
            }
            self.at += 1;
            self.expression(priority + 1)?;
        }
        self.depth -= 1;
        Ok(())
    }

    /// Prefix and postfix grammar never rewrites literals or looks up a member name.
    fn primary(&mut self) -> Result<(), Error> {
        let token = self.current().ok_or_else(invalid)?;
        if matches!(token.key, Some(Key::Not | Key::Await)) || token.is("+") || token.is("-") {
            self.at += 1;
            return self.expression(60);
        }
        if token.key == Some(Key::New) {
            self.at += 1;
            if self.symbol("(") {
                self.arguments()?;
            } else {
                self.name()?;
            }
        } else if token.is("(") {
            self.at += 1;
            self.expression(0)?;
            if !self.symbol(")") {
                return Err(invalid());
            }
        } else if token.is("?") {
            self.at += 1;
            if !self.symbol("(") {
                return Err(invalid());
            }
            self.arguments()?;
        } else if matches!(token.kind, Kind::Number | Kind::Literal)
            || token.kind == Kind::Word
                && (token.key.is_none()
                    || matches!(
                        token.key,
                        Some(Key::True | Key::False | Key::Undefined | Key::Null)
                    ))
        {
            self.at += 1;
        } else {
            return Err(invalid());
        }
        loop {
            if self.symbol(".") {
                self.name()?;
            } else if self.symbol("[") {
                self.expression(0)?;
                if !self.symbol("]") {
                    return Err(invalid());
                }
            } else if self.symbol("(") {
                self.arguments()?;
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Empty argument positions stay distinct from missing delimiters and expression failures.
    fn arguments(&mut self) -> Result<(), Error> {
        if self.symbol(")") {
            return Ok(());
        }
        loop {
            if !self
                .current()
                .is_some_and(|token| token.is(",") || token.is(")"))
            {
                self.expression(0)?;
            }
            if self.symbol(")") {
                return Ok(());
            }
            if !self.symbol(",") {
                return Err(invalid());
            }
        }
    }

    /// Declared names cannot silently absorb a reserved keyword or punctuation token.
    fn name(&mut self) -> Result<(), Error> {
        if !self
            .current()
            .is_some_and(|token| token.kind == Kind::Word && token.key.is_none())
        {
            return Err(invalid());
        }
        self.at += 1;
        Ok(())
    }

    /// Cursor movement is conditional and leaves the first nonmatching token untouched.
    fn symbol(&mut self, symbol: &str) -> bool {
        if self.current().is_some_and(|token| token.is(symbol)) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    /// End-of-input is represented explicitly so malformed input cannot index past a token slice.
    fn current(&self) -> Option<&Token<'_>> {
        self.tokens.get(self.at).copied()
    }
}

/// Numeric precedences are only syntax boundaries; emitted token order never depends on them.
pub(super) fn precedence(token: &Token<'_>) -> Option<u8> {
    match token.key {
        Some(Key::Or) => Some(10),
        Some(Key::And) => Some(20),
        _ => match token.text {
            "=" | "<>" | "<" | ">" | "<=" | ">=" => Some(30),
            "+" | "-" => Some(40),
            "*" | "/" | "%" => Some(50),
            _ => None,
        },
    }
}

/// Grammar errors carry no localized parser prose into the domain API.
fn invalid() -> Error {
    Error::new("invalid_syntax", "expression")
}
