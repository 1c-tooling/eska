//! The installed platform grammar determines extension slots; balanced braces alone never imply support.

use super::{DcsKind, DcsModel, DcsNode, DcsStatement, DcsValue};
use crate::project::query::{
    constructor::{
        ClauseKind, Error, Model,
        lex::{Kind, Token},
        visual,
    },
    virtual_tables::VirtualTable,
};
use std::ops::Range;

struct Block {
    range: Range<usize>,
    body: Range<usize>,
    kind: DcsKind,
}

/// A same-byte-length masked view proves ordinary clause boundaries but is never supplied to the editor or saved.
pub(super) fn inspect(input: &str, original: &Model, tokens: &[Token]) -> Result<DcsModel, Error> {
    let blocks = blocks(input, tokens);
    let mut masked = input.as_bytes().to_vec();
    for block in &blocks {
        if matches!(block.kind, DcsKind::OptionalSource | DcsKind::Parameter) {
            masked[block.range.start] = b' ';
            masked[block.range.end - 1] = b' ';
        } else {
            for byte in &mut masked[block.range.clone()] {
                if !matches!(*byte, b'\r' | b'\n') {
                    *byte = b' ';
                }
            }
        }
    }
    mask_cases(&mut masked)?;
    let masked = String::from_utf8(masked).map_err(|_| Error::InvalidSyntax)?;
    let parsed = Model::parse(&masked)?;
    let mut output = super::empty(original);
    for (index, owner) in original.statements.iter().enumerate() {
        let Some(base) = parsed
            .statements
            .iter()
            .find(|base| base.range == owner.range)
        else {
            continue;
        };
        let state = &mut output.statements[index];
        state.russian = base.russian;
        if base.unavailable.is_none() {
            sites(base, &masked, &blocks, state);
            standalone(input, base, &blocks, state);
            sources(input, &masked, base, &blocks, state)?;
        }
        for block in &blocks {
            if block.range.start < owner.range.start
                || block.range.end > owner.range.end
                || original.statements.iter().any(|child| {
                    child.nested
                        && child.range != owner.range
                        && child.range.start <= block.range.start
                        && child.range.end >= block.range.end
                        && child.range.len() < owner.range.len()
                })
                || state.nodes.iter().any(|node| node.range == block.range)
            {
                continue;
            }
            push(
                input,
                state,
                DcsKind::Opaque,
                block.range.clone(),
                None,
                true,
            );
        }
        state
            .nodes
            .sort_by_key(|node| (node.range.start, node.range.end));
        for (index, node) in state.nodes.iter_mut().enumerate() {
            node.index = index;
        }
        state.add_select =
            state.select.is_some() && !state.nodes.iter().any(|node| node.kind == DcsKind::Select);
        state.add_where = state.condition.is_some()
            && !state.nodes.iter().any(|node| node.kind == DcsKind::Where);
    }
    Ok(output)
}

/// Balanced scalar CASE expressions are opaque values for boundary proof; their original bytes are never edited.
fn mask_cases(bytes: &mut [u8]) -> Result<(), Error> {
    let input = std::str::from_utf8(bytes).map_err(|_| Error::InvalidSyntax)?;
    let tokens = super::super::lex::tokens(input)?;
    let mut stack = Vec::new();
    let mut ranges = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if tokens
            .get(index.wrapping_sub(1))
            .is_some_and(|previous| matches!(previous.kind, Kind::Symbol('.' | '&')))
        {
            continue;
        }
        if token.keyword(input, &["CASE", "ВЫБОР"]) {
            stack.push(token);
        } else if token.keyword(input, &["END", "КОНЕЦ"])
            && let Some(open) = stack.pop()
        {
            if open.depth != token.depth {
                return Ok(());
            }
            ranges.push(open.range.start..token.range.end);
        }
    }
    if !stack.is_empty() {
        return Ok(());
    }
    for range in ranges {
        for byte in &mut bytes[range.clone()] {
            if !matches!(*byte, b'\r' | b'\n') {
                *byte = b' ';
            }
        }
        bytes[range.start] = b'0';
    }
    Ok(())
}

