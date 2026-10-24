//! SELECT branches and nested queries share byte ranges; no query is regenerated to expose its children.

use super::{
    Error, Model, Unavailable,
    lex::{Kind, Token},
    model,
};
use std::ops::Range;

/// A query scope owns its UNION branches and one result-order/totals tail.
#[derive(Clone, Debug)]
pub struct QueryScope {
    pub range: Range<usize>,
    pub parent: Option<usize>,
    pub branches: Vec<usize>,
    pub unions: Vec<Union>,
    pub tail: usize,
}

/// A UNION separator keeps comments and whitespace outside the keyword being changed.
#[derive(Clone, Debug)]
pub struct Union {
    pub range: Range<usize>,
    pub keyword: Range<usize>,
    pub all: Option<Range<usize>>,
}

/// Package separators cannot split a string, comment, virtual-table argument or nested SELECT.
pub(super) fn parse(input: &str, tokens: &[Token]) -> Result<Model, Error> {
    let mut result = Model {
        statements: Vec::new(),
        scopes: Vec::new(),
    };
    let mut start = 0;
    let mut token_start = 0;
    for (index, end) in tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.depth == 0 && token.kind == Kind::Symbol(';'))
        .map(|(index, token)| (index, token.range.start))
        .chain([(tokens.len(), input.len())])
    {
        let content: Vec<_> = tokens[token_start..index].iter().collect();
        if content.iter().any(|token| token.kind != Kind::Comment) {
            append(input, start..end, &content, 0, None, &mut result)?;
        }
        token_start = index.saturating_add(1);
        start = end.saturating_add(1);
    }
    Ok(result)
}

/// Allocate all branches before descending, so an edit inside a branch cannot change that branch's identity during validation.
fn append(
    input: &str,
    range: Range<usize>,
    tokens: &[&Token],
    depth: u16,
    parent: Option<usize>,
    result: &mut Model,
) -> Result<(), Error> {
    if result.scopes.len() >= 256 {
        return Err(Error::TooLarge);
    }
    let mut ancestor = parent;
    let mut nesting = 0;
    while let Some(statement) = ancestor {
        nesting += 1;
        if nesting > 32 {
            return Err(Error::TooLarge);
        }
        ancestor = result.scopes[result.statements[statement].scope].parent;
    }
    let top: Vec<_> = tokens
        .iter()
        .copied()
        .filter(|token| token.depth == depth && token.kind != Kind::Comment)
        .collect();
    let separators = unions(input, &top);
    let invalid = separators.is_none();
    let separators = separators.unwrap_or_default();
    let id = result.scopes.len();
    let mut scope = QueryScope {
        tail: tail_start(input, &top).unwrap_or(range.end),
        range: range.clone(),
        parent,
        branches: Vec::new(),
        unions: separators,
    };
    let mut start = range.start;
    for (branch, end) in scope
        .unions
        .iter()
        .map(|value| value.range.start)
        .chain([range.end])
        .enumerate()
    {
        if result.statements.len() >= 256 {
            return Err(Error::TooLarge);
        }
        let content: Vec<_> = tokens
            .iter()
            .copied()
            .filter(|token| token.range.start >= start && token.range.end <= end)
            .collect();
        let mut statement = if invalid {
            model::opaque(start..end, Unavailable::InvalidSyntax)
        } else {
            model::parse_statement(input, start..end, &content, depth)
        };
        statement.scope = id;
        statement.branch = branch;
        statement.nested = parent.is_some();
        statement.result = branch == scope.unions.len();
        if statement.branch != 0
            && statement.clauses.iter().any(|clause| {
                matches!(
                    clause.kind,
                    super::ClauseKind::Into | super::ClauseKind::Destroy
                )
            })
        {
            statement.unavailable = Some(Unavailable::UnsupportedConstruct);
        }
        if (parent.is_some() || !statement.result)
            && statement.clauses.iter().any(|clause| {
                matches!(
                    clause.kind,
                    super::ClauseKind::Order
                        | super::ClauseKind::Index
                        | super::ClauseKind::Totals
                        | super::ClauseKind::TotalGroups
                )
            })
        {
            statement.unavailable = Some(Unavailable::UnsupportedConstruct);
        }
        scope.branches.push(result.statements.len());
        result.statements.push(statement);
        start = scope
            .unions
            .get(branch)
            .map_or(end, |value| value.range.end);
    }
    check_package_shape(&scope, result);
    result.scopes.push(scope);
    nested(input, tokens, id, result)
}

