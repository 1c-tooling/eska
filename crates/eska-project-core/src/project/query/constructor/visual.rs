//! Structured controls share one formatter and conservative projection across IDEs.

mod edits;
pub(super) use edits::{augment, path_references, source_prefixes};

use super::{
    Action, Clause, ClauseKind, Error, Item, JoinKind, Model, Selection,
    lex::{self, Kind},
};

/// Aggregate identifiers remain independent of query language and presentation locale.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Aggregate {
    Sum,
    Count,
    Minimum,
    Maximum,
    Average,
}

/// Visual comparisons never accept arbitrary keywords in place of the selected operator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Like,
    In,
    InHierarchy,
}

/// The core owns syntax construction; adapters supply values from ordinary input controls.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Expression {
    TotalField {
        expression: String,
        alias: String,
        aggregate: Option<Aggregate>,
    },
    TotalGroup {
        expression: String,
        alias: String,
        hierarchy: super::Hierarchy,
        period: Option<super::PeriodRange>,
    },
    Overall,
    SubquerySource {
        alias: String,
    },
    SubqueryJoin {
        alias: String,
        join: JoinKind,
        left: String,
        comparison: Comparison,
        right: String,
    },
    VirtualSource {
        table: String,
        alias: String,
        parameters: Vec<String>,
    },
    VirtualJoin {
        table: String,
        alias: String,
        parameters: Vec<String>,
        join: JoinKind,
        left: String,
        comparison: Comparison,
        right: String,
    },
    Source {
        table: String,
        alias: String,
    },
    Field {
        expression: String,
        alias: String,
        aggregate: Option<Aggregate>,
    },
    Join {
        table: String,
        alias: String,
        join: JoinKind,
        left: String,
        comparison: Comparison,
        right: String,
    },
    Condition {
        left: String,
        comparison: Comparison,
        right: String,
    },
    Having {
        left: String,
        comparison: Comparison,
        right: String,
    },
    Group {
        expression: String,
    },
    Order {
        expression: String,
        descending: bool,
    },
}

/// Existing source aliases are projected once by ESKA rather than reparsed in each IDE.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SourceBinding {
    pub table: String,
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<String>>,
    pub clause: usize,
    pub item: usize,
    pub derived: bool,
}

impl Expression {
    /// The expression type determines its destination clause.
    pub(super) const fn clause(&self) -> ClauseKind {
        match self {
            Self::Source { .. } | Self::VirtualSource { .. } | Self::SubquerySource { .. } => {
                ClauseKind::Sources
            }
            Self::Field { .. } => ClauseKind::Fields,
            Self::Join { .. } | Self::VirtualJoin { .. } | Self::SubqueryJoin { .. } => {
                ClauseKind::Joins
            }
            Self::Condition { .. } => ClauseKind::Conditions,
            Self::Having { .. } => ClauseKind::Having,
            Self::Group { .. } => ClauseKind::Group,
            Self::Order { .. } => ClauseKind::Order,
            Self::TotalField { .. } => ClauseKind::Totals,
            Self::TotalGroup { .. } | Self::Overall => ClauseKind::TotalGroups,
        }
    }

    /// Required controls reject empty operands before any formatting or mutation.
    fn complete(&self) -> bool {
        match self {
            Self::VirtualSource { .. }
            | Self::VirtualJoin { .. }
            | Self::SubquerySource { .. }
            | Self::SubqueryJoin { .. } => false,
            Self::Source { table, .. } => !table.trim().is_empty(),
            Self::Field { expression, .. }
            | Self::TotalField { expression, .. }
            | Self::TotalGroup { expression, .. }
            | Self::Group { expression }
            | Self::Order { expression, .. } => !expression.trim().is_empty(),
            Self::Join {
                table, left, right, ..
            } => !table.trim().is_empty() && !left.trim().is_empty() && !right.trim().is_empty(),
            Self::Condition { left, right, .. } | Self::Having { left, right, .. } => {
                !left.trim().is_empty() && !right.trim().is_empty()
            }
            Self::Overall => true,
        }
    }

