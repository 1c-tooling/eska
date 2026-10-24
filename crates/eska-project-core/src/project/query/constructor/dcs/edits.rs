//! Typed requests can replace only a proven DCS slot; the ordinary query and unknown extensions retain their bytes.

use super::{DcsKind, DcsNode, DcsStatement, DcsValue, scan};
use crate::project::{
    metadata_edit::{EditPlan, Replacement},
    query::constructor::{
        Action, ClauseKind, CommentDecision, Error, Expression, Model, editing,
        lex::{self, Kind},
        visual,
    },
};

/// One intention produces one bounded source replacement and is reparsed before the constructor publishes it.
pub(in super::super) fn plan(
    input: &str,
    original: &Model,
    action: &Action,
) -> Result<Option<EditPlan>, Error> {
    let Action::SetDcs {
        statement,
        node,
        target,
        values,
        enabled,
        comments,
    } = action
    else {
        return Ok(None);
    };
    let model = super::inspect(input, original)?;
    let owner = model
        .statements
        .get(*statement)
        .ok_or(Error::InvalidSelection)?;
    if values.len() > 128
        || serde_json::to_vec(values)
            .map_err(|_| Error::InvalidExpression)?
            .len()
            > 16_384
    {
        return Err(Error::TooLarge);
    }
    let selected = node
        .map(|index| owner.nodes.get(index).ok_or(Error::InvalidSelection))
        .transpose()?;
    if selected.is_none() && !enabled {
        return Err(Error::InvalidSelection);
    }
    let range = if let Some(selected) = selected {
        if selected.kind != *target || !selected.editable {
            return Err(Error::Unsupported);
        }
        if selected.values == *values && selected.enabled == *enabled {
            return Ok(Some(
                EditPlan::from_replacements(input, Vec::new())
                    .map_err(|_| Error::InvalidExpression)?,
            ));
        }
        selected.range.clone()
    } else {
        let at = match target {
            DcsKind::Select if owner.add_select => owner.select,
            DcsKind::Where if owner.add_where => owner.condition,
            _ => None,
        }
        .ok_or(Error::Unsupported)?;
        at..at
    };
    let mut text = render(input, owner, selected, *target, values, *enabled)?;
    if *target != DcsKind::OptionalSource {
        text = comments_before(input, selected, text, *comments)?;
    }
    if selected.is_none() {
        let newline = editing::newline(input);
        text = format!("{newline}{text}{newline}");
    }
    text = separate(input, &range, text);
    let plan = EditPlan::from_replacements(input, vec![Replacement { range, text }])
        .map_err(|_| Error::InvalidExpression)?;
    let after = Model::parse(plan.output())?;
    if after.statements.len() != original.statements.len()
        || after.scopes.len() != original.scopes.len()
    {
        return Err(Error::InvalidExpression);
    }
    let next = super::inspect(plan.output(), &after)?;
    let next = next
        .statements
        .get(*statement)
        .ok_or(Error::InvalidExpression)?;
    if *enabled
        && !next.nodes.iter().any(|node| {
            node.kind == *target && node.enabled && node.editable && node.values == *values
        })
    {
        return Err(Error::InvalidExpression);
    }
    Ok(Some(plan))
}

/// Removing a brace that separated two words must not merge an ordinary field with the following SQL keyword.
fn separate(input: &str, range: &std::ops::Range<usize>, mut text: String) -> String {
    let word =
        |value: Option<char>| value.is_some_and(|value| value.is_alphanumeric() || value == '_');
    let left = input[..range.start].chars().next_back();
    let right = input[range.end..].chars().next();
    if text.is_empty() {
        if word(left) && word(right) {
            text.push(' ');
        }
    } else {
        if word(left) && word(text.chars().next()) {
            text.insert(0, ' ');
        }
        if word(text.chars().next_back()) && word(right) {
            text.push(' ');
        }
    }
    text
}

/// Toggling a source preserves its complete body; lists and parameters use the core's bilingual formatter.
fn render(
    input: &str,
    owner: &DcsStatement,
    selected: Option<&DcsNode>,
    kind: DcsKind,
    values: &[DcsValue],
    enabled: bool,
) -> Result<String, Error> {
    if kind == DcsKind::OptionalSource {
        let selected = selected.ok_or(Error::InvalidSelection)?;
        if !values.is_empty() {
            return Err(Error::InvalidExpression);
        }
        let text = &input[selected.range.clone()];
        return Ok(if enabled {
            format!("{{{text}}}")
        } else {
            text[1..text.len() - 1].to_owned()
        });
    }
    if kind == DcsKind::Parameter {
        let [DcsValue::Parameter { name }] = values else {
            return Err(Error::InvalidExpression);
        };
        visual::identifier(name)?;
        return Ok(if enabled {
            format!("{{&{name}}}")
        } else {
            format!("&{name}")
        });
    }
    if !matches!(
        kind,
        DcsKind::Select | DcsKind::Where | DcsKind::VirtualFilter
    ) {
        return Err(Error::Unsupported);
    }
    if !enabled {
        return Ok(String::new());
    }
    if values.is_empty()
        || (kind == DcsKind::Select && !scan::valid_columns(values, &owner.columns))
    {
        return Err(Error::InvalidExpression);
    }
    let mut parts = Vec::new();
    for value in values {
        parts.push(value_text(value, kind, owner.russian)?);
    }
    let keyword = match kind {
        DcsKind::Select => format!("{} ", ClauseKind::Fields.keyword(owner.russian)),
        DcsKind::Where => format!("{} ", ClauseKind::Conditions.keyword(owner.russian)),
        _ => String::new(),
    };
    Ok(format!("{{{keyword}{}}}", parts.join(", ")))
}

/// Field paths and binary predicates cannot escape their comma-separated extension list.
fn value_text(value: &DcsValue, kind: DcsKind, russian: bool) -> Result<String, Error> {
    match value {
        DcsValue::Field {
            expression,
            children,
            alias,
        } => {
            scan::path(expression)?;
            if !alias.is_empty() {
                visual::identifier(alias)?;
            }
            Ok(format!(
                "{}{}{}",
                expression,
                if *children { ".*" } else { "" },
                if alias.is_empty() {
                    String::new()
                } else {
                    format!(" {} {alias}", if russian { "КАК" } else { "AS" })
                }
            ))
        }
        DcsValue::Condition {
            left,
            comparison,
            right,
        } if kind != DcsKind::Select => {
            editing::fragment(left)?;
            editing::fragment(right)?;
            Expression::Condition {
                left: left.clone(),
                comparison: *comparison,
                right: right.clone(),
            }
            .text(russian)
        }
        _ => Err(Error::InvalidExpression),
    }
}

/// Replacing a commented extension is explicit; retained comments stay immediately before the replacement boundary.
fn comments_before(
    input: &str,
    selected: Option<&DcsNode>,
    text: String,
    decision: Option<CommentDecision>,
) -> Result<String, Error> {
    let Some(selected) = selected.filter(|node| node.comments > 0) else {
        return Ok(text);
    };
    match decision {
        None => Err(Error::CommentsRequireDecision),
        Some(CommentDecision::Discard) => Ok(text),
        Some(CommentDecision::Keep) => {
            let original = &input[selected.range.clone()];
            let mut kept = String::new();
            for token in lex::tokens(original)?
                .iter()
                .filter(|token| token.kind == Kind::Comment)
            {
                kept.push_str(&original[token.range.clone()]);
                kept.push_str(editing::newline(input));
            }
            kept.push_str(&text);
            Ok(kept)
        }
    }
}