/// Braces inside strings and comments cannot become nodes; nested blocks keep independent source ranges.
fn blocks(input: &str, tokens: &[Token]) -> Vec<Block> {
    let mut stack = Vec::new();
    let mut result = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.kind == Kind::Symbol('{') {
            stack.push(index);
        } else if token.kind == Kind::Symbol('}')
            && let Some(open) = stack.pop()
        {
            let first = tokens[open + 1..index]
                .iter()
                .find(|token| token.kind != Kind::Comment);
            let kind = first.map_or(DcsKind::Opaque, |first| {
                if first.keyword(input, &["SELECT", "ВЫБРАТЬ"]) {
                    DcsKind::Select
                } else if first.keyword(input, &["WHERE", "ГДЕ"]) {
                    DcsKind::Where
                } else if first.keyword(
                    input,
                    &[
                        "LEFT",
                        "RIGHT",
                        "INNER",
                        "FULL",
                        "ЛЕВОЕ",
                        "ПРАВОЕ",
                        "ВНУТРЕННЕЕ",
                        "ПОЛНОЕ",
                    ],
                ) {
                    DcsKind::OptionalSource
                } else if first.kind == Kind::Symbol('&') {
                    DcsKind::Parameter
                } else {
                    DcsKind::Opaque
                }
            });
            result.push(Block {
                range: tokens[open].range.start..token.range.end,
                body: tokens[open].range.end..token.range.start,
                kind,
            });
        }
    }
    result.sort_by_key(|block| block.range.start);
    result
}

/// SELECT availability refers to first-branch output aliases; WHERE extensions follow ordinary source/WHERE clauses.
fn sites(base: &super::super::Statement, masked: &str, blocks: &[Block], state: &mut DcsStatement) {
    if let Some(fields) = base
        .clauses
        .iter()
        .find(|clause| clause.kind == ClauseKind::Fields)
    {
        state.columns = fields
            .items
            .iter()
            .filter_map(|item| item.output_name(masked, fields))
            .collect();
        state.select = (base.branch == 0)
            .then(|| fields.items.last().map(|item| item.range.end))
            .flatten();
    }
    state.condition = base
        .clauses
        .iter()
        .filter(|clause| clause.kind.rank() <= ClauseKind::Conditions.rank())
        .filter_map(|clause| clause.items.last())
        .map(|item| item.range.end)
        .next_back();
    if let Some(at) = state.condition.as_mut() {
        for block in blocks {
            if (block.kind == DcsKind::OptionalSource
                && block.body.start <= *at
                && block.body.end >= *at)
                || (block.kind == DcsKind::Select
                    && block.range.start >= *at
                    && block.range.end
                        <= base
                            .clauses
                            .iter()
                            .find(|clause| clause.kind.rank() > ClauseKind::Conditions.rank())
                            .map_or(base.range.end, |clause| clause.range.start))
            {
                *at = block.range.end;
            }
        }
    }
}

/// A recognized keyword in a wrong position remains opaque and cannot be rewritten through a typed request.
fn standalone(
    input: &str,
    base: &super::super::Statement,
    blocks: &[Block],
    state: &mut DcsStatement,
) {
    for kind in [DcsKind::Select, DcsKind::Where] {
        let start = if kind == DcsKind::Select {
            state.select
        } else {
            state.condition
        };
        let Some(start) = start else {
            continue;
        };
        let rank = if kind == DcsKind::Select {
            ClauseKind::Fields.rank()
        } else {
            ClauseKind::Conditions.rank()
        };
        let end = base
            .clauses
            .iter()
            .find(|clause| clause.kind.rank() > rank)
            .map_or(base.range.end, |clause| clause.range.start);
        let selected: Vec<_> = blocks
            .iter()
            .filter(|block| {
                block.kind == kind && block.range.start >= start && block.range.end <= end
            })
            .collect();
        for block in &selected {
            let body = &input[block.body.clone()];
            let keyword = super::super::lex::tokens(body)
                .ok()
                .and_then(|tokens| tokens.into_iter().find(|token| token.kind != Kind::Comment));
            let values = keyword.and_then(|keyword| values(&body[keyword.range.end..], kind));
            let values = values
                .filter(|values| kind != DcsKind::Select || valid_columns(values, &state.columns));
            push(
                input,
                state,
                kind,
                block.range.clone(),
                values.filter(|_| selected.len() == 1),
                true,
            );
        }
    }
}