    /// Generate only the requested fragment; the lossless editor still validates boundaries and owns comments.
    pub(super) fn text(&self, russian: bool) -> Result<String, Error> {
        if !self.complete() {
            return Err(Error::InvalidExpression);
        }
        let with_alias = |text: &str, alias: &str| -> Result<String, Error> {
            if alias.is_empty() {
                return Ok(text.to_owned());
            }
            identifier(alias)?;
            Ok(format!(
                "{text} {} {alias}",
                if russian { "КАК" } else { "AS" }
            ))
        };
        Ok(match self {
            Self::VirtualSource { .. }
            | Self::VirtualJoin { .. }
            | Self::SubquerySource { .. }
            | Self::SubqueryJoin { .. } => {
                return Err(Error::InvalidExpression);
            }
            Self::Source { table, alias } => with_alias(table, alias)?,
            Self::Field {
                expression,
                alias,
                aggregate,
            }
            | Self::TotalField {
                expression,
                alias,
                aggregate,
            } => {
                let value = aggregate.map_or_else(
                    || expression.clone(),
                    |value| format!("{}({expression})", value.keyword(russian)),
                );
                with_alias(&value, alias)?
            }
            Self::Condition {
                left,
                comparison,
                right,
            }
            | Self::Having {
                left,
                comparison,
                right,
            } => format!("{left} {} {right}", comparison.keyword(russian)),
            Self::Join {
                table,
                alias,
                left,
                comparison,
                right,
                ..
            } => {
                let right = if !alias.is_empty() && identifier(right).is_ok() {
                    format!("{alias}.{right}")
                } else {
                    right.clone()
                };
                format!(
                    "{} {} {left} {} {right}",
                    with_alias(table, alias)?,
                    if russian { "ПО" } else { "ON" },
                    comparison.keyword(russian)
                )
            }
            Self::Group { expression } => expression.clone(),
            Self::Order {
                expression,
                descending,
            } => format!(
                "{expression} {}",
                match (russian, descending) {
                    (true, true) => "УБЫВ",
                    (true, false) => "ВОЗР",
                    (false, true) => "DESC",
                    (false, false) => "ASC",
                }
            ),
            Self::TotalGroup {
                expression,
                alias,
                hierarchy,
                period,
            } => super::totals::group(expression, alias, *hierarchy, period.as_ref(), russian)?,
            Self::Overall => {
                if russian {
                    "ОБЩИЕ".into()
                } else {
                    "OVERALL".into()
                }
            }
        })
    }
}

impl Aggregate {
    /// Function spelling follows the selected statement rather than the editor locale.
    pub(super) const fn keyword(self, russian: bool) -> &'static str {
        match (self, russian) {
            (Self::Sum, true) => "СУММА",
            (Self::Sum, false) => "SUM",
            (Self::Count, true) => "КОЛИЧЕСТВО",
            (Self::Count, false) => "COUNT",
            (Self::Minimum, true) => "МИНИМУМ",
            (Self::Minimum, false) => "MIN",
            (Self::Maximum, true) => "МАКСИМУМ",
            (Self::Maximum, false) => "MAX",
            (Self::Average, true) => "СРЕДНЕЕ",
            (Self::Average, false) => "AVG",
        }
    }
}

impl Comparison {
    /// Keep localized query syntax outside the renderer.
    const fn keyword(self, russian: bool) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::NotEqual => "<>",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
            Self::Like => {
                if russian {
                    "ПОДОБНО"
                } else {
                    "LIKE"
                }
            }
            Self::In => {
                if russian {
                    "В"
                } else {
                    "IN"
                }
            }
            Self::InHierarchy => {
                if russian {
                    "В ИЕРАРХИИ"
                } else {
                    "IN HIERARCHY"
                }
            }
        }
    }
}

