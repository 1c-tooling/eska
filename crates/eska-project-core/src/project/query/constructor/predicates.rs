//! Boolean structure is syntax owned by ESKA; field types and relationship inference belong to the analyzer.

use super::{
    Action, ClauseKind, Constructor, Error, Item, editing,
    lex::{self, Kind},
    visual,
};
use crate::project::metadata_edit::EditPlan;

/// Unknown leaves remain exact text. Every recursive operation has explicit depth and node limits.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Predicate {
    All {
        children: Vec<Self>,
    },
    Any {
        children: Vec<Self>,
    },
    Not {
        child: Box<Self>,
    },
    Compare {
        left: String,
        comparison: super::Comparison,
        right: String,
    },
    Between {
        expression: String,
        begin: String,
        end: String,
    },
    IsNull {
        expression: String,
        negated: bool,
    },
    Raw {
        expression: String,
    },
}

/// The left relation contains preceding source bindings; this is not a type-based relationship suggestion.
#[derive(Clone, Debug, serde::Serialize)]
pub struct JoinContext {
    pub kind: super::JoinKind,
    pub left: Vec<String>,
    pub right: String,
}

impl Constructor {
    /// Present both relations and the checked join kind without interpreting a field's name as its type.
    #[must_use]
    pub fn join_context(&self, statement: usize, clause: usize) -> Option<JoinContext> {
        let owner = self.model.statements.get(statement)?;
        let section = owner
            .clauses
            .get(clause)
            .filter(|clause| clause.kind == ClauseKind::Joins)?;
        let header = &self.text()[section.range.start..section.body.start];
        let tokens = lex::tokens(header).ok()?;
        let kind = if tokens
            .iter()
            .any(|token| token.keyword(header, &["LEFT", "ЛЕВОЕ"]))
        {
            super::JoinKind::Left
        } else if tokens
            .iter()
            .any(|token| token.keyword(header, &["RIGHT", "ПРАВОЕ"]))
        {
            super::JoinKind::Right
        } else if tokens
            .iter()
            .any(|token| token.keyword(header, &["FULL", "ПОЛНОЕ"]))
        {
            super::JoinKind::Full
        } else {
            super::JoinKind::Inner
        };
        let sources = owner.source_bindings(self.text());
        Some(JoinContext {
            kind,
            left: sources
                .iter()
                .filter(|source| source.clause < clause)
                .map(|source| source.alias.clone())
                .collect(),
            right: sources
                .iter()
                .find(|source| source.clause == clause)?
                .alias
                .clone(),
        })
    }
    /// A projection never grants permission to rewrite an unknown source or infer a relationship.
    #[must_use]
    pub fn predicate(&self, statement: usize, clause: usize) -> Option<Predicate> {
        let owner = self.model.statements.get(statement)?;
        let section = owner.clauses.get(clause)?;
        if !matches!(
            section.kind,
            ClauseKind::Conditions | ClauseKind::Having | ClauseKind::Joins
        ) {
            return None;
        }
        let range = predicate_range(self.text(), section)?;
        if range.len() > 16_384 {
            return None;
        }
        Some(inspect(&without_comments(&self.text()[range]), 0, &mut 0))
    }
}

/// JOIN source spelling and comments preceding ON stay outside the predicate range.
fn predicate_range(input: &str, clause: &super::Clause) -> Option<std::ops::Range<usize>> {
    if clause.kind != ClauseKind::Joins {
        return Some(trim_range(input, clause.body.clone()));
    }
    let body = &input[clause.body.clone()];
    let token = lex::tokens(body)
        .ok()?
        .into_iter()
        .find(|token| token.depth == 0 && token.keyword(body, &["ON", "ПО"]))?;
    Some(trim_range(
        input,
        clause.body.start + token.range.end..clause.body.end,
    ))
}

