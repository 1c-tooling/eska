//! Virtual-table availability and fields follow real owner metadata rather than a client-side schema.

mod calculation;
mod columns;

use super::{
    QueryFieldChoice, QuerySessionError, QueryTableChoice,
    fields::{self, property},
    schema::Sources,
};
use crate::project::{
    metadata_model::MetadataKind,
    metadata_parser::{ParsedDescriptor, ParsedObject},
    query::virtual_tables::VirtualTable,
};
use roxmltree::{Document, Node};
use std::collections::BTreeMap;

/// Exclude virtual tables disabled by a register's actual Designer properties.
pub(super) fn available(kind: VirtualTable, category: MetadataKind, owner: Node<'_, '_>) -> bool {
    match kind {
        VirtualTable::SliceFirst | VirtualTable::SliceLast => {
            property(owner, "InformationRegisterPeriodicity")
                .is_some_and(|value| value != "Nonperiodical")
        }
        VirtualTable::Balance | VirtualTable::BalanceAndTurnovers
            if category == MetadataKind::AccumulationRegister =>
        {
            property(owner, "RegisterType") == Some("Balance")
        }
        VirtualTable::DrCrTurnovers => enabled(owner, "Correspondence"),
        VirtualTable::AdjustedEffectivePeriod => enabled(owner, "ActionPeriodUse"),
        VirtualTable::ScheduleData => {
            property(owner, "Schedule").is_some_and(|value| !value.is_empty())
        }
        VirtualTable::Base => enabled(owner, "BasePeriodUse"),
        _ => true,
    }
}

/// Expand one register into its supported virtual tables only when its descriptor is requested.
pub(super) fn choices(
    sources: &Sources<'_>,
    table: &QueryTableChoice,
    owner: Node<'_, '_>,
    tables: &mut BTreeMap<String, QueryTableChoice>,
) -> Result<(), QuerySessionError> {
    for &kind in VirtualTable::for_category(table.category) {
        if kind == VirtualTable::Base || !available(kind, table.category, owner) {
            continue;
        }
        let (russian, english) = kind.names();
        let value = QueryTableChoice {
            name: russian.to_owned(),
            category: table.category,
            russian: format!("{}.{russian}", table.russian),
            english: format!("{}.{english}", table.english),
            alias: format!("{}{russian}", table.name),
            virtual_table: Some(kind),
        };
        tables.entry(value.english.to_lowercase()).or_insert(value);
    }
    if table.category == MetadataKind::CalculationRegister {
        calculation::choices(sources, table, owner, tables)?;
    }
    Ok(())
}

/// A calculated base table must name a register related through the assigned calculation-type chart.
pub(super) fn selected_available(
    sources: &Sources<'_>,
    kind: VirtualTable,
    category: MetadataKind,
    owner: Node<'_, '_>,
    requested: &str,
) -> Result<bool, QuerySessionError> {
    if !available(kind, category, owner) {
        return Ok(false);
    }
    if kind == VirtualTable::Base {
        return Ok(calculation::base_register(sources, owner, requested)?.is_some());
    }
    Ok(true)
}

/// Virtual rows derive from the descriptor's own dimensions/resources and the platform's table-specific column rules.
pub(super) fn fields(
    sources: &Sources<'_>,
    table: &QueryTableChoice,
    document: &Document<'_>,
    parsed: &ParsedDescriptor,
    owner: &ParsedObject,
    requested: &str,
    output: &mut BTreeMap<String, QueryFieldChoice>,
) -> Result<(), QuerySessionError> {
    let (_, kind) = VirtualTable::resolve(requested)
        .ok_or(crate::project::query::QueryError::InvalidPosition)?;
    let node = fields::object_node(document, owner)?;
    if matches!(
        kind,
        VirtualTable::SliceFirst
            | VirtualTable::SliceLast
            | VirtualTable::RecordsWithExtDimensions
            | VirtualTable::AdjustedEffectivePeriod
            | VirtualTable::ScheduleData
            | VirtualTable::Base
    ) {
        fields::append(document, parsed, owner, table.category, output)?;
    }
    let context = columns::Context {
        sources,
        document,
        parsed,
        owner,
        node,
        category: table.category,
        kind,
    };
    context.append(output)?;
    if table.category == MetadataKind::CalculationRegister {
        calculation::fields(sources, node, kind, requested, output)?;
    }
    Ok(())
}

/// XML Schema boolean word and numeric forms have identical meaning.
fn enabled(owner: Node<'_, '_>, name: &str) -> bool {
    matches!(property(owner, name), Some("true" | "1"))
}
