//! Lazy query choices use the selected application's real Designer metadata, independently of any IDE or analyzer.

use std::collections::BTreeMap;

mod constructor;
mod fields;
mod virtuals;

use serde::{Deserialize, Serialize};

use super::{ProjectSession, QueryDocumentError, QuerySessionError, WorkspaceError};
use crate::project::{
    designer_source,
    metadata_model::{MetadataKind, NodeId, ObjectId, ProjectScope},
    metadata_parser::{self, PropertiesMode},
    query::QueryError,
};

/// Catalog pages never require a complete descriptor walk or a language index.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryCatalogRequest {
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
    pub table: Option<String>,
    pub qualifier: Option<String>,
    #[serde(default)]
    pub virtual_tables: bool,
    pub source: Option<crate::project::query::constructor::Selection>,
}

/// Both spellings are query names, not locale-dependent identities or client-generated SQL.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryTableChoice {
    pub name: String,
    pub category: MetadataKind,
    pub russian: String,
    pub english: String,
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_table: Option<crate::project::query::virtual_tables::VirtualTable>,
}

/// Custom names are retained; standard query fields expose their real bilingual spelling.
#[derive(Debug, Serialize)]
pub struct QueryFieldChoice {
    pub russian: String,
    pub english: String,
}

/// A missing table is distinct from a successfully resolved table without selectable fields.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryCatalogPage {
    pub tables: Vec<QueryTableChoice>,
    pub fields: Vec<QueryFieldChoice>,
    pub next_offset: Option<usize>,
    pub found: bool,
    pub context_fingerprint: String,
    pub limited_context: bool,
    pub parameters: Vec<crate::project::query::virtual_tables::Parameter>,
}

impl QueryCatalogRequest {
    /// Apply the same bounded shape contract to physical and derived-source inspection.
    fn validate(&self) -> Result<(), QuerySessionError> {
        let limit = self.limit.unwrap_or(200);
        if limit == 0
            || limit > 500
            || self.search.len() > 1024
            || self.table.as_ref().is_some_and(|value| value.len() > 4096)
            || self.qualifier.as_ref().is_some_and(|value| {
                value.len() > 1024
                    || !value.split('.').all(|part| {
                        let mut chars = part.chars();
                        chars
                            .next()
                            .is_some_and(|ch| ch == '_' || ch.is_alphabetic())
                            && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
                    })
            })
        {
            return Err(QueryError::InvalidPosition.into());
        }
        Ok(())
    }
}

impl ProjectSession {
    /// Resolve only selected application roots and, for field requests, the requested owner descriptor.
    ///
    /// # Errors
    /// Rejects invalid selection, unavailable context and unreadable metadata instead of claiming an empty catalog.
    pub fn query_catalog(
        &self,
        application: Option<&str>,
        request: &QueryCatalogRequest,
    ) -> Result<QueryCatalogPage, QuerySessionError> {
        request.validate()?;
        if request.source.is_some() {
            return Err(QueryError::InvalidPosition.into());
        }
        let limit = request.limit.unwrap_or(200);
        let context = self.query_context(application)?;
        let mut choices = CatalogContents::default();
        for source in context.context.sources() {
            if source.project().source() == self.project().source() {
                collect(self, request, &mut choices)?;
            } else {
                let scope = source.member().map_or(ProjectScope::Standalone, |name| {
                    ProjectScope::Member(name.clone())
                });
                let source = designer_source::open_project(source.project().clone(), scope, true)
                    .map_err(|error| workspace_error(WorkspaceError::Source(error)))?;
                let project = Self::open(source).map_err(workspace_error)?;
                collect(&project, request, &mut choices)?;
            }
        }
        let search = request.search.to_lowercase();
        let mut matching = choices
            .tables
            .into_values()
            .filter(|table| {
                search.is_empty()
                    || table.russian.to_lowercase().contains(&search)
                    || table.english.to_lowercase().contains(&search)
            })
            .skip(request.offset);
        let tables: Vec<_> = matching.by_ref().take(limit).collect();
        let next_offset = matching
            .next()
            .map(|_| request.offset.saturating_add(tables.len()));
        let fields = choices
            .fields
            .into_values()
            .map(|mut field| {
                if let Some(alias) = &request.qualifier {
                    field.russian = format!("{alias}.{}", field.russian);
                    field.english = format!("{alias}.{}", field.english);
                }
                field
            })
            .collect();
        Ok(QueryCatalogPage {
            tables,
            fields,
            next_offset,
            found: request.table.is_none() || choices.found,
            context_fingerprint: context.fingerprint,
            limited_context: context.context.limited_context(),
            parameters: choices.parameters,
        })
    }
}