/// Parse only boolean boundaries; strings, nested SELECTs and BETWEEN's AND cannot become separators.
fn inspect(input: &str, depth: usize, count: &mut usize) -> Predicate {
    *count += 1;
    let raw = || Predicate::Raw {
        expression: input.trim().to_owned(),
    };
    if depth >= 32 || *count > 512 {
        return raw();
    }
    let text = input.trim();
    let Ok(tokens) = lex::tokens(text) else {
        return raw();
    };
    let top: Vec<_> = tokens
        .iter()
        .filter(|token| token.depth == 0 && token.kind != Kind::Comment)
        .collect();
    if top.len() == 2 && top[0].kind == Kind::Symbol('(') && top[1].kind == Kind::Symbol(')') {
        return inspect(
            &text[top[0].range.end..top[1].range.start],
            depth + 1,
            count,
        );
    }
    for any in [true, false] {
        let mut between = false;
        let separators: Vec<_> = top
            .iter()
            .filter(|token| {
                if token.keyword(text, &["BETWEEN", "МЕЖДУ"]) {
                    between = true;
                    return false;
                }
                if token.keyword(text, &["AND", "И"]) && between {
                    between = false;
                    return false;
                }
                token.keyword(
                    text,
                    if any {
                        &["OR", "ИЛИ"]
                    } else {
                        &["AND", "И"]
                    },
                )
            })
            .collect();
        if separators.len() > 512 {
            return raw();
        }
        if !separators.is_empty() {
            let mut start = 0;
            let mut children = Vec::new();
            for token in separators {
                children.push(inspect(&text[start..token.range.start], depth + 1, count));
                start = token.range.end;
            }
            children.push(inspect(&text[start..], depth + 1, count));
            if *count > 512 {
                return raw();
            }
            return if any {
                Predicate::Any { children }
            } else {
                Predicate::All { children }
            };
        }
    }
    if let Some(token) = top
        .first()
        .filter(|token| token.keyword(text, &["NOT", "НЕ"]))
    {
        return Predicate::Not {
            child: Box::new(inspect(&text[token.range.end..], depth + 1, count)),
        };
    }
    atom(text, &top)
}

/// Leaf recognition keeps unsupported operators as text instead of inventing meaning.
fn atom(text: &str, top: &[&lex::Token]) -> Predicate {
    let raw = || Predicate::Raw {
        expression: text.to_owned(),
    };
    if let Some((index, token)) = top
        .iter()
        .enumerate()
        .find(|(_, token)| token.keyword(text, &["BETWEEN", "МЕЖДУ"]))
        && let Some(and) = top[index + 1..]
            .iter()
            .find(|token| token.keyword(text, &["AND", "И"]))
    {
        return Predicate::Between {
            expression: text[..token.range.start].trim().into(),
            begin: text[token.range.end..and.range.start].trim().into(),
            end: text[and.range.end..].trim().into(),
        };
    }
    if let Some((index, token)) = top
        .iter()
        .enumerate()
        .find(|(_, token)| token.keyword(text, &["IS", "ЕСТЬ"]))
    {
        let tail = &top[index + 1..];
        let negated = tail
            .first()
            .is_some_and(|token| token.keyword(text, &["NOT", "НЕ"]));
        if tail.len() == usize::from(negated) + 1
            && tail
                .last()
                .is_some_and(|token| token.keyword(text, &["NULL"]))
        {
            return Predicate::IsNull {
                expression: text[..token.range.start].trim().into(),
                negated,
            };
        }
    }
    visual::comparison(text).map_or_else(raw, |(left, comparison, right)| Predicate::Compare {
        left,
        comparison,
        right,
    })
}

