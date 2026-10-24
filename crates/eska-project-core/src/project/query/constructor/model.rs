//! A conservative SELECT model exposes only clauses whose token boundaries are unambiguous.

use std::ops::Range;

use super::{
    Error,
    lex::{self, Kind, Token},
};

/// Stable clause identifiers are independent of presentation language and IDE controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClauseKind {
    Fields,
    Sources,
    Joins,
    Conditions,
    Group,
    Order,
    Totals,
    TotalGroups,
}

impl ClauseKind {
    /// Negotiated clients receive only clause identifiers they can render safely.
    #[must_use]
    pub const fn all(advanced: bool) -> &'static [Self] {
        if advanced {
            &[
                Self::Fields,
                Self::Sources,
                Self::Joins,
                Self::Conditions,
                Self::Group,
                Self::Order,
                Self::Totals,
                Self::TotalGroups,
            ]
        } else {
            &[
                Self::Fields,
                Self::Sources,
                Self::Joins,
                Self::Conditions,
                Self::Group,
                Self::Order,
            ]
        }
    }
    /// Clause rank chooses an insertion point without moving unsupported or unrelated package text.
    pub(super) const fn rank(self) -> u8 {
        match self {
            Self::Fields => 0,
            Self::Sources => 1,
            Self::Joins => 2,
            Self::Conditions => 3,
            Self::Group => 4,
            Self::Order => 5,
            Self::Totals => 6,
            Self::TotalGroups => 7,
        }
    }

    /// New syntax follows the selected SELECT's language; existing tokens are never regenerated.
    pub(super) const fn keyword(self, russian: bool) -> &'static str {
        match (self, russian) {
            (Self::Fields, true) => "ВЫБРАТЬ",
            (Self::Fields, false) => "SELECT",
            (Self::Sources, true) => "ИЗ",
            (Self::Sources, false) => "FROM",
            (Self::Joins, true) => "ВНУТРЕННЕЕ СОЕДИНЕНИЕ",
            (Self::Joins, false) => "INNER JOIN",
            (Self::Conditions, true) => "ГДЕ",
            (Self::Conditions, false) => "WHERE",
            (Self::Group, true) => "СГРУППИРОВАТЬ ПО",
            (Self::Group, false) => "GROUP BY",
            (Self::Order, true) => "УПОРЯДОЧИТЬ ПО",
            (Self::Order, false) => "ORDER BY",
            (Self::Totals, true) => "ИТОГИ",
            (Self::Totals, false) => "TOTALS",
            (Self::TotalGroups, true) => "ПО",
            (Self::TotalGroups, false) => "BY",
        }
    }
}

/// Unsupported syntax remains an exact opaque statement, with a machine-readable reason for disabled operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unavailable {
    UnsupportedStatement,
    UnsupportedConstruct,
    InvalidSyntax,
    ModelLimit,
}

/// Comments are attached to the node's original byte range and move with that node.
#[derive(Clone, Debug)]
pub struct Item {
    pub range: Range<usize>,
    pub comments: Vec<Range<usize>>,
}

impl Item {
    /// Presentation can edit the expression while the core retains comments separately.
    #[must_use]
    pub fn expression(&self, input: &str) -> String {
        let mut value = String::new();
        let mut end = self.range.start;
        for comment in &self.comments {
            value.push_str(&input[end..comment.start]);
            end = comment.end;
        }
        value.push_str(&input[end..self.range.end]);
        value.trim().to_owned()
    }
}

/// Headers and separators remain in the source; items address only their own lossless text.
#[derive(Clone, Debug)]
pub struct Clause {
    pub kind: ClauseKind,
    pub range: Range<usize>,
    pub body: Range<usize>,
    pub items: Vec<Item>,
    pub comments: Vec<Range<usize>>,
}

impl Clause {
    /// Movement eligibility is shared with adapters; the OVERALL grouping cannot be moved away from its first position.
    #[must_use]
    pub fn move_unavailable(&self, input: &str, from: usize, to: usize) -> Option<Error> {
        if from >= self.items.len() || to >= self.items.len() {
            return Some(Error::InvalidSelection);
        }
        if self.kind == ClauseKind::Joins
            || (self.kind == ClauseKind::TotalGroups
                && self.items.iter().enumerate().any(|(index, item)| {
                    (index == from || index == to)
                        && matches!(item.visual(input, self), Some(super::Expression::Overall))
                }))
        {
            return Some(Error::UnsafeMove);
        }
        None
    }
}

/// Every statement keeps its original range even when no visual operation is safe.
#[derive(Clone, Debug)]
pub struct Statement {
    pub range: Range<usize>,
    pub russian: bool,
    pub unavailable: Option<Unavailable>,
    pub clauses: Vec<Clause>,
    pub scope: usize,
    pub branch: usize,
    pub nested: bool,
    pub result: bool,
}