/// Destinations belong to the first SELECT; destroy and orphan index clauses cannot be UNION branches.
fn check_package_shape(scope: &QueryScope, result: &mut Model) {
    if (!scope.unions.is_empty()
        && scope.branches.iter().any(|&branch| {
            result.statements[branch]
                .clauses
                .iter()
                .any(|c| c.kind == super::ClauseKind::Destroy)
        }))
        || (scope.branches.iter().any(|&branch| {
            result.statements[branch]
                .clauses
                .iter()
                .any(|c| c.kind == super::ClauseKind::Index)
        }) && !result.statements[scope.branches[0]]
            .clauses
            .iter()
            .any(|c| c.kind == super::ClauseKind::Into))
    {
        for &branch in &scope.branches {
            result.statements[branch].unavailable = Some(Unavailable::UnsupportedConstruct);
        }
    }
}

/// UNION ALL is one separator; an extra SELECT or trailing UNION stays an exact opaque branch.
fn unions(input: &str, top: &[&Token]) -> Option<Vec<Union>> {
    let mut values = Vec::new();
    for (index, token) in top
        .iter()
        .enumerate()
        .filter(|(_, token)| token.keyword(input, &["UNION", "ОБЪЕДИНИТЬ"]))
    {
        let next = top.get(index + 1)?;
        let all = next.keyword(input, &["ALL", "ВСЕ"]);
        let select = top.get(index + if all { 2 } else { 1 })?;
        if !select.keyword(input, &["SELECT", "ВЫБРАТЬ"]) {
            return None;
        }
        values.push(Union {
            range: token.range.start..select.range.start,
            keyword: token.range.clone(),
            all: all.then(|| next.range.clone()),
        });
    }
    Some(values)
}

/// Result ordering and totals belong to the whole union, after its last SELECT.
fn tail_start(input: &str, top: &[&Token]) -> Option<usize> {
    let last = top
        .iter()
        .rposition(|token| token.keyword(input, &["UNION", "ОБЪЕДИНИТЬ"]))
        .unwrap_or(0);
    top.iter()
        .enumerate()
        .skip(last)
        .find(|(index, token)| {
            token.keyword(input, &["TOTALS", "ИТОГИ", "INDEX", "ИНДЕКСИРОВАТЬ"])
                || (token.keyword(input, &["ORDER", "УПОРЯДОЧИТЬ"])
                    && top
                        .get(index + 1)
                        .is_some_and(|next| next.keyword(input, &["BY", "ПО"])))
        })
        .map(|(_, token)| token.range.start)
}

/// Descend only into a parenthesized SELECT; commas in function arguments never create another query scope.
fn nested(input: &str, tokens: &[&Token], scope: usize, result: &mut Model) -> Result<(), Error> {
    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        if token.kind != Kind::Symbol('(') {
            index += 1;
            continue;
        }
        let Some(next) = tokens[index + 1..]
            .iter()
            .find(|value| value.kind != Kind::Comment)
        else {
            break;
        };
        if !next.keyword(input, &["SELECT", "ВЫБРАТЬ"]) {
            index += 1;
            continue;
        }
        let close = tokens[index + 1..]
            .iter()
            .position(|value| value.kind == Kind::Symbol(')') && value.depth == token.depth)
            .map(|position| position + index + 1)
            .ok_or(Error::InvalidSyntax)?;
        let parent = result.scopes[scope]
            .branches
            .iter()
            .copied()
            .find(|&branch| result.statements[branch].range.contains(&token.range.start))
            .ok_or(Error::InvalidSyntax)?;
        let body = token.range.end..tokens[close].range.start;
        append(
            input,
            body,
            &tokens[index + 1..close],
            token.depth + 1,
            Some(parent),
            result,
        )?;
        index = close + 1;
    }
    Ok(())
}

/// Scope anchors follow the accepted edit plan; removing a branch selects its surviving ordinal neighbour.
pub(super) fn focus(
    before: &Model,
    after: &Model,
    plan: &crate::project::metadata_edit::EditPlan,
    action: &super::Action,
) -> Option<usize> {
    let owner = before.statements.get(action.statement())?;
    let anchor = before.scopes.get(owner.scope)?.range.start;
    let mut offset = 0isize;
    let mut mapped = None;
    for edit in plan.replacements() {
        if edit.range.start > anchor {
            break;
        }
        if edit.range.end > anchor {
            mapped = edit.range.start.checked_add_signed(offset);
            break;
        }
        offset = offset.checked_add(
            isize::try_from(edit.text.len())
                .ok()?
                .checked_sub(isize::try_from(edit.range.len()).ok()?)?,
        )?;
    }
    let anchor = mapped.or_else(|| anchor.checked_add_signed(offset))?;
    let scope = after
        .scopes
        .iter()
        .find(|scope| scope.range.start == anchor)?;
    scope
        .branches
        .get(owner.branch.min(scope.branches.len().checked_sub(1)?))
        .copied()
}