/// Whole predicates are replaced together, so changing their grouping cannot leave half a WHERE/HAVING.
pub(super) fn plan(value: &Constructor, action: &Action) -> Result<Option<EditPlan>, Error> {
    if let Action::SetJoinKind { selection, join } = action {
        let owner = value
            .model
            .statements
            .get(selection.statement)
            .ok_or(Error::InvalidSelection)?;
        let section = owner
            .clauses
            .get(selection.clause)
            .filter(|clause| clause.kind == ClauseKind::Joins && selection.item == 0)
            .ok_or(Error::InvalidSelection)?;
        if owner.unavailable.is_some() {
            return Err(Error::Unsupported);
        }
        return EditPlan::from_replacements(
            value.text(),
            visual::join_header(value.text(), section, *join, owner.russian)?,
        )
        .map(Some)
        .map_err(|_| Error::InvalidExpression);
    }
    let Action::SetPredicate {
        statement,
        clause,
        selection,
        predicate,
    } = action
    else {
        return Ok(None);
    };
    if !matches!(
        clause,
        ClauseKind::Conditions | ClauseKind::Having | ClauseKind::Joins
    ) {
        return Err(Error::InvalidSelection);
    }
    let owner = value
        .model
        .statements
        .get(*statement)
        .ok_or(Error::InvalidSelection)?;
    if owner.unavailable.is_some() {
        return Err(Error::Unsupported);
    }
    let section = if *clause == ClauseKind::Joins {
        let selected = selection.ok_or(Error::InvalidSelection)?;
        if selected.statement != *statement || selected.item != 0 {
            return Err(Error::InvalidSelection);
        }
        owner
            .clauses
            .get(selected.clause)
            .filter(|section| section.kind == *clause)
    } else {
        if selection.is_some() {
            return Err(Error::InvalidSelection);
        }
        owner.clauses.iter().find(|section| section.kind == *clause)
    };
    let input = value.text();
    let text = render(predicate, owner.russian, editing::newline(input), 0, &mut 0)?;
    if *clause == ClauseKind::Conditions && outer_aggregate(&text)? {
        return Err(Error::InvalidExpression);
    }
    let edits = if let Some(section) = section {
        let range = predicate_range(input, section).ok_or(Error::Unsupported)?;
        if inspect(&without_comments(&input[range.clone()]), 0, &mut 0) == *predicate {
            return Ok(Some(
                EditPlan::from_replacements(input, vec![]).map_err(|_| Error::InvalidExpression)?,
            ));
        }
        let comments = section
            .comments
            .iter()
            .filter(|comment| comment.start >= range.start && comment.end <= range.end)
            .cloned()
            .collect();
        editing::replace(input, &Item { range, comments }, &text)?
    } else {
        if *clause == ClauseKind::Joins {
            return Err(Error::InvalidSelection);
        }
        editing::append(input, owner, *clause, &text, super::JoinKind::Inner)?
    };
    let output = EditPlan::from_replacements(input, edits).map_err(|_| Error::InvalidExpression)?;
    if output.output().len() > super::super::MAX_SNAPSHOT_BYTES {
        return Err(Error::TooLarge);
    }
    Ok(Some(output))
}

/// Moving a predicate changes both sections on a private snapshot and transfers its comment bytes once.
pub(super) fn move_stage(
    value: &Constructor,
    selection: super::Selection,
    target: ClauseKind,
) -> Result<super::items::Staged, Error> {
    let owner = value
        .model
        .statements
        .get(selection.statement)
        .ok_or(Error::InvalidSelection)?;
    let section = owner
        .clauses
        .get(selection.clause)
        .ok_or(Error::InvalidSelection)?;
    if !matches!(target, ClauseKind::Conditions | ClauseKind::Having)
        || !matches!(section.kind, ClauseKind::Conditions | ClauseKind::Having)
        || target == section.kind
    {
        return Err(Error::InvalidSelection);
    }
    let item = section
        .items
        .get(selection.item)
        .ok_or(Error::InvalidSelection)?;
    let mut text = value.text()[item.range.clone()].to_owned();
    text.push_str(editing::newline(value.text()));
    let mut staged = value.clone();
    staged.revision = 1;
    staged.apply(
        1,
        &Action::Remove {
            selection,
            comments: Some(super::CommentDecision::Discard),
        },
    )?;
    let old = staged.model.statements[selection.statement]
        .clauses
        .iter()
        .position(|clause| clause.kind == target)
        .and_then(|index| staged.predicate(selection.statement, index));
    let predicate = Predicate::Raw { expression: text };
    let predicate = old.map_or_else(
        || predicate.clone(),
        |old| Predicate::All {
            children: vec![old, predicate.clone()],
        },
    );
    staged.apply(
        staged.revision,
        &Action::SetPredicate {
            statement: selection.statement,
            clause: target,
            selection: None,
            predicate,
        },
    )?;
    Ok(super::items::Staged {
        value: staged,
        focus: Vec::new(),
    })
}