/// Parameter inputs become one checked source fragment before the ordinary lossless operation is planned.
fn normalize_virtual(expression: &Expression, russian: bool) -> Result<Option<Expression>, Error> {
    let nested = if russian {
        "(ВЫБРАТЬ 1 КАК Значение)"
    } else {
        "(SELECT 1 AS Value)"
    };
    Ok(match expression {
        Expression::SubquerySource { alias } => Some(Expression::Source {
            table: nested.into(),
            alias: alias.clone(),
        }),
        Expression::SubqueryJoin {
            alias,
            join,
            left,
            comparison,
            right,
        } => Some(Expression::Join {
            table: nested.into(),
            alias: alias.clone(),
            join: *join,
            left: left.clone(),
            comparison: *comparison,
            right: right.clone(),
        }),
        Expression::VirtualSource {
            table,
            alias,
            parameters,
        } => Some(Expression::Source {
            table: super::super::virtual_tables::call(table, parameters)?,
            alias: alias.clone(),
        }),
        Expression::VirtualJoin {
            table,
            alias,
            parameters,
            join,
            left,
            comparison,
            right,
        } => Some(Expression::Join {
            table: super::super::virtual_tables::call(table, parameters)?,
            alias: alias.clone(),
            join: *join,
            left: left.clone(),
            comparison: *comparison,
            right: right.clone(),
        }),
        _ => None,
    })
}

/// A new source receives one collision-free alias in the query language, including derived tables.
fn automatic_alias(
    input: &str,
    owner: &super::Statement,
    expression: &Expression,
) -> Result<Option<Expression>, Error> {
    let (Expression::Source { table, alias } | Expression::Join { table, alias, .. }) = expression
    else {
        return Ok(None);
    };
    if !alias.is_empty() {
        return Ok(None);
    }
    let name = if table.starts_with('(') {
        if owner.russian {
            "ВложенныйЗапрос"
        } else {
            "NestedQuery"
        }
    } else {
        table
            .split('(')
            .next()
            .unwrap_or(table)
            .rsplit('.')
            .next()
            .ok_or(Error::InvalidExpression)?
    };
    identifier(name)?;
    let bindings = owner.source_bindings(input);
    let mut alias = name.to_owned();
    let mut suffix = 1;
    while bindings.iter().any(|binding| {
        binding.alias.to_lowercase() == alias.to_lowercase()
            || binding
                .alias
                .rsplit_once('.')
                .is_some_and(|(_, name)| name.to_lowercase() == alias.to_lowercase())
    }) {
        suffix += 1;
        alias = format!("{name}{suffix}");
    }
    Ok(Some(match expression {
        Expression::Source { table, .. } => Expression::Source {
            table: table.clone(),
            alias,
        },
        Expression::Join {
            table,
            join,
            left,
            comparison,
            right,
            ..
        } => Expression::Join {
            table: table.clone(),
            alias,
            join: *join,
            left: left.clone(),
            comparison: *comparison,
            right: right.clone(),
        },
        _ => return Err(Error::InvalidExpression),
    }))
}

/// Lower a visual form to the same minimal operation used by the expression editor.
pub(super) fn compose(
    input: &str,
    model: &Model,
    statement: usize,
    selection: Option<Selection>,
    expression: &Expression,
) -> Result<Action, Error> {
    let requested = expression;
    let owner = model
        .statements
        .get(statement)
        .ok_or(Error::InvalidSelection)?;
    if matches!(expression, Expression::Overall) && selection.is_none() {
        for (clause, group) in owner
            .clauses
            .iter()
            .enumerate()
            .filter(|(_, clause)| clause.kind == ClauseKind::TotalGroups)
            .take(1)
        {
            if let Some((item, value)) =
                group.items.iter().enumerate().find(|(_, item)| {
                    matches!(item.visual(input, group), Some(Expression::Overall))
                })
            {
                return Ok(Action::Replace {
                    selection: Selection {
                        statement,
                        clause,
                        item,
                    },
                    text: value.expression(input),
                });
            }
        }
    }
    let normalized = normalize_virtual(expression, owner.russian)?;
    let expression = normalized.as_ref().unwrap_or(expression);
    let automatic = if selection.is_none() {
        automatic_alias(input, owner, expression)?
    } else {
        None
    };
    let expression = automatic.as_ref().unwrap_or(expression);
    let mut text = expression.text(owner.russian)?;
    if text.len() > 16_384 {
        return Err(Error::TooLarge);
    }
    if let Some(selection) = selection {
        if selection.statement != statement
            || owner
                .clauses
                .get(selection.clause)
                .is_none_or(|clause| clause.kind != expression.clause())
        {
            return Err(Error::InvalidSelection);
        }
        let clause = &owner.clauses[selection.clause];
        if let Some(item) = clause.items.get(selection.item)
            && item.visual(input, clause).as_ref() == Some(requested)
        {
            text = item.expression(input);
        }
        Ok(Action::Replace { selection, text })
    } else {
        Ok(Action::Append {
            statement,
            clause: expression.clause(),
            text,
            join: if let Expression::Join { join, .. } = expression {
                *join
            } else {
                JoinKind::Inner
            },
        })
    }
}