impl Statement {
    /// Inserting a JOIN requires a main source; opaque syntax never grants permission for a partial rewrite.
    #[must_use]
    pub fn append_unavailable(&self, kind: ClauseKind) -> Option<Error> {
        if self.unavailable.is_some()
            || (matches!(
                kind,
                ClauseKind::Order | ClauseKind::Totals | ClauseKind::TotalGroups
            ) && (self.nested || !self.result))
        {
            Some(Error::Unsupported)
        } else if kind == ClauseKind::Joins
            && !self
                .clauses
                .iter()
                .any(|clause| clause.kind == ClauseKind::Sources)
        {
            Some(Error::SourceRequired)
        } else {
            None
        }
    }
}

/// Ranges, rather than a regenerated AST, are the canonical model consumed by any IDE adapter.
#[derive(Clone, Debug)]
pub struct Model {
    pub statements: Vec<Statement>,
    pub scopes: Vec<super::QueryScope>,
}

impl Model {
    /// Composite definitions need a scope-aware adapter; an older conservative view remains opaque.
    #[must_use]
    pub fn requires_scopes(&self, index: usize) -> bool {
        self.statements.get(index).is_some_and(|statement| {
            statement.nested
                || statement.clauses.iter().any(|clause| {
                    matches!(clause.kind, ClauseKind::Totals | ClauseKind::TotalGroups)
                })
                || self.scopes[statement.scope].branches.len() > 1
                || self.scopes.iter().any(|scope| scope.parent == Some(index))
        })
    }

    /// Known explicit projection counts must agree before a changed UNION can be published.
    #[must_use]
    pub fn union_mismatches(&self, input: &str) -> Vec<usize> {
        self.scopes
            .iter()
            .enumerate()
            .filter_map(|(index, scope)| {
                if scope.branches.len() < 2 {
                    return None;
                }
                let counts: Option<Vec<_>> = scope
                    .branches
                    .iter()
                    .map(|&branch| {
                        let fields = self.statements[branch]
                            .clauses
                            .iter()
                            .find(|clause| clause.kind == ClauseKind::Fields)?;
                        if fields
                            .items
                            .iter()
                            .any(|item| item.wildcard_source(input, fields).is_some())
                        {
                            None
                        } else {
                            Some(fields.items.len())
                        }
                    })
                    .collect();
                counts
                    .filter(|counts| counts.windows(2).any(|pair| pair[0] != pair[1]))
                    .map(|_| index)
            })
            .collect()
    }

    /// Resolve a derived source only through the selected revision's item range and direct parent relationship.
    #[must_use]
    pub fn source_scope(&self, selection: super::Selection) -> Option<usize> {
        let item = self
            .statements
            .get(selection.statement)?
            .clauses
            .get(selection.clause)?
            .items
            .get(selection.item)?;
        self.scopes.iter().position(|scope| {
            scope.parent == Some(selection.statement)
                && scope.range.start >= item.range.start
                && scope.range.end <= item.range.end
        })
    }

    /// Malformed or unknown statements remain readable; resource exhaustion is an explicit bounded error.
    pub(super) fn parse(input: &str) -> Result<Self, Error> {
        let tokens = match lex::tokens(input) {
            Ok(tokens) => tokens,
            Err(Error::InvalidSyntax) => {
                return Ok(Self {
                    statements: vec![opaque(0..input.len(), Unavailable::InvalidSyntax)],
                    scopes: vec![super::QueryScope {
                        range: 0..input.len(),
                        parent: None,
                        branches: vec![0],
                        unions: Vec::new(),
                        tail: input.len(),
                    }],
                });
            }
            Err(error) => return Err(error),
        };
        super::scopes::parse(input, &tokens)
    }

    /// Include nested allocations in the shared query budget, without duplicating source strings.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of_val(self.statements.as_slice())
            + std::mem::size_of_val(self.scopes.as_slice())
            + self
                .scopes
                .iter()
                .map(|scope| {
                    std::mem::size_of_val(scope.branches.as_slice())
                        + std::mem::size_of_val(scope.unions.as_slice())
                })
                .sum::<usize>()
            + self
                .statements
                .iter()
                .map(|statement| {
                    std::mem::size_of_val(statement.clauses.as_slice())
                        + statement
                            .clauses
                            .iter()
                            .map(|clause| {
                                std::mem::size_of_val(clause.items.as_slice())
                                    + std::mem::size_of_val(clause.comments.as_slice())
                                    + clause
                                        .items
                                        .iter()
                                        .map(|item| std::mem::size_of_val(item.comments.as_slice()))
                                        .sum::<usize>()
                            })
                            .sum::<usize>()
                })
                .sum::<usize>()
    }
}

