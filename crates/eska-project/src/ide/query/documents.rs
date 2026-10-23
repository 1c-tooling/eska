//! A query document binds checked application composition to the original unsaved owner.

use eska_project_core::application::{self, ContextSnapshot};
use eska_project_core::project::{
    metadata_edit::snapshot,
    metadata_workspace::{ProjectSession, QueryDocument, QueryDocumentKind, QueryRevision},
    query::{Affinity, Query, QueryIdentity, position_byte},
};
use serde_json::{Value, json};

use super::{
    MAX_DOCUMENTS, State, codec_error, document_error, dto, failure,
    params::{self, Key},
};
use crate::ide::{
    envelope::{domain, error},
    params::{counter, decode, node, optional_fields},
};

pub(super) struct Entry {
    pub key: Key,
    pub application: Option<String>,
    pub fingerprint: String,
    pub document: QueryDocument,
}

impl Entry {
    /// Re-resolve declared application/member roots without generating config or starting an analyzer.
    fn check_context(&self, project: &ProjectSession) -> Result<(), Value> {
        if context(project, self.application.as_deref())?.fingerprint != self.fingerprint {
            return Err(failure("context_changed"));
        }
        Ok(())
    }

    /// Editor versions and application fingerprints are both required on every read/plan.
    fn revision<'a>(&self, input: &'a params::Revision) -> Result<QueryRevision<'a>, Value> {
        if input.context_fingerprint != self.fingerprint {
            return Err(failure("context_changed"));
        }
        Ok(QueryRevision {
            version: counter(&input.version)?,
            snapshot: &input.snapshot,
        })
    }

    /// Report source binding without exposing another complete copy of the owner in each response.
    fn binding(&self) -> Value {
        json!({"version":self.document.version().to_string(),"snapshot":self.document.snapshot(),
            "diskSnapshot":self.document.disk_snapshot(),"source":crate::ide::dto::path(self.document.path()),
            "sourceKind":match self.document.kind() {QueryDocumentKind::Bsl=>"bsl",QueryDocumentKind::Form=>"form",QueryDocumentKind::DataComposition=>"dataCompositionSchema"},
            "contextFingerprint":self.fingerprint,"application":self.application,"writable":self.document.writable()})
    }
}

impl State {
    /// Open against the editor's exact text, with a disk snapshot retained only for external conflicts.
    pub(super) fn open_document(
        &mut self,
        project: &mut ProjectSession,
        key: &Key,
        args: &Value,
    ) -> Result<Value, Value> {
        optional_fields(args, &["application", "bufferId", "text"])?;
        let input: params::Open = decode(args)?;
        if input.application.as_ref().is_some_and(String::is_empty) {
            return Err(error(-32602));
        }
        if self.documents.len() == MAX_DOCUMENTS {
            return Err(domain(
                "resource_limit",
                json!({"resource":"query_documents","maxEntries":MAX_DOCUMENTS}),
            ));
        }
        let source = input.source.native()?;
        if self
            .documents
            .values()
            .any(|entry| entry.key.same_owner(key) && entry.document.path() == source)
        {
            return Err(failure("already_open"));
        }
        let context = context(project, input.application.as_deref())?;
        let buffer = self.input_text(key, input.text.as_deref(), input.buffer_id.as_deref())?;
        let document = project
            .query_document(
                &node(&input.node)?,
                &source,
                counter(&input.version)?,
                buffer,
            )
            .map_err(|error| document_error(&error))?;
        self.admit(document.input().len())?;
        let entry = Entry {
            key: key.clone(),
            application: context.context.name().map(|name| name.as_str().to_owned()),
            fingerprint: context.fingerprint,
            document,
        };
        entry.check_context(project)?;
        let id = self.next_id("d")?;
        let mut output = entry.binding();
        output["documentId"] = json!(id);
        output["limitedContext"] = json!(context.context.limited_context());
        self.documents.insert(id, entry);
        Ok(output)
    }

    /// Plan/list/map use immutable owner snapshots but fresh source/policy/application checks.
    pub(super) fn document_request(
        &self,
        project: &mut ProjectSession,
        key: &Key,
        method: &str,
        args: &Value,
    ) -> Result<Value, Value> {
        let input: params::Revision = decode(args)?;
        let entry = self
            .documents
            .get(&input.document_id)
            .filter(|entry| &entry.key == key)
            .ok_or_else(|| failure("unknown_document"))?;
        let revision = entry.revision(&input)?;
        entry.check_context(project)?;
        let writable = project
            .check_query_read(&entry.document, revision)
            .map_err(|error| document_error(&error))?;
        let mut output = match method {
            "query/list" => list(&entry.document, args)?,
            "query/read" => {
                optional_fields(args, &["query", "at"])?;
                let input: params::Selection = decode(args)?;
                let query = selected(&entry.document, input)?;
                json!({"query":dto::query(entry.document.input(), &query)})
            }
            "query/map" => map(&entry.document, args)?,
            "query/plan" => self.plan_document(project, entry, revision, args)?,
            _ => return Err(error(-32601)),
        };
        let current_writable = project
            .check_query_read(&entry.document, revision)
            .map_err(|error| document_error(&error))?;
        entry.check_context(project)?;
        output["documentId"] = json!(input.document_id);
        output["version"] = json!(entry.document.version().to_string());
        output["snapshot"] = json!(entry.document.snapshot());
        output["contextFingerprint"] = json!(entry.fingerprint);
        output["writable"] = json!(writable && current_writable);
        Ok(output)
    }

