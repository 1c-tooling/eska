//! Register-specific virtual columns never leak into a physical record table.

use super::super::{
    ProjectSession, QueryFieldChoice, QuerySessionError, WorkspaceError,
    fields::{insert, object_node, property},
    workspace_error,
};
use crate::project::{
    metadata_model::{MetadataKind, NodeId},
    metadata_parser::{ParsedDescriptor, ParsedObject},
    query::{QueryError, virtual_tables::VirtualTable},
};
use roxmltree::{Document, Node};
use std::collections::BTreeMap;

pub(super) struct Context<'a, 'input> {
    pub project: &'a ProjectSession,
    pub document: &'a Document<'input>,
    pub parsed: &'a ParsedDescriptor,
    pub owner: &'a ParsedObject,
    pub node: Node<'a, 'input>,
    pub category: MetadataKind,
    pub kind: VirtualTable,
}

impl Context<'_, '_> {
    /// Keep resources and dimensions distinct: balances and turnovers expose transformed resource columns.
    pub(super) fn append(
        &self,
        output: &mut BTreeMap<String, QueryFieldChoice>,
    ) -> Result<(), QuerySessionError> {
        for object in self
            .parsed
            .objects
            .iter()
            .filter(|object| object.metadata.parent() == Some(self.owner.metadata.id()))
        {
            let name = object.metadata.name();
            match object.metadata.kind() {
                MetadataKind::Dimension if !matches!(self.kind, VirtualTable::ExtDimensions) => {
                    self.dimension(object, output)?;
                }
                MetadataKind::Resource => {
                    let unbalanced = matches!(
                        property(object_node(self.document, object)?, "Balance"),
                        Some("false" | "0")
                    );
                    self.resource(name, unbalanced, output);
                    if self.kind == VirtualTable::Turnovers
                        && self.category == MetadataKind::AccountingRegister
                        && matches!(
                            property(object_node(self.document, object)?, "Balance"),
                            Some("false" | "0")
                        )
                        && matches!(property(self.node, "Correspondence"), Some("true" | "1"))
                    {
                        for (ru, en) in [
                            ("КорОборот", "BalancedTurnover"),
                            ("КорОборотДт", "BalancedTurnoverDr"),
                            ("КорОборотКт", "BalancedTurnoverCr"),
                        ] {
                            insert(output, &format!("{name}{ru}"), &format!("{name}{en}"));
                        }
                    }
                }
                _ => (),
            }
        }
        self.standard(output)?;
        Ok(())
    }

    /// Aggregate virtual tables collapse debit/credit dimensions while record views retain their physical split.
    fn dimension(
        &self,
        object: &ParsedObject,
        output: &mut BTreeMap<String, QueryFieldChoice>,
    ) -> Result<(), QuerySessionError> {
        let name = object.metadata.name();
        let unbalanced = self.category == MetadataKind::AccountingRegister
            && matches!(
                property(object_node(self.document, object)?, "Balance"),
                Some("false" | "0")
            );
        if self.kind == VirtualTable::RecordsWithExtDimensions && unbalanced {
            return Ok(());
        }
        if self.kind == VirtualTable::DrCrTurnovers && unbalanced {
            insert(output, &format!("{name}Дт"), &format!("{name}Dr"));
            insert(output, &format!("{name}Кт"), &format!("{name}Cr"));
        } else {
            insert(output, name, name);
        }
        if self.kind == VirtualTable::Turnovers
            && unbalanced
            && matches!(property(self.node, "Correspondence"), Some("true" | "1"))
        {
            insert(output, &format!("{name}Кор"), &format!("{name}Balanced"));
        }
        Ok(())
    }