/// One accumulation keeps overlay fields and positional signatures consistent across application sources.
#[derive(Default)]
struct CatalogContents {
    tables: BTreeMap<String, QueryTableChoice>,
    fields: BTreeMap<String, QueryFieldChoice>,
    found: bool,
    parameters: Vec<crate::project::query::virtual_tables::Parameter>,
}

/// Root declarations enumerate physical tables without opening every catalog, document or register.
fn collect(
    project: &ProjectSession,
    request: &QueryCatalogRequest,
    choices: &mut CatalogContents,
) -> Result<(), QuerySessionError> {
    for object in project
        .objects
        .values()
        .filter(|object| object.parent.is_none())
    {
        let Some((russian, english)) = prefixes(object.kind) else {
            continue;
        };
        let table = choice(object.kind, &object.name, russian, english);
        if let Some(requested) = request.table.as_deref() {
            let matches = [table.russian.as_str(), table.english.as_str()]
                .iter()
                .any(|name| {
                    same_name(requested, name)
                        || requested
                            .to_lowercase()
                            .starts_with(&format!("{}.", name.to_lowercase()))
                });
            if matches {
                collect_fields(project, &object.id, &table, request, choices)?;
            }
        } else {
            choices
                .tables
                .entry(table.english.to_lowercase())
                .or_insert(table);
        }
    }
    Ok(())
}

/// Fields and tabular sections are loaded from this single checked descriptor; modules and neighboring descriptors stay unread.
fn collect_fields(
    project: &ProjectSession,
    id: &ObjectId,
    table: &QueryTableChoice,
    request: &QueryCatalogRequest,
    choices: &mut CatalogContents,
) -> Result<(), QuerySessionError> {
    let parsed = metadata_parser::load(&project.source, id, PropertiesMode::Summary)
        .map_err(|error| workspace_error(WorkspaceError::Load(error)))?;
    let location = project
        .source
        .object_descriptor(id)
        .map_err(|error| workspace_error(WorkspaceError::Source(error)))?
        .ok_or_else(|| {
            workspace_error(WorkspaceError::MissingSource(NodeId::Object(id.clone())))
        })?;
    let input = project
        .source
        .read_xml(&location.path)
        .map_err(|error| workspace_error(WorkspaceError::Source(error)))?
        .ok_or_else(|| {
            workspace_error(WorkspaceError::MissingSource(NodeId::Object(id.clone())))
        })?;
    let document = roxmltree::Document::parse(&input)
        .map_err(|_| QuerySessionError::from(QueryError::InvalidXml))?;
    let owner = parsed
        .objects
        .iter()
        .find(|object| object.metadata.id() == id)
        .ok_or_else(|| workspace_error(WorkspaceError::UnknownObject(id.clone())))?;
    let requested = request
        .table
        .as_deref()
        .ok_or(QueryError::InvalidPosition)?;
    let is_root = same_name(requested, &table.russian) || same_name(requested, &table.english);
    if request.virtual_tables {
        let node = fields::object_node(&document, owner)?;
        if is_root {
            virtuals::choices(table, node, &mut choices.tables);
        } else if let Some((category, virtual_table)) =
            crate::project::query::virtual_tables::VirtualTable::resolve(requested)
        {
            if virtuals::available(virtual_table, category, node) {
                choices.found = true;
                if choices.parameters.is_empty() {
                    choices.parameters = virtual_table.parameters(
                        category,
                        fields::property(node, "Correspondence") == Some("true"),
                    );
                }
                return virtuals::fields(
                    project,
                    virtual_table,
                    table,
                    &document,
                    &parsed,
                    owner,
                    &mut choices.fields,
                );
            }
            return Ok(());
        }
    }
    let selected = if is_root {
        Some(owner)
    } else {
        parsed.objects.iter().find(|object| {
            object.metadata.parent() == Some(id)
                && object.metadata.kind() == MetadataKind::TabularSection
                && [
                    format!("{}.{}", table.russian, object.metadata.name()),
                    format!("{}.{}", table.english, object.metadata.name()),
                ]
                .iter()
                .any(|name| same_name(requested, name))
        })
    };
    let Some(selected) = selected else {
        return Ok(());
    };
    choices.found = true;
    for object in parsed.objects.iter().filter(|object| {
        object.metadata.parent() == Some(selected.metadata.id())
            && object.metadata.kind() == MetadataKind::TabularSection
    }) {
        let name = object.metadata.name();
        let value = QueryTableChoice {
            name: name.to_owned(),
            category: table.category,
            russian: format!("{}.{name}", table.russian),
            english: format!("{}.{name}", table.english),
            alias: name.to_owned(),
            virtual_table: None,
        };
        choices
            .tables
            .entry(value.english.to_lowercase())
            .or_insert(value);
    }
    fields::append(
        &document,
        &parsed,
        selected,
        table.category,
        &mut choices.fields,
    )?;
    Ok(())
}