/// An opaque node is still available for exact no-op round trips and text-mode editing after cancellation.
pub(super) const fn opaque(range: Range<usize>, reason: Unavailable) -> Statement {
    Statement {
        range,
        russian: false,
        unavailable: Some(reason),
        clauses: Vec::new(),
        scope: 0,
        branch: 0,
        nested: false,
        result: true,
    }
}

/// Reject package operators and extensions before exposing editable clause boundaries.
pub(super) fn parse_statement(
    input: &str,
    range: Range<usize>,
    tokens: &[&Token],
    depth: u16,
) -> Statement {
    let top: Vec<_> = tokens
        .iter()
        .copied()
        .filter(|token| token.depth == depth && token.kind != Kind::Comment)
        .collect();
    let Some(first) = top.first() else {
        return opaque(range, Unavailable::UnsupportedStatement);
    };
    if !first.keyword(input, &["SELECT", "ВЫБРАТЬ"]) {
        return opaque(range, Unavailable::UnsupportedStatement);
    }
    if unsupported_controls(input, tokens, &top) {
        return opaque(range, Unavailable::UnsupportedConstruct);
    }
    if top
        .iter()
        .filter(|token| token.kind == Kind::Symbol(',') || token.keyword(input, &["AND", "И"]))
        .count()
        > 2048
    {
        return opaque(range, Unavailable::ModelLimit);
    }
    let russian = first.keyword(input, &["ВЫБРАТЬ"]);
    let mut headers = vec![(
        ClauseKind::Fields,
        first.range.start,
        select_end(input, &top),
    )];
    for (index, token) in top.iter().enumerate().skip(1) {
        if let Some((kind, start, end)) = header(input, &top, index, token) {
            headers.push((kind, start, end));
        }
    }
    if headers.len() > 128 {
        return opaque(range, Unavailable::ModelLimit);
    }
    if headers.windows(2).any(|pair| {
        pair[0].0.rank() > pair[1].0.rank()
            || (pair[0].0 == pair[1].0 && pair[0].0 != ClauseKind::Joins)
    }) {
        return opaque(range, Unavailable::UnsupportedConstruct);
    }
    let mut clauses = Vec::new();
    for (index, &(kind, start, body)) in headers.iter().enumerate() {
        let end = headers.get(index + 1).map_or(range.end, |header| header.1);
        let clause = clause(input, kind, start..end, body..end, tokens, depth);
        if clause.items.is_empty() && kind != ClauseKind::Totals {
            return opaque(range, Unavailable::InvalidSyntax);
        }
        clauses.push(clause);
    }
    if clauses
        .iter()
        .any(|clause| clause.kind == ClauseKind::Totals)
        && !clauses
            .iter()
            .any(|clause| clause.kind == ClauseKind::TotalGroups)
    {
        return opaque(range, Unavailable::InvalidSyntax);
    }
    Statement {
        range,
        russian,
        unavailable: None,
        clauses,
        scope: 0,
        branch: 0,
        nested: false,
        result: true,
    }
}

/// Unsupported control syntax is kept opaque rather than guessed from a partial expression model.
fn unsupported_controls(input: &str, tokens: &[&Token], top: &[&Token]) -> bool {
    if tokens
        .iter()
        .any(|token| matches!(token.kind, Kind::Symbol('{' | '}' | '\'' | '`')))
        || top.iter().skip(1).any(|token| {
            token.keyword(
                input,
                &[
                    "SELECT",
                    "ВЫБРАТЬ",
                    "UNION",
                    "ОБЪЕДИНИТЬ",
                    "INTO",
                    "ПОМЕСТИТЬ",
                    "INDEX",
                    "ИНДЕКСИРОВАТЬ",
                    "HAVING",
                    "ИМЕЮЩИЕ",
                    "FOR",
                    "ДЛЯ",
                    "OR",
                    "ИЛИ",
                    "CASE",
                    "ВЫБОР",
                    "BETWEEN",
                    "МЕЖДУ",
                ],
            )
        })
    {
        return true;
    }
    if top.iter().enumerate().any(|(index, token)| {
        (token.keyword(input, &["TOP", "ПЕРВЫЕ"])
            && top
                .get(index + 1)
                .is_none_or(|next| next.kind != Kind::Number))
            || (token.keyword(input, &["GROUP", "СГРУППИРОВАТЬ", "ORDER", "УПОРЯДОЧИТЬ"])
                && top
                    .get(index + 1)
                    .is_none_or(|next| !next.keyword(input, &["BY", "ПО"])))
    }) {
        return true;
    }
    false
}