/// Existing JOIN bodies are never regenerated; only their optional wrapper can be toggled.
fn sources(
    input: &str,
    masked: &str,
    base: &super::super::Statement,
    blocks: &[Block],
    state: &mut DcsStatement,
) -> Result<(), Error> {
    for clause in &base.clauses {
        if !matches!(clause.kind, ClauseKind::Sources | ClauseKind::Joins) {
            continue;
        }
        for item in &clause.items {
            let source = if clause.kind == ClauseKind::Joins {
                clause.range.start..item.range.end
            } else {
                item.range.clone()
            };
            if clause.kind == ClauseKind::Joins {
                let wrapper = blocks.iter().find(|block| {
                    block.kind == DcsKind::OptionalSource
                        && block.body.start <= source.start
                        && block.body.end >= source.end
                });
                let range = wrapper.map_or_else(|| source.clone(), |block| block.range.clone());
                push(
                    input,
                    state,
                    DcsKind::OptionalSource,
                    range,
                    Some(Vec::new()),
                    wrapper.is_some(),
                );
            }
            arguments(input, masked, item.range.clone(), state)?;
        }
    }
    Ok(())
}

/// A conditional parameter/filter must occupy a whole argument of a documented register virtual-table signature.
fn arguments(
    input: &str,
    masked: &str,
    range: Range<usize>,
    state: &mut DcsStatement,
) -> Result<(), Error> {
    let text = &masked[range.clone()];
    let tokens = super::super::lex::tokens(text)?;
    let Some(open) = tokens
        .iter()
        .position(|token| token.kind == Kind::Symbol('(') && token.depth == 0)
    else {
        return Ok(());
    };
    let table = text[..tokens[open].range.start].trim();
    let Some((category, table)) = VirtualTable::resolve(table) else {
        return Ok(());
    };
    let Some(close) = tokens
        .iter()
        .skip(open + 1)
        .find(|token| token.kind == Kind::Symbol(')') && token.depth == 0)
    else {
        return Ok(());
    };
    let signature = table.parameters(category, false);
    let mut start = tokens[open].range.end;
    for (index, end) in tokens
        .iter()
        .skip(open + 1)
        .take_while(|token| token.range.start < close.range.start)
        .filter(|token| token.kind == Kind::Symbol(',') && token.depth == 1)
        .map(|token| token.range.start)
        .chain([close.range.start])
        .enumerate()
    {
        let raw = trim(input, range.start + start..range.start + end);
        start = end + 1;
        let Some(parameter) = signature.get(index) else {
            continue;
        };
        let value = &input[raw.clone()];
        let enabled = value.starts_with('{') && value.ends_with('}');
        let body = if enabled {
            &value[1..value.len() - 1]
        } else {
            value
        };
        if ["period", "beginPeriod", "endPeriod"].contains(&parameter.key) {
            let name = body
                .trim()
                .strip_prefix('&')
                .map(str::trim)
                .filter(|name| visual::identifier(name).is_ok());
            if value.is_empty() || name.is_some() {
                push(
                    input,
                    state,
                    DcsKind::Parameter,
                    raw,
                    Some(vec![DcsValue::Parameter {
                        name: name.unwrap_or_default().to_owned(),
                    }]),
                    enabled,
                );
            }
        } else if parameter.key == "condition" {
            let parsed = if body.trim().is_empty() {
                Some(Vec::new())
            } else if enabled {
                values(body, DcsKind::VirtualFilter)
            } else {
                None
            };
            if enabled || parsed.is_some() {
                push(input, state, DcsKind::VirtualFilter, raw, parsed, enabled);
            }
        }
    }
    Ok(())
}

