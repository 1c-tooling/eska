//! Virtual-table availability and fields follow real owner metadata rather than a client-side schema.

mod columns;

use super::{
    ProjectSession, QueryFieldChoice, QuerySessionError, QueryTableChoice,
    fields::{self, property},
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
        VirtualTable::Base => false,
        _ => true,
    }
}

/// Expand one register into its supported virtual tables only when its descriptor is requested.
pub(super) fn choices(
    table: &QueryTableChoice,
    owner: Node<'_, '_>,
    tables: &mut BTreeMap<String, QueryTableChoice>,
) {
    for &kind in VirtualTable::for_category(table.category) {
        if !available(kind, table.category, owner) {
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
}

/// Virtual rows derive from the descriptor's own dimensions/resources and the platform's table-specific column rules.
pub(super) fn fields(
    project: &ProjectSession,
    kind: VirtualTable,
    table: &QueryTableChoice,
    document: &Document<'_>,
    parsed: &ParsedDescriptor,
    owner: &ParsedObject,
    output: &mut BTreeMap<String, QueryFieldChoice>,
) -> Result<(), QuerySessionError> {
    let node = fields::object_node(document, owner)?;
    if matches!(
        kind,
        VirtualTable::SliceFirst
            | VirtualTable::SliceLast
            | VirtualTable::RecordsWithExtDimensions
            | VirtualTable::AdjustedEffectivePeriod
            | VirtualTable::ScheduleData
    ) {
        fields::append(document, parsed, owner, table.category, output)?;
    }
    let context = columns::Context {
        project,
        document,
        parsed,
        owner,
        node,
        category: table.category,
        kind,
    };
    context.append(output)
}

/// XML Schema boolean word and numeric forms have identical meaning.
fn enabled(owner: Node<'_, '_>, name: &str) -> bool {
    matches!(property(owner, name), Some("true" | "1"))
}
