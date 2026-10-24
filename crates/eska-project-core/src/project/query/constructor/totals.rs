//! Totals syntax is domain data, including hierarchy and filling missing date periods.

use super::{Error, Expression, lex, visual};

/// OVERALL is idempotent and always precedes ordinary groupings.
pub(super) fn plan(
    input: &str,
    model: &super::Model,
    action: &super::Action,
) -> Result<Option<crate::project::metadata_edit::EditPlan>, Error> {
    let super::Action::Compose {
        statement,
        selection: None,
        expression: Expression::Overall,
    } = action
    else {
        return Ok(None);
    };
    let owner = model
        .statements
        .get(*statement)
        .ok_or(Error::InvalidSelection)?;
    if let Some(error) = owner.append_unavailable(super::ClauseKind::TotalGroups) {
        return Err(error);
    }
    let Some(groups) = owner
        .clauses
        .iter()
        .find(|clause| clause.kind == super::ClauseKind::TotalGroups)
    else {
        return Ok(None);
    };
    let edits = if groups
        .items
        .iter()
        .any(|item| matches!(item.visual(input, groups), Some(Expression::Overall)))
    {
        Vec::new()
    } else {
        let first = groups.items.first().ok_or(Error::InvalidExpression)?;
        vec![crate::project::metadata_edit::Replacement {
            range: first.range.start..first.range.start,
            text: format!(
                "{},{}",
                if owner.russian {
                    "ОБЩИЕ"
                } else {
                    "OVERALL"
                },
                super::editing::newline(input)
            ),
        }]
    };
    Ok(Some(
        crate::project::metadata_edit::EditPlan::from_replacements(input, edits)
            .map_err(|_| Error::InvalidExpression)?,
    ))
}

/// Hierarchy identifiers remain stable across both query languages.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Hierarchy {
    #[default]
    None,
    Hierarchy,
    OnlyHierarchy,
}

/// PERIODS accepts calendar units, unlike virtual-table periodicity that also accepts Recorder and Auto.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PeriodUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    TenDays,
    Month,
    Quarter,
    HalfYear,
    Year,
}

impl PeriodUnit {
    /// A complete reviewed set prevents query-keyword generation in an IDE.
    pub const ALL: &'static [Self] = &[
        Self::Second,
        Self::Minute,
        Self::Hour,
        Self::Day,
        Self::Week,
        Self::TenDays,
        Self::Month,
        Self::Quarter,
        Self::HalfYear,
        Self::Year,
    ];

    /// These pairs are query identifiers and may differ from the editor's interface language.
    #[must_use]
    pub const fn names(self) -> (&'static str, &'static str) {
        match self {
            Self::Second => ("СЕКУНДА", "SECOND"),
            Self::Minute => ("МИНУТА", "MINUTE"),
            Self::Hour => ("ЧАС", "HOUR"),
            Self::Day => ("ДЕНЬ", "DAY"),
            Self::Week => ("НЕДЕЛЯ", "WEEK"),
            Self::TenDays => ("ДЕКАДА", "TENDAYS"),
            Self::Month => ("МЕСЯЦ", "MONTH"),
            Self::Quarter => ("КВАРТАЛ", "QUARTER"),
            Self::HalfYear => ("ПОЛУГОДИЕ", "HALFYEAR"),
            Self::Year => ("ГОД", "YEAR"),
        }
    }
}

/// Date boundaries are query expressions; missing positions retain their positional meaning.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PeriodRange {
    pub unit: PeriodUnit,
    pub begin: Option<String>,
    pub end: Option<String>,
}

/// Format one total grouping without accepting a second grouping or a package operator from an input value.
pub(super) fn group(
    expression: &str,
    alias: &str,
    hierarchy: Hierarchy,
    period: Option<&PeriodRange>,
    russian: bool,
) -> Result<String, Error> {
    if expression.trim().is_empty() || (period.is_some() && hierarchy != Hierarchy::None) {
        return Err(Error::InvalidExpression);
    }
    let mut text = expression.to_owned();
    match hierarchy {
        Hierarchy::None => (),
        Hierarchy::Hierarchy => text.push_str(if russian {
            " ИЕРАРХИЯ"
        } else {
            " HIERARCHY"
        }),
        Hierarchy::OnlyHierarchy => text.push_str(if russian {
            " ТОЛЬКО ИЕРАРХИЯ"
        } else {
            " ONLY HIERARCHY"
        }),
    }
    if let Some(period) = period {
        let (ru, en) = period.unit.names();
        let mut values = vec![if russian { ru } else { en }.to_owned()];
        for boundary in [&period.begin, &period.end].into_iter().flatten() {
            super::super::virtual_tables::validate_argument(boundary)?;
        }
        if period.begin.is_some() || period.end.is_some() {
            values.push(period.begin.clone().unwrap_or_default());
        }
        if let Some(end) = &period.end {
            values.push(end.clone());
        }
        text.push_str(if russian {
            " ПЕРИОДАМИ("
        } else {
            " PERIODS("
        });
        text.push_str(&values.join(", "));
        text.push(')');
    }
    if !alias.is_empty() {
        visual::identifier(alias)?;
        text.push_str(if russian { " КАК " } else { " AS " });
        text.push_str(alias);
    }
    Ok(text)
}

/// Recognize only complete terminal grouping modifiers; unusual expressions remain available in raw mode.
pub(super) fn project(input: &str) -> Option<Expression> {
    let tokens = lex::tokens(input).ok()?;
    if tokens.len() == 1 && tokens[0].keyword(input, &["OVERALL", "ОБЩИЕ"]) {
        return Some(Expression::Overall);
    }
    let (value, alias) = visual::split_alias(input)?;
    let tokens = lex::tokens(&value).ok()?;
    let mut end = value.len();
    let mut period = None;
    let mut hierarchy = Hierarchy::None;
    if let Some(token) = tokens
        .iter()
        .find(|token| token.depth == 0 && token.keyword(&value, &["PERIODS", "ПЕРИОДАМИ"]))
    {
        let (_, parameters) =
            super::super::virtual_tables::split_arguments(value[token.range.start..].trim())?;
        let unit = *PeriodUnit::ALL.iter().find(|unit| {
            let (ru, en) = unit.names();
            parameters
                .first()
                .is_some_and(|value| value.to_uppercase() == ru || value.to_uppercase() == en)
        })?;
        if parameters.len() > 3 {
            return None;
        }
        period = Some(PeriodRange {
            unit,
            begin: parameters.get(1).cloned(),
            end: parameters.get(2).cloned(),
        });
        end = token.range.start;
    } else if let Some(last) = tokens
        .last()
        .filter(|last| last.depth == 0 && last.keyword(&value, &["HIERARCHY", "ИЕРАРХИЯ"]))
    {
        end = last.range.start;
        hierarchy = Hierarchy::Hierarchy;
        if let Some(only) = tokens
            .get(tokens.len().checked_sub(2)?)
            .filter(|token| token.keyword(&value, &["ONLY", "ТОЛЬКО"]))
        {
            end = only.range.start;
            hierarchy = Hierarchy::OnlyHierarchy;
        }
    }
    let expression = value[..end].trim().to_owned();
    if expression.is_empty() {
        return None;
    }
    Some(Expression::TotalGroup {
        expression,
        alias,
        hierarchy,
        period,
    })
}