    /// A projected owner is returned as replacements; no file or metadata generation is published.
    fn plan_document(
        &self,
        project: &mut ProjectSession,
        entry: &Entry,
        revision: QueryRevision<'_>,
        args: &Value,
    ) -> Result<Value, Value> {
        optional_fields(args, &["text", "bufferId"])?;
        let input: params::Plan = decode(args)?;
        let value = self
            .input_text(
                &entry.key,
                input.text.as_deref(),
                input.buffer_id.as_deref(),
            )?
            .ok_or_else(|| error(-32602))?;
        let identity = input.query.core()?;
        let decisions = input
            .comments
            .iter()
            .map(params::Placement::core)
            .collect::<Result<Vec<_>, _>>()?;
        let plan = project
            .plan_query(&entry.document, revision, &identity, value, &decisions)
            .map_err(|error| document_error(&error))?;
        let query = match identity {
            QueryIdentity::BslLiteral(range) => {
                eska_project_core::project::query::read_bsl(plan.output(), range.start)
            }
            _ => eska_project_core::project::query::read_xml(plan.output()).and_then(|queries| {
                queries
                    .into_iter()
                    .find(|query| query.identity == identity)
                    .ok_or(eska_project_core::project::query::QueryError::NotFound)
            }),
        }
        .map_err(codec_error)?;
        Ok(
            json!({"noOp":plan.is_empty(),"outputSnapshot":snapshot(plan.output()),
            "replacements":plan.replacements().iter().map(|replacement|json!({"range":dto::bytes(&replacement.range),"rangeUtf16":{"start":dto::point(plan.original(),replacement.range.start),"end":dto::point(plan.original(),replacement.range.end)},"text":replacement.text})).collect::<Vec<_>>(),
            "projectedQuery":dto::query(plan.output(), &query)}),
        )
    }
}

/// Selected member membership is proved from manifests and root descriptors, not a caller's path.
fn context(project: &ProjectSession, application: Option<&str>) -> Result<ContextSnapshot, Value> {
    let plan =
        application::resolve_snapshot(project.project().root(), application).map_err(|error| {
            domain(
                "query_context_unavailable",
                crate::analyzer_config::error_document(&eska_analyzer::Error::Context(Box::new(
                    error,
                ))),
            )
        })?;
    if !plan.context.sources().iter().any(|source| {
        source.project().root() == project.project().root()
            && source.project().source() == project.project().source()
    }) {
        return Err(failure("context_member_mismatch"));
    }
    Ok(plan)
}

/// An initial BSL cursor and a retained query identity are mutually exclusive selectors.
fn selected(document: &QueryDocument, input: params::Selection) -> Result<Query, Value> {
    match (input.query, input.at) {
        (Some(identity), None) => by_identity(document, &identity.core()?),
        (None, Some(at)) => document
            .bsl_query(
                position_byte(document.input(), at.core())
                    .ok_or_else(|| failure("invalid_position"))?,
            )
            .map_err(codec_error),
        _ => Err(error(-32602)),
    }
}

/// Byte ranges are valid only when the scanner finds the exact same whole literal again.
fn by_identity(document: &QueryDocument, identity: &QueryIdentity) -> Result<Query, Value> {
    let query = match identity {
        QueryIdentity::BslLiteral(range) => document.bsl_query(range.start),
        _ => document.xml_queries().and_then(|queries| {
            queries
                .into_iter()
                .find(|query| &query.identity == identity)
                .ok_or(eska_project_core::project::query::QueryError::NotFound)
        }),
    }
    .map_err(codec_error)?;
    if &query.identity != identity {
        return Err(failure("conflict"));
    }
    Ok(query)
}

/// XML tree discovery returns bounded headers; complete values/maps are read only on demand.
fn list(document: &QueryDocument, args: &Value) -> Result<Value, Value> {
    optional_fields(args, &["offset", "limit"])?;
    let input: params::List = decode(args)?;
    let offset = input.offset.unwrap_or(0);
    let limit = input.limit.unwrap_or(64);
    if !(1..=64).contains(&limit) {
        return Err(error(-32602));
    }
    let queries = document.xml_queries().map_err(codec_error)?;
    let end = offset.saturating_add(limit).min(queries.len());
    Ok(
        json!({"queries":queries.iter().skip(offset).take(limit).map(|query|json!({"identity":dto::identity(&query.identity),"sourceRange":dto::bytes(&query.source)})).collect::<Vec<_>>(),"nextOffset":(end<queries.len()).then_some(end)}),
    )
}

/// Convert batches of UTF-16 positions without mapping source-only syntax to a nearby SDBL token.
fn map(document: &QueryDocument, args: &Value) -> Result<Value, Value> {
    optional_fields(args, &["affinity"])?;
    let input: params::Map = decode(args)?;
    if input.positions.len() > 64 {
        return Err(domain(
            "resource_limit",
            json!({"resource":"query_mapping_positions","maxItems":64}),
        ));
    }
    let query = by_identity(document, &input.query.core()?)?;
    let affinity = match input.affinity {
        Some(params::Affinity::Before) => Affinity::Before,
        _ => Affinity::After,
    };
    let positions = input
        .positions
        .iter()
        .map(|point| {
            let (from, to) = match input.direction {
                params::Direction::QueryToSource => (query.text.as_str(), document.input()),
                params::Direction::SourceToQuery => (document.input(), query.text.as_str()),
            };
            let byte =
                position_byte(from, point.core()).ok_or_else(|| failure("invalid_position"))?;
            let translated = match input.direction {
                params::Direction::QueryToSource => {
                    query.map.query_to_source(&query.text, byte, affinity)
                }
                params::Direction::SourceToQuery => {
                    query.map.source_to_query(document.input(), byte)
                }
            };
            Ok(translated.map_or(Value::Null, |offset| dto::point(to, offset)))
        })
        .collect::<Result<Vec<_>, Value>>()?;
    Ok(json!({"positions":positions}))
}