/// Preserve common SELECT modifiers instead of exposing them as editable field expressions.
fn select_end(input: &str, top: &[&Token]) -> usize {
    let mut index = 1;
    while let Some(token) = top.get(index) {
        if token.keyword(input, &["DISTINCT", "РАЗЛИЧНЫЕ", "ALLOWED", "РАЗРЕШЕННЫЕ"])
        {
            index += 1;
        } else if token.keyword(input, &["TOP", "ПЕРВЫЕ"])
            && top
                .get(index + 1)
                .is_some_and(|next| next.kind == Kind::Number)
        {
            index += 2;
        } else {
            break;
        }
    }
    top.get(index.saturating_sub(1))
        .map_or(0, |token| token.range.end)
}

/// JOIN modifiers are part of their clause header, never the preceding source expression.
fn header(
    input: &str,
    top: &[&Token],
    index: usize,
    token: &Token,
) -> Option<(ClauseKind, usize, usize)> {
    let kind = if token.keyword(input, &["FROM", "ИЗ"]) {
        ClauseKind::Sources
    } else if token.keyword(input, &["WHERE", "ГДЕ"]) {
        ClauseKind::Conditions
    } else if token.keyword(input, &["TOTALS", "ИТОГИ"]) {
        ClauseKind::Totals
    } else if token.keyword(input, &["BY", "ПО"])
        && top[..index]
            .iter()
            .any(|token| token.keyword(input, &["TOTALS", "ИТОГИ"]))
    {
        ClauseKind::TotalGroups
    } else if token.keyword(input, &["JOIN", "СОЕДИНЕНИЕ"]) {
        ClauseKind::Joins
    } else if token.keyword(input, &["GROUP", "СГРУППИРОВАТЬ"])
        && top
            .get(index + 1)
            .is_some_and(|next| next.keyword(input, &["BY", "ПО"]))
    {
        ClauseKind::Group
    } else if token.keyword(input, &["ORDER", "УПОРЯДОЧИТЬ"])
        && top
            .get(index + 1)
            .is_some_and(|next| next.keyword(input, &["BY", "ПО"]))
    {
        ClauseKind::Order
    } else {
        return None;
    };
    let mut start = index;
    if kind == ClauseKind::Joins {
        while start > 0
            && top[start - 1].keyword(
                input,
                &[
                    "LEFT",
                    "RIGHT",
                    "FULL",
                    "INNER",
                    "OUTER",
                    "ЛЕВОЕ",
                    "ПРАВОЕ",
                    "ПОЛНОЕ",
                    "ВНУТРЕННЕЕ",
                    "ВНЕШНЕЕ",
                ],
            )
        {
            start -= 1;
        }
    }
    let end = if matches!(kind, ClauseKind::Group | ClauseKind::Order) {
        top[index + 1].range.end
    } else {
        token.range.end
    };
    Some((kind, top[start].range.start, end))
}

/// Split only top-level separators; nested calls, quoted commas and comments cannot divide a visual item.
fn clause(
    input: &str,
    kind: ClauseKind,
    range: Range<usize>,
    body: Range<usize>,
    tokens: &[&Token],
    depth: u16,
) -> Clause {
    let comments: Vec<_> = tokens
        .iter()
        .filter(|token| {
            token.kind == Kind::Comment
                && token.range.start >= range.start
                && token.range.end <= range.end
        })
        .map(|token| token.range.clone())
        .collect();
    let mut items = Vec::new();
    let mut start = body.start;
    for end in tokens
        .iter()
        .filter(|token| {
            token.range.start >= body.start
                && token.range.end <= body.end
                && token.depth == depth
                && ((!matches!(kind, ClauseKind::Conditions | ClauseKind::Joins)
                    && token.kind == Kind::Symbol(','))
                    || (kind == ClauseKind::Conditions && token.keyword(input, &["AND", "И"])))
        })
        .map(|token| (token.range.start, token.range.end))
        .chain([(body.end, body.end)])
    {
        if let Some(item) = item(input, start..end.0, &comments) {
            items.push(item);
        }
        start = end.1;
    }
    Clause {
        kind,
        range,
        body,
        items,
        comments,
    }
}

/// Whitespace separators stay outside node ranges; editing supplies safe newlines for moved line comments.
fn item(input: &str, range: Range<usize>, comments: &[Range<usize>]) -> Option<Item> {
    let text = &input[range.clone()];
    let start = range.start + text.len() - text.trim_start().len();
    let end = range.start + text.trim_end().len();
    if start >= end {
        return None;
    }
    let first = comments.partition_point(|comment| comment.start < start);
    let comments = comments[first..]
        .iter()
        .take_while(|comment| comment.end <= end)
        .cloned()
        .collect();
    Some(Item {
        range: start..end,
        comments,
    })
}