    /// Suffixes are query-language identifiers, not human captions or guessed record properties.
    fn resource(
        &self,
        name: &str,
        unbalanced: bool,
        output: &mut BTreeMap<String, QueryFieldChoice>,
    ) {
        let accounting = self.category == MetadataKind::AccountingRegister;
        let suffixes: &[(&str, &str)] = match self.kind {
            VirtualTable::Balance if accounting => &[
                ("Остаток", "Balance"),
                ("ОстатокДт", "BalanceDr"),
                ("ОстатокКт", "BalanceCr"),
                ("РазвернутыйОстатокДт", "SplittedBalanceDr"),
                ("РазвернутыйОстатокКт", "SplittedBalanceCr"),
            ],
            VirtualTable::Balance => &[("Остаток", "Balance")],
            VirtualTable::Turnovers if accounting => &[
                ("Оборот", "Turnover"),
                ("ОборотДт", "TurnoverDr"),
                ("ОборотКт", "TurnoverCr"),
            ],
            VirtualTable::Turnovers | VirtualTable::BalanceAndTurnovers => &[
                ("Оборот", "Turnover"),
                ("Приход", "Receipt"),
                ("Расход", "Expense"),
            ],
            VirtualTable::DrCrTurnovers if unbalanced => {
                &[("ОборотДт", "TurnoverDr"), ("ОборотКт", "TurnoverCr")]
            }
            VirtualTable::DrCrTurnovers => &[("Оборот", "Turnover")],
            _ => &[],
        };
        for (russian, english) in suffixes {
            if self.category == MetadataKind::AccumulationRegister
                && property(self.node, "RegisterType") == Some("Turnovers")
                && ["Receipt", "Expense"].contains(english)
            {
                continue;
            }
            insert(
                output,
                &format!("{name}{russian}"),
                &format!("{name}{english}"),
            );
        }
        if self.kind == VirtualTable::BalanceAndTurnovers {
            for (russian, english) in [
                ("НачальныйОстаток", "OpeningBalance"),
                ("КонечныйОстаток", "ClosingBalance"),
            ] {
                insert(
                    output,
                    &format!("{name}{russian}"),
                    &format!("{name}{english}"),
                );
                if accounting {
                    for (ru, en) in [("Дт", "Dr"), ("Кт", "Cr")] {
                        insert(
                            output,
                            &format!("{name}{russian}{ru}"),
                            &format!("{name}{english}{en}"),
                        );
                    }
                }
            }
            if accounting {
                for (ru, en) in [
                    ("ОборотДт", "TurnoverDr"),
                    ("ОборотКт", "TurnoverCr"),
                    ("НачальныйРазвернутыйОстатокДт", "OpeningSplittedBalanceDr"),
                    ("НачальныйРазвернутыйОстатокКт", "OpeningSplittedBalanceCr"),
                    ("КонечныйРазвернутыйОстатокДт", "ClosingSplittedBalanceDr"),
                    ("КонечныйРазвернутыйОстатокКт", "ClosingSplittedBalanceCr"),
                ] {
                    insert(output, &format!("{name}{ru}"), &format!("{name}{en}"));
                }
                output.remove(&format!("{name}Receipt").to_lowercase());
                output.remove(&format!("{name}Expense").to_lowercase());
            }
        }
    }

    /// Period and debit/credit fields follow the requested virtual table, never the physical-table provider.
    fn standard(
        &self,
        output: &mut BTreeMap<String, QueryFieldChoice>,
    ) -> Result<(), QuerySessionError> {
        if matches!(
            self.kind,
            VirtualTable::Turnovers
                | VirtualTable::BalanceAndTurnovers
                | VirtualTable::DrCrTurnovers
        ) {
            for (russian, english) in [
                ("Период", "Period"),
                ("Регистратор", "Recorder"),
                ("НомерСтроки", "LineNumber"),
            ] {
                insert(output, russian, english);
            }
            for (russian, english) in [
                ("Секунда", "Second"),
                ("Минута", "Minute"),
                ("Час", "Hour"),
                ("День", "Day"),
                ("Неделя", "Week"),
                ("Декада", "TenDays"),
                ("Месяц", "Month"),
                ("Квартал", "Quarter"),
                ("Полугодие", "HalfYear"),
                ("Год", "Year"),
            ] {
                insert(
                    output,
                    &format!("Период{russian}"),
                    &format!("{english}Period"),
                );
            }
        }
        if self.category == MetadataKind::AccountingRegister {
            self.accounting(output)?;
        }
        Ok(())
    }