impl Item {
    /// A result-column name is proven only by an explicit alias or a qualified identifier.
    #[must_use]
    pub fn output_name(&self, input: &str, clause: &Clause) -> Option<String> {
        let Expression::Field {
            expression, alias, ..
        } = self.visual(input, clause)?
        else {
            return None;
        };
        if !alias.is_empty() {
            return Some(alias);
        }
        let tokens = lex::tokens(&expression).ok()?;
        if tokens.is_empty()
            || tokens.len() % 2 == 0
            || tokens.iter().enumerate().any(|(index, token)| {
                token.kind
                    != if index % 2 == 0 {
                        Kind::Word
                    } else {
                        Kind::Symbol('.')
                    }
            })
        {
            return None;
        }
        Some(expression[tokens.last()?.range.clone()].to_owned())
    }

    /// Wildcard expansion belongs to ESKA metadata resolution, including qualified source wildcards.
    #[must_use]
    pub fn wildcard_source(&self, input: &str, clause: &Clause) -> Option<Option<String>> {
        let Expression::Field {
            expression,
            alias,
            aggregate: None,
        } = self.visual(input, clause)?
        else {
            return None;
        };
        if !alias.is_empty() {
            return None;
        }
        let tokens = lex::tokens(&expression).ok()?;
        if tokens.last()?.kind != Kind::Symbol('*') {
            return None;
        }
        if tokens.len() == 1 {
            return Some(None);
        }
        if tokens.len() % 2 == 0
            || tokens[..tokens.len() - 1]
                .iter()
                .enumerate()
                .any(|(index, token)| {
                    token.kind
                        != if index % 2 == 0 {
                            Kind::Word
                        } else {
                            Kind::Symbol('.')
                        }
                })
        {
            return None;
        }
        Some(Some(
            expression[..tokens[tokens.len() - 2].range.start]
                .trim()
                .to_owned(),
        ))
    }

    /// Only recognized row components populate structured controls; unknown syntax stays in expression mode.
    #[must_use]
    pub fn visual(&self, input: &str, clause: &Clause) -> Option<Expression> {
        let text = self.expression(input);
        match clause.kind {
            ClauseKind::Fields | ClauseKind::Totals => {
                let (value, alias) = split_alias(&text)?;
                let (expression, aggregate) = split_aggregate(&value);
                Some(if clause.kind == ClauseKind::Totals {
                    Expression::TotalField {
                        expression,
                        alias,
                        aggregate,
                    }
                } else {
                    Expression::Field {
                        expression,
                        alias,
                        aggregate,
                    }
                })
            }
            ClauseKind::Sources => {
                if let Some(source) = super::nested::source(&text) {
                    return Some(Expression::SubquerySource {
                        alias: source.alias,
                    });
                }
                let (table, alias) = split_alias(&text)?;
                Some(
                    if let Some((name, parameters)) =
                        super::super::virtual_tables::split_call(&table)
                    {
                        Expression::VirtualSource {
                            table: name.to_owned(),
                            alias,
                            parameters,
                        }
                    } else {
                        Expression::Source { table, alias }
                    },
                )
            }
            ClauseKind::Conditions | ClauseKind::Having => {
                let (left, comparison, right) = comparison(&text)?;
                Some(if clause.kind == ClauseKind::Having {
                    Expression::Having {
                        left,
                        comparison,
                        right,
                    }
                } else {
                    Expression::Condition {
                        left,
                        comparison,
                        right,
                    }
                })
            }
            ClauseKind::Group => Some(Expression::Group { expression: text }),
            ClauseKind::Order => {
                let tokens = lex::tokens(&text).ok()?;
                let last = tokens.last()?;
                let direction =
                    last.depth == 0 && last.keyword(&text, &["ASC", "DESC", "ВОЗР", "УБЫВ"]);
                Some(Expression::Order {
                    expression: if direction {
                        text[..last.range.start].trim().to_owned()
                    } else {
                        text.clone()
                    },
                    descending: direction && last.keyword(&text, &["DESC", "УБЫВ"]),
                })
            }
            ClauseKind::Joins => {
                join_projection(&text, &input[clause.range.start..clause.body.start])
            }
            ClauseKind::TotalGroups => super::totals::project(&text),
            ClauseKind::Into | ClauseKind::Destroy | ClauseKind::Index => None,
        }
    }
}

