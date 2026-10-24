//! Calculation virtual tables follow the explicitly assigned schedule and base calculation-type charts.

use super::super::fields::{insert, property};
use super::{
    ProjectSession, QueryFieldChoice, QuerySessionError, QueryTableChoice, enabled, related,
};
use crate::project::{
    metadata_model::MetadataKind,
    query::{QueryError, virtual_tables::VirtualTable},
};
use roxmltree::{Document, Node};
use std::collections::{BTreeMap, BTreeSet};

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

/// Base tables are named after actual registers assigned to the owner's declared base charts.
pub(super) fn choices(
    project: &ProjectSession,
    table: &QueryTableChoice,
    owner: Node<'_, '_>,
    output: &mut BTreeMap<String, QueryTableChoice>,
) -> Result<(), QuerySessionError> {
    for (name, _) in bases(project, owner)? {
        let russian = format!("База{name}");
        let english = format!("Base{name}");
        let value = QueryTableChoice {
            name: russian.clone(),
            category: table.category,
            russian: format!("{}.{russian}", table.russian),
            english: format!("{}.{english}", table.english),
            alias: format!("{}{russian}", table.name),
            virtual_table: Some(VirtualTable::Base),
        };
        output.insert(value.english.to_lowercase(), value);
    }
    Ok(())
}

/// The requested suffix cannot select an unrelated register, even if it exists in the same project.
pub(super) fn base_register(
    project: &ProjectSession,
    owner: Node<'_, '_>,
    requested: &str,
) -> Result<Option<String>, QuerySessionError> {
    let suffix = requested
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_lowercase();
    Ok(bases(project, owner)?
        .into_iter()
        .find(|(name, _)| {
            [format!("base{name}"), format!("база{name}")]
                .iter()
                .any(|value| value.to_lowercase() == suffix)
        })
        .map(|(_, text)| text))
}

/// Only calculation descriptors need inspection, and only after expanding a register that declares base-period support.
fn bases(
    project: &ProjectSession,
    owner: Node<'_, '_>,
) -> Result<Vec<(String, String)>, QuerySessionError> {
    if !enabled(owner, "BasePeriodUse") {
        return Ok(Vec::new());
    }
    let Some(chart) = property(owner, "ChartOfCalculationTypes") else {
        return Ok(Vec::new());
    };
    let Some(text) = related::descriptor(project, MetadataKind::ChartOfCalculationTypes, chart)?
    else {
        return Ok(Vec::new());
    };
    let document = Document::parse(&text).map_err(|_| QueryError::InvalidXml)?;
    let Some(node) = object(&document, "ChartOfCalculationTypes") else {
        return Err(QueryError::InvalidXml.into());
    };
    if property(node, "DependenceOnCalculationTypes").is_none_or(|value| value == "DontUse") {
        return Ok(Vec::new());
    }
    let charts: BTreeSet<_> = node
        .children()
        .find(|node| node.has_tag_name((MD, "Properties")))
        .and_then(|node| {
            node.children()
                .find(|node| node.has_tag_name((MD, "BaseCalculationTypes")))
        })
        .into_iter()
        .flat_map(|node| node.descendants())
        .filter_map(|node| node.text())
        .map(str::trim)
        .filter(|value| value.starts_with("ChartOfCalculationTypes."))
        .map(str::to_owned)
        .collect();
    let mut result = Vec::new();
    for candidate in project
        .objects
        .values()
        .filter(|value| value.kind == MetadataKind::CalculationRegister)
    {
        let Some(text) =
            related::descriptor(project, MetadataKind::CalculationRegister, &candidate.name)?
        else {
            continue;
        };
        let document = Document::parse(&text).map_err(|_| QueryError::InvalidXml)?;
        let Some(node) = object(&document, "CalculationRegister") else {
            return Err(QueryError::InvalidXml.into());
        };
        if property(node, "ChartOfCalculationTypes").is_some_and(|value| charts.contains(value)) {
            result.push((candidate.name.clone(), text));
        }
    }
    Ok(result)
}

/// Standard owner columns and resource-derived columns share the same backend schema for every IDE.
pub(super) fn fields(
    project: &ProjectSession,
    owner: Node<'_, '_>,
    kind: VirtualTable,
    requested: &str,
    output: &mut BTreeMap<String, QueryFieldChoice>,
) -> Result<(), QuerySessionError> {
    for (russian, english) in [
        ("ВидРасчета", "CalculationType"),
        ("Сторно", "ReversingEntry"),
    ] {
        insert(output, russian, english);
    }
    for (setting, columns) in [
        (
            "ActionPeriodUse",
            &[
                ("ПериодДействия", "ActionPeriod"),
                ("ПериодДействияНачало", "BegOfActionPeriod"),
                ("ПериодДействияКонец", "EndOfActionPeriod"),
            ][..],
        ),
        (
            "BasePeriodUse",
            &[
                ("БазовыйПериодНачало", "BegOfBasePeriod"),
                ("БазовыйПериодКонец", "EndOfBasePeriod"),
            ][..],
        ),
    ] {
        if enabled(owner, setting) {
            for (ru, en) in columns {
                insert(output, ru, en);
            }
        }
    }
    if kind == VirtualTable::ScheduleData {
        if let Some(schedule) = property(owner, "Schedule")
            && let Some(text) =
                related::descriptor(project, MetadataKind::InformationRegister, schedule)?
        {
            let document = Document::parse(&text).map_err(|_| QueryError::InvalidXml)?;
            let node = object(&document, "InformationRegister").ok_or(QueryError::InvalidXml)?;
            for name in names(node, "Resource") {
                for (ru, en, available) in [
                    (
                        "БазовыйПериод",
                        "BasePeriod",
                        enabled(owner, "BasePeriodUse"),
                    ),
                    (
                        "ПериодДействия",
                        "ActivePeriod",
                        enabled(owner, "ActionPeriodUse"),
                    ),
                    ("ПериодРегистрации", "RegistrationPeriod", true),
                    (
                        "ФактическийПериодДействия",
                        "ActualActionPeriod",
                        enabled(owner, "ActionPeriodUse"),
                    ),
                ] {
                    if available {
                        insert(output, &format!("{name}{ru}"), &format!("{name}{en}"));
                    }
                }
            }
        }
    } else if kind == VirtualTable::Base
        && let Some(text) = base_register(project, owner, requested)?
    {
        let document = Document::parse(&text).map_err(|_| QueryError::InvalidXml)?;
        let node = object(&document, "CalculationRegister").ok_or(QueryError::InvalidXml)?;
        for (tag, ru, en) in [
            ("Dimension", "Разрез", "Dimension"),
            ("Resource", "База", "Base"),
        ] {
            for name in names(node, tag) {
                insert(output, &format!("{name}{ru}"), &format!("{name}{en}"));
            }
        }
    }
    Ok(())
}

/// Namespace-aware roots avoid matching a property with the same local name.
fn object<'a>(document: &'a Document<'_>, kind: &str) -> Option<Node<'a, 'a>> {
    document
        .root_element()
        .children()
        .find(|node| node.has_tag_name((MD, kind)))
}

/// Only the related register's direct resources/dimensions contribute virtual column names.
fn names<'a>(node: Node<'a, 'a>, tag: &str) -> Vec<&'a str> {
    node.children()
        .find(|node| node.has_tag_name((MD, "ChildObjects")))
        .into_iter()
        .flat_map(|node| node.children())
        .filter(|node| node.has_tag_name((MD, tag)))
        .filter_map(|node| property(node, "Name"))
        .collect()
}