/// Query tables have language-specific namespaces; forms, modules and processors are not database tables.
const fn prefixes(kind: MetadataKind) -> Option<(&'static str, &'static str)> {
    Some(match kind {
        MetadataKind::Catalog => ("Справочник", "Catalog"),
        MetadataKind::Document => ("Документ", "Document"),
        MetadataKind::Enum => ("Перечисление", "Enum"),
        MetadataKind::InformationRegister => ("РегистрСведений", "InformationRegister"),
        MetadataKind::AccumulationRegister => ("РегистрНакопления", "AccumulationRegister"),
        MetadataKind::AccountingRegister => ("РегистрБухгалтерии", "AccountingRegister"),
        MetadataKind::CalculationRegister => ("РегистрРасчета", "CalculationRegister"),
        MetadataKind::ChartOfAccounts => ("ПланСчетов", "ChartOfAccounts"),
        MetadataKind::ChartOfCharacteristicTypes => {
            ("ПланВидовХарактеристик", "ChartOfCharacteristicTypes")
        }
        MetadataKind::ChartOfCalculationTypes => ("ПланВидовРасчета", "ChartOfCalculationTypes"),
        MetadataKind::ExchangePlan => ("ПланОбмена", "ExchangePlan"),
        MetadataKind::BusinessProcess => ("БизнесПроцесс", "BusinessProcess"),
        MetadataKind::Task => ("Задача", "Task"),
        MetadataKind::DocumentJournal => ("ЖурналДокументов", "DocumentJournal"),
        _ => return None,
    })
}

/// Names originate in metadata; aliases are suggestions, never rewritten client expressions.
fn choice(category: MetadataKind, name: &str, russian: &str, english: &str) -> QueryTableChoice {
    QueryTableChoice {
        name: name.to_owned(),
        category,
        russian: format!("{russian}.{name}"),
        english: format!("{english}.{name}"),
        alias: name.to_owned(),
        virtual_table: None,
    }
}

/// Standard spelling is shared across configurations; owner-dependent availability comes from the existing field provider.
fn standard_name(english: &str) -> Option<&'static str> {
    Some(match english {
        "Ref" => "Ссылка",
        "DeletionMark" => "ПометкаУдаления",
        "Predefined" => "Предопределенный",
        "PredefinedDataName" => "ИмяПредопределенныхДанных",
        "Code" => "Код",
        "Description" => "Наименование",
        "Parent" => "Родитель",
        "IsFolder" => "ЭтоГруппа",
        "Owner" => "Владелец",
        "Date" => "Дата",
        "Number" => "Номер",
        "Posted" => "Проведен",
        "LineNumber" => "НомерСтроки",
        "Active" => "Активность",
        "Recorder" => "Регистратор",
        "Period" => "Период",
        "RecordType" => "ВидДвижения",
        "RegistrationPeriod" => "ПериодРегистрации",
        "Account" => "Счет",
        "PeriodAdjustment" => "ПериодКорректировки",
        "Started" => "Стартован",
        "Completed" => "Завершен",
        "Executed" => "Выполнена",
        "Type" => "Вид",
        "OffBalance" => "Забалансовый",
        "Order" => "Порядок",
        "ValueType" => "ТипЗначения",
        _ => return None,
    })
}

/// Metadata names use Unicode case folding consistently for base and extension overlays.
fn same_name(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

/// Preserve the common source/error model instead of embedding localized catalog diagnostics.
fn workspace_error(error: WorkspaceError) -> QuerySessionError {
    QueryDocumentError::from(error).into()
}