/// JOIN projections share the same header and source parsing for physical, virtual and nested tables.
fn join_projection(text: &str, header: &str) -> Option<Expression> {
    let tokens = lex::tokens(text).ok()?;
    let on = tokens
        .iter()
        .find(|token| token.depth == 0 && token.keyword(text, &["ON", "ПО"]))?;
    let (table, alias) = split_alias(text[..on.range.start].trim())?;
    let (left, comparison, right) = comparison(text[on.range.end..].trim())?;
    let tokens = lex::tokens(header).ok()?;
    let join = if tokens
        .iter()
        .any(|token| token.keyword(header, &["LEFT", "ЛЕВОЕ"]))
    {
        JoinKind::Left
    } else if tokens
        .iter()
        .any(|token| token.keyword(header, &["RIGHT", "ПРАВОЕ"]))
    {
        JoinKind::Right
    } else if tokens
        .iter()
        .any(|token| token.keyword(header, &["FULL", "ПОЛНОЕ"]))
    {
        JoinKind::Full
    } else {
        JoinKind::Inner
    };
    if let Some(source) = super::nested::source(text) {
        return Some(Expression::SubqueryJoin {
            alias: source.alias,
            join,
            left,
            comparison,
            right,
        });
    }
    if let Some((name, parameters)) = super::super::virtual_tables::split_call(&table) {
        return Some(Expression::VirtualJoin {
            table: name.to_owned(),
            alias,
            parameters,
            join,
            left,
            comparison,
            right,
        });
    }
    Some(Expression::Join {
        table,
        alias,
        join,
        left,
        comparison,
        right,
    })
}

impl super::Statement {
    /// FROM and JOIN bindings are shared metadata inputs for all five visual pages.
    #[must_use]
    pub fn source_bindings(&self, input: &str) -> Vec<SourceBinding> {
        self.clauses
            .iter()
            .enumerate()
            .filter(|(_, clause)| matches!(clause.kind, ClauseKind::Sources | ClauseKind::Joins))
            .flat_map(|(clause, value)| {
                value
                    .items
                    .iter()
                    .enumerate()
                    .map(move |(item, value)| (clause, item, value))
            })
            .filter_map(|(clause, index, item)| {
                let expression = item.expression(input);
                if let Some(source) = super::nested::source(&expression) {
                    return Some(SourceBinding {
                        table: source.alias.clone(),
                        alias: source.alias,
                        parameters: None,
                        clause,
                        item: index,
                        derived: true,
                    });
                }
                if expression.len() > 16_384 {
                    return None;
                }
                let tokens = lex::tokens(&expression).ok()?;
                let end = tokens
                    .iter()
                    .find(|token| token.depth == 0 && token.keyword(&expression, &["ON", "ПО"]))
                    .map_or(expression.len(), |token| token.range.start);
                let (table, alias) = split_alias(expression[..end].trim())?;
                let alias = if alias.is_empty() {
                    table.split('(').next()?.to_owned()
                } else {
                    alias
                };
                let (table, parameters) = super::super::virtual_tables::split_call(&table)
                    .map_or_else(
                        || (table.clone(), None),
                        |(name, parameters)| (name.to_owned(), Some(parameters)),
                    );
                Some(SourceBinding {
                    table,
                    alias,
                    parameters,
                    clause,
                    item: index,
                    derived: false,
                })
            })
            .collect()
    }
}

