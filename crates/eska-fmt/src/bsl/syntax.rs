//! Logical units and block states preserve conditional-compilation branches without an index.

use super::expressions;
use crate::{
    lex::{Kind, Token},
    spelling::Key,
};
use eska_project_core::formatting::Error;
use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitKind {
    Statement,
    Header,
    End,
    Method,
    MethodEnd,
    Directive,
    Annotation,
    Comment,
    Label,
    Deleted,
}

#[derive(Clone, Debug)]
pub struct Unit {
    pub tokens: Range<usize>,
    pub level: usize,
    pub kind: UnitKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Block {
    If(bool),
    Loop,
    Procedure,
    Function,
    Try(bool),
}

struct Conditional {
    initial: Vec<Block>,
    branches: Vec<Vec<Block>>,
    has_else: bool,
}

#[derive(Default)]
struct State {
    blocks: Vec<Block>,
    conditions: Vec<Conditional>,
    regions: usize,
    deleting: bool,
    inserting: bool,
}

/// Split only at grammar boundaries, retaining comments within calls and headers.
pub fn parse(input: &str, tokens: &[Token<'_>]) -> Result<Vec<Unit>, Error> {
    let mut units = Vec::new();
    let mut at = 0;
    let mut state = State::default();
    while at < tokens.len() {
        let first = at;
        let (end, kind) = if state.deleting
            && !matches!(directive(&tokens[at]), "enddelete" | "конецудаления")
        {
            let end = tokens[at..]
                .iter()
                .position(|token| {
                    token.kind == Kind::Directive
                        && matches!(directive(token), "enddelete" | "конецудаления")
                })
                .map(|offset| at + offset)
                .ok_or_else(|| invalid("unclosed_deletion"))?;
            (end, UnitKind::Deleted)
        } else {
            boundary(input, tokens, at).map_err(|error| error.at(tokens[at].range.start))?
        };
        at = end;
        if tokens
            .get(at)
            .is_some_and(|token| token.kind == Kind::Comment)
            && !input[tokens[at - 1].range.end..tokens[at].range.start].contains('\n')
        {
            at += 1;
        }
        let level = state
            .accept(&tokens[first..at], kind)
            .map_err(|error| error.at(tokens[first].range.start))?;
        units.push(Unit {
            tokens: first..at,
            level,
            kind,
        });
    }
    if !state.blocks.is_empty()
        || !state.conditions.is_empty()
        || state.regions != 0
        || state.deleting
        || state.inserting
    {
        return Err(invalid("unclosed_block"));
    }
    Ok(units)
}

/// Header and terminal keywords are recognized only at the beginning of a logical unit.
fn boundary(input: &str, tokens: &[Token<'_>], at: usize) -> Result<(usize, UnitKind), Error> {
    let first = &tokens[at];
    if first.kind == Kind::Comment {
        return Ok((at + 1, UnitKind::Comment));
    }
    if matches!(first.kind, Kind::Directive | Kind::Annotation) {
        if !input[..first.range.start]
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .trim_matches(['\r', ' ', '\t', '\u{feff}'])
            .is_empty()
        {
            return Err(invalid("directive_position"));
        }
        let header = first.kind == Kind::Directive
            && matches!(directive(first), "if" | "если" | "elsif" | "иначеесли");
        return Ok((
            if header {
                until(tokens, at, Some(Key::Then))?
            } else {
                line_end(input, tokens, at)?
            },
            if first.kind == Kind::Directive {
                UnitKind::Directive
            } else {
                UnitKind::Annotation
            },
        ));
    }
    let key = if first.key == Some(Key::Async) {
        tokens.get(at + 1).and_then(|token| token.key)
    } else {
        first.key
    };
    if matches!(key, Some(Key::Procedure | Key::Function)) {
        let offset = at + usize::from(first.key == Some(Key::Async));
        if tokens
            .get(offset + 1)
            .is_none_or(|token| token.kind != Kind::Word || token.key.is_some())
            || tokens.get(offset + 2).is_none_or(|token| !token.is("("))
        {
            return Err(invalid("method_header"));
        }
        let mut end = group_end(tokens, offset + 2)?;
        if tokens
            .get(end)
            .is_some_and(|token| token.key == Some(Key::Export))
        {
            end += 1;
        }
        return Ok((end, UnitKind::Method));
    }
    match key {
        Some(Key::If | Key::ElseIf) => Ok((until(tokens, at, Some(Key::Then))?, UnitKind::Header)),
        Some(Key::For | Key::While) => Ok((until(tokens, at, Some(Key::Do))?, UnitKind::Header)),
        Some(Key::Else | Key::Try | Key::Except) => Ok((at + 1, UnitKind::Header)),
        Some(Key::EndIf | Key::EndLoop | Key::EndTry | Key::EndProcedure | Key::EndFunction) => {
            let end = at + 1 + usize::from(tokens.get(at + 1).is_some_and(|token| token.is(";")));
            Ok((
                end,
                if matches!(key, Some(Key::EndProcedure | Key::EndFunction)) {
                    UnitKind::MethodEnd
                } else {
                    UnitKind::End
                },
            ))
        }
        _ if first.is("~") => {
            if tokens
                .get(at + 1)
                .is_none_or(|token| token.kind != Kind::Word || token.key.is_some())
                || tokens.get(at + 2).is_none_or(|token| !token.is(":"))
            {
                return Err(invalid("label"));
            }
            Ok((at + 3, UnitKind::Label))
        }
        _ => Ok((until(tokens, at, None)?, UnitKind::Statement)),
    }
}

/// Nested calls and subscripts cannot make a semicolon or header keyword end an outer unit.
fn until(tokens: &[Token<'_>], start: usize, terminal: Option<Key>) -> Result<usize, Error> {
    let mut at = start;
    while at < tokens.len() {
        if tokens[at].is("(") || tokens[at].is("[") {
            at = group_end(tokens, at)?;
            continue;
        }
        if tokens[at].is(")") || tokens[at].is("]") {
            return Err(invalid("delimiter"));
        }
        if tokens[at].is(";") {
            return if terminal.is_none() {
                Ok(at + 1)
            } else {
                Err(invalid("header"))
            };
        }
        if terminal.is_some() && tokens[at].key == terminal {
            return Ok(at + 1);
        }
        if at > start
            && matches!(
                tokens[at].key,
                Some(
                    Key::EndIf
                        | Key::EndLoop
                        | Key::EndTry
                        | Key::EndProcedure
                        | Key::EndFunction
                        | Key::Else
                        | Key::ElseIf
                        | Key::Except
                )
            )
        {
            return if terminal.is_none() {
                Ok(at)
            } else {
                Err(invalid("header"))
            };
        }
        at += 1;
    }
    if terminal.is_some() {
        Err(invalid("header"))
    } else {
        Ok(at)
    }
}

/// A bounded explicit stack checks every delimiter, including differently nested bracket kinds.
pub fn group_end(tokens: &[Token<'_>], start: usize) -> Result<usize, Error> {
    let mut stack = Vec::new();
    for (index, token) in tokens.iter().enumerate().skip(start) {
        match token.text {
            "(" => stack.push(")"),
            "[" => stack.push("]"),
            ")" | "]" if stack.pop() != Some(token.text) => return Err(invalid("delimiter")),
            _ => {}
        }
        if stack.len() > 128 {
            return Err(Error::new("syntax_too_large", "delimiter_depth"));
        }
        if stack.is_empty() {
            return Ok(index + 1);
        }
    }
    Err(invalid("delimiter"))
}

/// Annotation and region names stay on their physical line unless an argument group continues it.
fn line_end(input: &str, tokens: &[Token<'_>], start: usize) -> Result<usize, Error> {
    let mut at = start + 1;
    while at < tokens.len()
        && !input[tokens[at - 1].range.end..tokens[at].range.start].contains('\n')
    {
        if tokens[at].is("(") {
            at = group_end(tokens, at)?;
        } else {
            at += 1;
        }
    }
    Ok(at)
}

impl State {
    /// Block ownership determines indentation before the source is rendered.
    fn accept(&mut self, tokens: &[Token<'_>], kind: UnitKind) -> Result<usize, Error> {
        if kind == UnitKind::Directive {
            self.preprocessor(tokens)?;
            return Ok(0);
        }
        if kind == UnitKind::Annotation {
            return Ok(0);
        }
        if kind == UnitKind::Comment {
            return Ok(self.blocks.len());
        }
        if kind == UnitKind::Deleted {
            return Ok(self.blocks.len());
        }
        validate(tokens, kind)?;
        let key = tokens
            .get(usize::from(tokens[0].key == Some(Key::Async)))
            .and_then(|token| token.key);
        let level = self.blocks.len();
        match key {
            Some(Key::Procedure | Key::Function) => {
                if !self.blocks.is_empty() {
                    return Err(invalid("nested_method"));
                }
                self.blocks.push(if key == Some(Key::Procedure) {
                    Block::Procedure
                } else {
                    Block::Function
                });
            }
            Some(Key::If) => self.blocks.push(Block::If(false)),
            Some(Key::For | Key::While) => self.blocks.push(Block::Loop),
            Some(Key::Try) => self.blocks.push(Block::Try(false)),
            Some(Key::Else | Key::ElseIf) => {
                let Some(Block::If(has_else)) = self.blocks.last_mut() else {
                    return Err(invalid("else"));
                };
                if *has_else {
                    return Err(invalid("else"));
                }
                *has_else = key == Some(Key::Else);
                return Ok(level - 1);
            }
            Some(Key::Except) => {
                let Some(Block::Try(seen)) = self.blocks.last_mut() else {
                    return Err(invalid("except"));
                };
                if *seen {
                    return Err(invalid("except"));
                }
                *seen = true;
                return Ok(level - 1);
            }
            Some(
                Key::EndIf | Key::EndLoop | Key::EndTry | Key::EndProcedure | Key::EndFunction,
            ) => {
                let closed = self.blocks.pop().ok_or_else(|| invalid("end_block"))?;
                if !matches!(
                    (key, closed),
                    (Some(Key::EndIf), Block::If(_))
                        | (Some(Key::EndLoop), Block::Loop)
                        | (Some(Key::EndTry), Block::Try(_))
                        | (Some(Key::EndProcedure), Block::Procedure)
                        | (Some(Key::EndFunction), Block::Function)
                ) {
                    return Err(invalid("end_block"));
                }
                return Ok(level - 1);
            }
            Some(Key::Return)
                if !self
                    .blocks
                    .iter()
                    .any(|block| matches!(block, Block::Procedure | Block::Function)) =>
            {
                return Err(invalid("return"));
            }
            Some(Key::Break | Key::Continue) if !self.blocks.contains(&Block::Loop) => {
                return Err(invalid("loop_control"));
            }
            _ => {}
        }
        if self.blocks.len() > 128 {
            return Err(Error::new("syntax_too_large", "block_depth"));
        }
        Ok(level)
    }

    /// Each preprocessor branch starts from the same syntax state and must merge consistently.
    fn preprocessor(&mut self, tokens: &[Token<'_>]) -> Result<(), Error> {
        match directive(&tokens[0]) {
            "if" | "если" => {
                validate_directive_condition(tokens)?;
                if self.conditions.len() >= 64 {
                    return Err(Error::new("syntax_too_large", "preprocessor_depth"));
                }
                self.conditions.push(Conditional {
                    initial: self.blocks.clone(),
                    branches: Vec::new(),
                    has_else: false,
                });
            }
            "elsif" | "иначеесли" | "else" | "иначе" => {
                let has_condition = matches!(directive(&tokens[0]), "elsif" | "иначеесли");
                if has_condition {
                    validate_directive_condition(tokens)?;
                }
                let frame = self
                    .conditions
                    .last_mut()
                    .ok_or_else(|| invalid("preprocessor_branch"))?;
                if frame.has_else {
                    return Err(invalid("preprocessor_branch"));
                }
                frame.branches.push(self.blocks.clone());
                self.blocks.clone_from(&frame.initial);
                frame.has_else = !has_condition;
            }
            "endif" | "конецесли" => {
                let mut frame = self
                    .conditions
                    .pop()
                    .ok_or_else(|| invalid("preprocessor_end"))?;
                frame.branches.push(self.blocks.clone());
                if !frame.has_else {
                    frame.branches.push(frame.initial);
                }
                if frame.branches.iter().any(|branch| branch != &self.blocks) {
                    return Err(Error::new("unsupported_syntax", "preprocessor_merge"));
                }
            }
            "region" | "область" => self.regions += 1,
            "endregion" | "конецобласти" => {
                self.regions = self
                    .regions
                    .checked_sub(1)
                    .ok_or_else(|| invalid("region"))?;
            }
            "delete" | "удаление" if !self.deleting => self.deleting = true,
            "enddelete" | "конецудаления" if self.deleting => self.deleting = false,
            "insert" | "вставка" if !self.inserting => self.inserting = true,
            "endinsert" | "конецвставки" if self.inserting => self.inserting = false,
            _ => return Err(Error::new("unsupported_syntax", "preprocessor_directive")),
        }
        Ok(())
    }
}

/// The lexer retains original spellings; these fixed lowercase views avoid changing region names.
fn directive(token: &Token<'_>) -> &'static str {
    match token.text.to_lowercase().as_str() {
        "#если" => "если",
        "#иначеесли" => "иначеесли",
        "#иначе" => "иначе",
        "#конецесли" => "конецесли",
        "#область" => "область",
        "#конецобласти" => "конецобласти",
        "#if" => "if",
        "#elsif" => "elsif",
        "#else" => "else",
        "#endif" => "endif",
        "#region" => "region",
        "#endregion" => "endregion",
        "#удаление" => "удаление",
        "#конецудаления" => "конецудаления",
        "#вставка" => "вставка",
        "#конецвставки" => "конецвставки",
        "#delete" => "delete",
        "#enddelete" => "enddelete",
        "#insert" => "insert",
        "#endinsert" => "endinsert",
        _ => "unsupported",
    }
}

/// Directive expressions use the same boolean grammar but retain unknown environment symbols.
fn validate_directive_condition(tokens: &[Token<'_>]) -> Result<(), Error> {
    let tokens = expressions::significant(tokens);
    if tokens
        .last()
        .is_none_or(|token| token.key != Some(Key::Then))
    {
        return Err(invalid("preprocessor_condition"));
    }
    expressions::expression(&tokens[1..tokens.len() - 1])
}

/// Validation dispatch is separate from source layout and never invents missing tokens.
fn validate(tokens: &[Token<'_>], kind: UnitKind) -> Result<(), Error> {
    let mut significant = expressions::significant(tokens);
    if significant.last().is_some_and(|token| token.is(";")) {
        significant.pop();
    }
    if kind == UnitKind::Statement && significant.is_empty() {
        return Ok(());
    }
    if kind == UnitKind::Method {
        let at = usize::from(significant[0].key == Some(Key::Async));
        let closing = significant
            .iter()
            .rposition(|token| token.is(")"))
            .ok_or_else(|| invalid("method_header"))?;
        return expressions::parameters(&significant[at + 3..closing]);
    }
    if kind == UnitKind::Header {
        return match significant[0].key {
            Some(Key::If | Key::ElseIf | Key::While) => {
                expressions::expression(&significant[1..significant.len() - 1])
            }
            Some(Key::For) => validate_for(&significant),
            _ => Ok(()),
        };
    }
    if kind != UnitKind::Statement {
        return Ok(());
    }
    validate_statement(&significant)
}

/// For headers have explicit separators that cannot be inferred from identifier values.
fn validate_for(tokens: &[&Token<'_>]) -> Result<(), Error> {
    if tokens
        .get(1)
        .is_some_and(|token| token.key == Some(Key::Each))
    {
        let at = tokens
            .iter()
            .position(|token| token.key == Some(Key::In))
            .ok_or_else(|| invalid("for_each"))?;
        if at != 3 || tokens[2].kind != Kind::Word || tokens[2].key.is_some() {
            return Err(invalid("for_each"));
        }
        expressions::expression(&tokens[at + 1..tokens.len() - 1])
    } else {
        let at = tokens
            .iter()
            .position(|token| token.key == Some(Key::To))
            .ok_or_else(|| invalid("for"))?;
        if tokens.get(2).is_none_or(|token| !token.is("=")) {
            return Err(invalid("for"));
        }
        expressions::expression(&tokens[1..at])?;
        expressions::expression(&tokens[at + 1..tokens.len() - 1])
    }
}

/// Assignments, calls and control statements are checked without evaluating their operands.
fn validate_statement(tokens: &[&Token<'_>]) -> Result<(), Error> {
    let first = tokens.first().ok_or_else(|| invalid("statement"))?;
    match first.key {
        Some(Key::Var) => {
            let rest = &tokens[1..tokens.len()
                - usize::from(
                    tokens
                        .last()
                        .is_some_and(|token| token.key == Some(Key::Export)),
                )];
            if rest.is_empty()
                || !rest.iter().enumerate().all(|(index, token)| {
                    if index % 2 == 0 {
                        token.kind == Kind::Word && token.key.is_none()
                    } else {
                        token.is(",")
                    }
                })
                || rest.len().is_multiple_of(2)
            {
                return Err(invalid("variable"));
            }
            Ok(())
        }
        Some(Key::Return | Key::Raise | Key::Break | Key::Continue) if tokens.len() == 1 => Ok(()),
        Some(Key::Return | Key::Raise | Key::Execute) => expressions::expression(&tokens[1..]),
        Some(Key::Goto)
            if tokens.len() == 3 && tokens[1].is("~") && tokens[2].kind == Kind::Word =>
        {
            Ok(())
        }
        Some(Key::AddHandler | Key::RemoveHandler) => {
            let comma = tokens
                .iter()
                .position(|token| token.is(","))
                .ok_or_else(|| invalid("handler"))?;
            expressions::expression(&tokens[1..comma])?;
            expressions::expression(&tokens[comma + 1..])
        }
        _ if tokens.iter().any(|token| token.is("=") || token.is("(")) => {
            expressions::expression(tokens)
        }
        _ => Err(Error::new("unsupported_syntax", "statement")),
    }
}

/// Parse failures expose stable grammar categories rather than language-dependent parser text.
fn invalid(field: &str) -> Error {
    Error::new("invalid_syntax", field)
}