/// Lexical aggregate checks ignore a nested SELECT's complete scope and do not infer field types.
fn outer_aggregate(input: &str) -> Result<bool, Error> {
    let tokens = lex::tokens(input)?;
    let mut query_depth = None;
    for (index, token) in tokens.iter().enumerate() {
        if query_depth.is_some_and(|depth| token.depth < depth) {
            query_depth = None;
        }
        if token.keyword(input, &["SELECT", "ВЫБРАТЬ"]) {
            query_depth = Some(token.depth);
        }
        if query_depth.is_none()
            && token.keyword(
                input,
                &[
                    "SUM",
                    "СУММА",
                    "COUNT",
                    "КОЛИЧЕСТВО",
                    "MIN",
                    "МИНИМУМ",
                    "MAX",
                    "МАКСИМУМ",
                    "AVG",
                    "СРЕДНЕЕ",
                ],
            )
            && tokens
                .get(index + 1)
                .is_some_and(|next| next.kind == Kind::Symbol('('))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Generated groups are explicitly parenthesized; no adapter assembles localized boolean syntax.
fn render(
    value: &Predicate,
    russian: bool,
    newline: &str,
    depth: usize,
    count: &mut usize,
) -> Result<String, Error> {
    *count += 1;
    if depth > 32 || *count > 512 {
        return Err(Error::TooLarge);
    }
    let text = match value {
        Predicate::All { children } | Predicate::Any { children } => {
            if children.is_empty() {
                return Err(Error::InvalidExpression);
            }
            let separator = match (matches!(value, Predicate::Any { .. }), russian) {
                (true, true) => " ИЛИ ",
                (true, false) => " OR ",
                (false, true) => " И ",
                (false, false) => " AND ",
            };
            let children = children
                .iter()
                .map(|child| render(child, russian, newline, depth + 1, count))
                .collect::<Result<Vec<_>, _>>()?;
            format!("({})", children.join(separator))
        }
        Predicate::Not { child } => format!(
            "{} ({})",
            if russian { "НЕ" } else { "NOT" },
            render(child, russian, newline, depth + 1, count)?
        ),
        Predicate::Compare {
            left,
            comparison,
            right,
        } => {
            operand(left)?;
            operand(right)?;
            format!("{left} {} {right}", comparison.keyword(russian))
        }
        Predicate::Between {
            expression,
            begin,
            end,
        } => {
            operand(expression)?;
            operand(begin)?;
            operand(end)?;
            format!(
                "{expression} {} {begin} {} {end}",
                if russian { "МЕЖДУ" } else { "BETWEEN" },
                if russian { "И" } else { "AND" }
            )
        }
        Predicate::IsNull {
            expression,
            negated,
        } => {
            operand(expression)?;
            format!(
                "{expression} {} {}NULL",
                if russian { "ЕСТЬ" } else { "IS" },
                if *negated {
                    if russian { "НЕ " } else { "NOT " }
                } else {
                    ""
                }
            )
        }
        Predicate::Raw { expression } => {
            editing::fragment(expression)?;
            // A raw leaf is one boolean child even when it contains OR; a trailing
            // line comment must not consume the closing delimiter or the next child.
            let line_comment = lex::tokens(expression)?.last().is_some_and(|token| {
                token.kind == Kind::Comment
                    && expression[token.range.clone()].starts_with("//")
                    && !expression[token.range.end..].contains(['\r', '\n'])
            });
            format!("({expression}{})", if line_comment { newline } else { "" })
        }
    };
    if text.len() > 16_384 {
        return Err(Error::TooLarge);
    }
    editing::fragment(&text)?;
    Ok(text)
}

/// A boolean operand cannot smuggle a second outer predicate; a subquery's own WHERE/aggregate stays nested.
fn operand(input: &str) -> Result<(), Error> {
    editing::fragment(input)?;
    if lex::tokens(input)?.iter().any(|token| {
        token.depth == 0 && token.keyword(input, &["AND", "И", "OR", "ИЛИ", "BETWEEN", "МЕЖДУ"])
    }) {
        return Err(Error::InvalidExpression);
    }
    Ok(())
}

/// Whitespace outside the expression remains an unchanged separator after WHERE/ON.
fn trim_range(input: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let text = &input[range.clone()];
    range.start + text.len() - text.trim_start().len()..range.start + text.trim_end().len()
}

/// Comments keep their source associations instead of being duplicated inside a projected leaf.
fn without_comments(input: &str) -> String {
    let Ok(tokens) = lex::tokens(input) else {
        return input.to_owned();
    };
    let mut text = String::new();
    let mut start = 0;
    for token in tokens.iter().filter(|token| token.kind == Kind::Comment) {
        text.push_str(&input[start..token.range.start]);
        text.push(' ');
        start = token.range.end;
    }
    text.push_str(&input[start..]);
    text
}

/// Earlier adapters retain the opaque behavior for syntax they could not edit safely.
pub(super) fn complex(input: &str, statement: &super::Statement) -> bool {
    statement
        .clauses
        .iter()
        .filter(|clause| {
            matches!(
                clause.kind,
                ClauseKind::Conditions | ClauseKind::Having | ClauseKind::Joins
            )
        })
        .any(|clause| {
            predicate_range(input, clause).is_some_and(|range| {
                let text = &input[range];
                lex::tokens(text).is_ok_and(|tokens| {
                    tokens.iter().any(|token| {
                        token.depth == 0 && token.keyword(text, &["OR", "ИЛИ", "BETWEEN", "МЕЖДУ"])
                    })
                })
            })
        })
}