/// Alias inputs cannot introduce another expression, qualification or whitespace.
pub(super) fn identifier(value: &str) -> Result<(), Error> {
    let mut chars = value.chars();
    if !chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_alphabetic())
        || !chars.all(|ch| ch == '_' || ch.is_alphanumeric())
    {
        return Err(Error::InvalidExpression);
    }
    Ok(())
}

/// Split only a terminal explicit AS alias, never AS inside strings or a nested expression.
pub(super) fn split_alias(text: &str) -> Option<(String, String)> {
    let tokens = lex::tokens(text).ok()?;
    if let Some((index, token)) = tokens
        .iter()
        .enumerate()
        .rev()
        .find(|(_, token)| token.depth == 0 && token.keyword(text, &["AS", "КАК"]))
    {
        if index + 2 != tokens.len() {
            return None;
        }
        let alias = text[tokens[index + 1].range.clone()].to_owned();
        identifier(&alias).ok()?;
        return Some((text[..token.range.start].trim().to_owned(), alias));
    }
    Some((text.to_owned(), String::new()))
}

/// Recognize an outer aggregate while retaining the exact operand expression.
fn split_aggregate(text: &str) -> (String, Option<Aggregate>) {
    let Some(open) = text.find('(') else {
        return (text.to_owned(), None);
    };
    let aggregate = [
        Aggregate::Sum,
        Aggregate::Count,
        Aggregate::Minimum,
        Aggregate::Maximum,
        Aggregate::Average,
    ]
    .into_iter()
    .find(|value| {
        [value.keyword(true), value.keyword(false)]
            .contains(&text[..open].trim().to_uppercase().as_str())
    });
    let entire = lex::tokens(text).is_ok_and(|tokens| {
        tokens
            .iter()
            .filter(|token| token.depth == 0 && token.kind == Kind::Symbol(')'))
            .count()
            == 1
            && tokens
                .last()
                .is_some_and(|token| token.kind == Kind::Symbol(')') && token.depth == 0)
    });
    if aggregate.is_some() && entire {
        (text[open + 1..text.len() - 1].to_owned(), aggregate)
    } else {
        (text.to_owned(), None)
    }
}

/// Complex predicates remain exact expressions instead of being guessed into a binary condition.
fn comparison(text: &str) -> Option<(String, Comparison, String)> {
    let tokens = lex::tokens(text).ok()?;
    for (index, token) in tokens.iter().enumerate() {
        if token.depth != 0 {
            continue;
        }
        let mut end = token.range.end;
        let operator = match token.kind {
            Kind::Symbol('=') => Comparison::Equal,
            Kind::Symbol('<' | '>') => {
                let next = tokens.get(index + 1).filter(|next| next.range.start == end);
                match (
                    &text[token.range.clone()],
                    next.map(|next| &text[next.range.clone()]),
                ) {
                    ("<", Some("=")) => {
                        end += 1;
                        Comparison::LessOrEqual
                    }
                    (">", Some("=")) => {
                        end += 1;
                        Comparison::GreaterOrEqual
                    }
                    ("<", Some(">")) => {
                        end += 1;
                        Comparison::NotEqual
                    }
                    ("<", _) => Comparison::Less,
                    _ => Comparison::Greater,
                }
            }
            Kind::Word if token.keyword(text, &["LIKE", "ПОДОБНО"]) => Comparison::Like,
            Kind::Word if token.keyword(text, &["IN", "В"]) => tokens
                .get(index + 1)
                .filter(|next| next.keyword(text, &["HIERARCHY", "ИЕРАРХИИ"]))
                .map_or(Comparison::In, |next| {
                    end = next.range.end;
                    Comparison::InHierarchy
                }),
            _ => continue,
        };
        let left = text[..token.range.start].trim();
        let right = text[end..].trim();
        if !left.is_empty() && !right.is_empty() {
            return Some((left.to_owned(), operator, right.to_owned()));
        }
    }
    None
}