/// Trimming is byte-safe even for Cyrillic identifiers and mixed line endings.
fn trim(input: &str, range: Range<usize>) -> Range<usize> {
    let text = &input[range.clone()];
    let start = range.start + text.len() - text.trim_start().len();
    start..start + text.trim().len()
}

/// Comments are counted from original bytes, never from the masked parsing view.
fn push(
    input: &str,
    state: &mut DcsStatement,
    kind: DcsKind,
    range: Range<usize>,
    values: Option<Vec<DcsValue>>,
    enabled: bool,
) {
    let comments = super::super::lex::tokens(&input[range.clone()]).map_or(0, |tokens| {
        tokens
            .iter()
            .filter(|token| token.kind == Kind::Comment)
            .count()
    });
    state.nodes.push(DcsNode {
        index: 0,
        kind,
        range,
        editable: values.is_some(),
        values: values.unwrap_or_default(),
        enabled,
        comments,
    });
}

/// Commas separate extension values only at their own lexical level, never inside expressions or quoted strings.
pub(super) fn values(text: &str, kind: DcsKind) -> Option<Vec<DcsValue>> {
    let tokens = super::super::lex::tokens(text).ok()?;
    if tokens
        .iter()
        .any(|token| matches!(token.kind, Kind::Symbol('{' | '}')))
    {
        return None;
    }
    let mut start = 0;
    let mut result = Vec::new();
    for end in tokens
        .iter()
        .filter(|token| token.depth == 0 && token.kind == Kind::Symbol(','))
        .map(|token| token.range.start)
        .chain([text.len()])
    {
        let raw = &text[start..end];
        start = end + 1;
        let clean = strip_comments(raw)?;
        if let Some((left, comparison, right)) = visual::comparison(&clean) {
            if kind == DcsKind::Select {
                return None;
            }
            super::super::editing::fragment(&left).ok()?;
            super::super::editing::fragment(&right).ok()?;
            result.push(DcsValue::Condition {
                left,
                comparison,
                right,
            });
        } else {
            let (field, alias) = visual::split_alias(&clean)?;
            let children = field.ends_with(".*");
            let field = if children {
                &field[..field.len() - 2]
            } else {
                &field
            };
            let expression = field
                .trim()
                .trim_start_matches('(')
                .trim_end_matches(')')
                .trim()
                .to_owned();
            path(&expression).ok()?;
            if !alias.is_empty() {
                visual::identifier(&alias).ok()?;
            }
            if kind == DcsKind::Select && (!alias.is_empty() || expression.contains('.')) {
                return None;
            }
            result.push(DcsValue::Field {
                expression,
                children,
                alias,
            });
        }
    }
    (result.len() <= 128).then_some(result)
}

/// Removing comments is a projection operation; rewriting them still requires an explicit decision.
fn strip_comments(input: &str) -> Option<String> {
    let tokens = super::super::lex::tokens(input).ok()?;
    let mut bytes = input.as_bytes().to_vec();
    for token in tokens.iter().filter(|token| token.kind == Kind::Comment) {
        bytes[token.range.clone()].fill(b' ');
    }
    String::from_utf8(bytes)
        .ok()
        .map(|text| text.trim().to_owned())
}

/// Field paths cannot introduce functions, parameters, additional fields or query clauses.
pub(super) fn path(input: &str) -> Result<(), Error> {
    if input.len() > 1024 {
        return Err(Error::TooLarge);
    }
    for part in input.split('.') {
        visual::identifier(part)?;
    }
    Ok(())
}

/// DCS SELECT names must resolve to the first branch's explicit output aliases.
pub(super) fn valid_columns(values: &[DcsValue], columns: &[String]) -> bool {
    values.iter().all(|value|matches!(value,DcsValue::Field{expression,alias,..} if alias.is_empty() && !expression.contains('.') && columns.iter().any(|column|column.to_lowercase()==expression.to_lowercase())))
}
