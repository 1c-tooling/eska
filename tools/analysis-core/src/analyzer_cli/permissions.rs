//! Read-only policy requests use current snapshots and independent accepted-base approval.

use std::{collections::BTreeSet, time::Duration};

use eska::{project_core::eska_source::EskaSource, tool_cli};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AnalysisSession, RequestCancellation};

use super::{dto, protocol, read_permissions::ReadPermissions, request_control};

pub const METHOD: &str = "eska/metadata/permissions";
const MAX_SOURCES: usize = 200;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub snapshot: protocol::Snapshot,
    sources: Vec<String>,
}

/// The inventory remains read-only even when independently checked policy would allow a source edit.
pub fn capability() -> Value {
    json!({"method":METHOD,"scope":"declared-source","readOnly":true,"cancellable":true,
        "approval":"independent-accepted-base-pin","maxSources":MAX_SOURCES})
}

/// Parse only caller/environment approval; editable manifests and generated candidates grant none.
pub fn selected(source: &EskaSource, explicit: Option<&str>) -> Result<ReadPermissions, Value> {
    let reference = if explicit.is_some() || std::env::var_os("ESKA_ACCEPTED_BASE_SHA256").is_some()
    {
        Some(
            tool_cli::accepted_ref(explicit)
                .map_err(|failure| protocol::error_value(-32602, failure.code, Value::Null))?,
        )
    } else {
        None
    };
    Ok(ReadPermissions::selected(source, reference))
}

/// Strict bounded sources and cooperative stop controls precede full accepted-source verification.
pub fn read(
    authority: &ReadPermissions,
    session: &AnalysisSession,
    id: &str,
    params: &Params,
    cancellation: &RequestCancellation,
    timeout: Duration,
) -> Result<Value, Value> {
    if !(1..=MAX_SOURCES).contains(&params.sources.len()) {
        return Err(protocol::error_value(
            -32602,
            "permission_sources_invalid",
            json!({"min":1,"max":MAX_SOURCES}),
        ));
    }
    let sources = params
        .sources
        .iter()
        .map(|source| {
            eska::project_core::source_format::SourcePath::parse(source)
                .map_err(|error| protocol::error_value(-32602, error.code, Value::Null))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if sources.iter().collect::<BTreeSet<_>>().len() != sources.len() {
        return Err(protocol::error_value(
            -32602,
            "permission_sources_invalid",
            json!({"unique":true}),
        ));
    }
    let method = request_control::Method::ReadPermissions;
    let deadline = request_control::start(session, id, method, cancellation, timeout)?;
    let result = authority
        .files(session, &sources, cancellation)
        .map(|files| json!({"snapshot":dto::snapshot(session,id),"files":files}))
        .map_err(|error| {
            protocol::error_value(-32000, error.code, json!({"location":error.location}))
        });
    request_control::finish(session, id, method, &deadline, result)
}