    /// Numbered subaccounts are bounded by the assigned chart's actual `MaxExtDimensionCount`.
    fn accounting(
        &self,
        output: &mut BTreeMap<String, QueryFieldChoice>,
    ) -> Result<(), QuerySessionError> {
        let correspondence = matches!(property(self.node, "Correspondence"), Some("true" | "1"));
        if property(self.node, "PeriodAdjustmentLength")
            .and_then(|value| value.parse::<u32>().ok())
            .is_some_and(|value| value > 0)
            && matches!(
                self.kind,
                VirtualTable::RecordsWithExtDimensions
                    | VirtualTable::Turnovers
                    | VirtualTable::DrCrTurnovers
                    | VirtualTable::BalanceAndTurnovers
            )
        {
            insert(output, "УточнениеПериода", "PeriodAdjustment");
        }
        if self.kind == VirtualTable::RecordsWithExtDimensions {
            insert(output, "МоментВремени", "PointInTime");
        }
        if self.kind == VirtualTable::Turnovers {
            insert(output, "КорСчет", "BalancedAccount");
        }
        let split = self.kind == VirtualTable::DrCrTurnovers
            || (self.kind == VirtualTable::RecordsWithExtDimensions && correspondence);
        if self.kind == VirtualTable::ExtDimensions {
            for (russian, english) in [
                ("Вид", "Type"),
                ("Значение", "Value"),
                ("Период", "Period"),
                ("Регистратор", "Recorder"),
                ("НомерСтроки", "LineNumber"),
                ("МоментВремени", "PointInTime"),
            ] {
                insert(output, russian, english);
            }
            if correspondence {
                insert(output, "ВидДвижения", "RecordType");
            }
            return Ok(());
        }
        if split {
            insert(output, "СчетДт", "AccountDr");
            insert(output, "СчетКт", "AccountCr");
        } else {
            insert(output, "Счет", "Account");
        }
        for index in 1..=self.extra_dimension_count()? {
            if self.kind == VirtualTable::Turnovers {
                insert(
                    output,
                    &format!("КорСубконто{index}"),
                    &format!("BalancedExtDimension{index}"),
                );
            }
            for (ru, en) in if split {
                &[("Дт", "Dr"), ("Кт", "Cr")][..]
            } else {
                &[("", "")][..]
            } {
                insert(
                    output,
                    &format!("Субконто{ru}{index}"),
                    &format!("ExtDimension{en}{index}"),
                );
                if self.kind == VirtualTable::RecordsWithExtDimensions {
                    insert(
                        output,
                        &format!("ВидСубконто{ru}{index}"),
                        &format!("ExtDimensionType{en}{index}"),
                    );
                }
            }
        }
        Ok(())
    }

    /// Read only the explicitly related chart; an unreadable declared descriptor remains an error.
    fn extra_dimension_count(&self) -> Result<usize, QuerySessionError> {
        let Some(reference) = property(self.node, "ChartOfAccounts") else {
            return Ok(0);
        };
        let name = reference.rsplit('.').next().unwrap_or(reference);
        let Some(object) = self
            .project
            .objects
            .values()
            .find(|object| object.kind == MetadataKind::ChartOfAccounts && object.name == name)
        else {
            return Ok(0);
        };
        let location = self
            .project
            .source
            .object_descriptor(&object.id)
            .map_err(|error| workspace_error(WorkspaceError::Source(error)))?
            .ok_or_else(|| {
                workspace_error(WorkspaceError::MissingSource(NodeId::Object(
                    object.id.clone(),
                )))
            })?;
        let text = self
            .project
            .source
            .read_xml(&location.path)
            .map_err(|error| workspace_error(WorkspaceError::Source(error)))?
            .ok_or_else(|| {
                workspace_error(WorkspaceError::MissingSource(NodeId::Object(
                    object.id.clone(),
                )))
            })?;
        let doc = Document::parse(&text).map_err(|_| QueryError::InvalidXml)?;
        Ok(doc
            .descendants()
            .find(|node| node.tag_name().name() == "ChartOfAccounts")
            .and_then(|node| property(node, "MaxExtDimensionCount"))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
            .min(100))
    }
}
