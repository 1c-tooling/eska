//! Standard full-document synchronization delegates bytes and version checks to the common host.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use eska::{
    analysis_core::{InputRequest, UpdateError},
    project_core::source_format::SourcePath,
};
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

use crate::{AnalysisSession, SessionRequestError, composition::SourceKey};

use super::{dto, protocol, query_syntax};

pub const MAX_REQUEST: usize = 4 * 1024 * 1024;

/// Only open/close ownership is duplicated here; text, versions and hashes belong to the host.
#[derive(Default)]
pub struct Documents {
    open: BTreeSet<SourceKey>,
    query_syntax: bool,
    query_analysis: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Open {
    text_document: Item,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Item {
    uri: String,
    language_id: String,
    version: i32,
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Identifier {
    uri: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Versioned {
    uri: String,
    version: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Change {
    text_document: Versioned,
    content_changes: Vec<Content>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Content {
    text: String,
    range: Option<Value>,
    range_length: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Close {
    text_document: Identifier,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Save {
    text_document: Identifier,
    /// Save notifications do not replace the editor-owned current bytes or their version.
    #[serde(rename = "text")]
    _text: Option<String>,
}

impl Documents {
    /// One initialize handshake fixes query synchronization until this host is retired.
    pub const fn with_query_syntax(query_syntax: bool) -> Self {
        Self {
            open: BTreeSet::new(),
            query_syntax,
            query_analysis: false,
        }
    }

    /// Contextual query synchronization is negotiated independently of the parse-only channel.
    pub const fn with_query_analysis(mut self, enabled: bool) -> Self {
        self.query_analysis = enabled;
        self
    }

    /// Diagnostic routing preserves both independent choices for this host.
    pub const fn query_analysis(&self) -> bool {
        self.query_analysis
    }

    /// Diagnostic requests share the same negotiated query document boundary as notifications.
    pub const fn query_syntax(&self) -> bool {
        self.query_syntax
    }

    /// One event acknowledges each supported notification; rejected updates preserve current state.
    pub fn notify(
        &mut self,
        session: &mut AnalysisSession,
        id: &str,
        method: &str,
        params: &Value,
        request: &InputRequest,
    ) -> Value {
        match self.update(session, method, params, request) {
            Ok(path) => json!({"jsonrpc":"2.0", "method":"eska/inputChanged", "params":{
                "snapshot":dto::snapshot(session, id), "source":path.source,"root":path.root,
                "input":current_at(session, &path), "resources":session.resources()}}),
            Err(error) => json!({"jsonrpc":"2.0", "method":"eska/inputRejected", "params":{
                "snapshot":dto::snapshot(session, id), "operation":method, "error":error}}),
        }
    }

    /// A balanced lifecycle is checked before the host can accept a new revision.
    fn update(
        &mut self,
        session: &mut AnalysisSession,
        method: &str,
        params: &Value,
        request: &InputRequest,
    ) -> Result<SourceKey, Value> {
        match method {
            "textDocument/didOpen" => self.open(session, protocol::params(params)?, request),
            "textDocument/didChange" => self.change(session, &protocol::params(params)?, request),
            "textDocument/didClose" => self.close(session, &protocol::params(params)?, request),
            "textDocument/didSave" => {
                let params: Save = protocol::params(params)?;
                self.require_open(session, &params.text_document.uri)
            }
            _ => Err(protocol::error_value(
                -32601,
                "rpc_method_not_found",
                Value::Null,
            )),
        }
    }

    /// Opening once gives the editor authority even when accepted text is currently invalid.
    fn open(
        &mut self,
        session: &mut AnalysisSession,
        params: Open,
        request: &InputRequest,
    ) -> Result<SourceKey, Value> {
        let document = params.text_document;
        let path = source_key(session, &document.uri)?;
        self.check_query(session, &path)?;
        if session.query_input_at(&path).is_some() && document.language_id != "sdbl" {
            return Err(protocol::error_value(
                -32602,
                "document_language_invalid",
                Value::Null,
            ));
        }
        if document.language_id.is_empty() {
            return Err(protocol::error_value(
                -32602,
                "document_language_invalid",
                Value::Null,
            ));
        }
        if self.open.contains(&path) {
            return Err(protocol::error_value(
                -32602,
                "document_already_open",
                Value::Null,
            ));
        }
        overlay(session, &path, document.version, &document.text, request)?;
        self.open.insert(path.clone());
        Ok(path)
    }

    /// Only one full replacement is advertised; incremental edits must not erase current text.
    fn change(
        &self,
        session: &mut AnalysisSession,
        params: &Change,
        request: &InputRequest,
    ) -> Result<SourceKey, Value> {
        let path = self.require_open(session, &params.text_document.uri)?;
        let [content] = params.content_changes.as_slice() else {
            return Err(protocol::error_value(
                -32602,
                "full_document_change_required",
                Value::Null,
            ));
        };
        if content.range.is_some() || content.range_length.is_some() {
            return Err(protocol::error_value(
                -32602,
                "full_document_change_required",
                Value::Null,
            ));
        }
        overlay(
            session,
            &path,
            params.text_document.version,
            &content.text,
            request,
        )?;
        Ok(path)
    }

    /// Closing reads current saved bytes, including an explicit failure if the file disappeared.
    fn close(
        &mut self,
        session: &mut AnalysisSession,
        params: &Close,
        request: &InputRequest,
    ) -> Result<SourceKey, Value> {
        let path = self.require_open(session, &params.text_document.uri)?;
        let revision = session.revision();
        if session.owner_at(&path).is_some() {
            session.close_overlay_at(&path, revision, request)
        } else if session.metadata_resource_at(&path).is_some() {
            session.close_resource_overlay_at(&path, revision, request)
        } else if session.query_input_at(&path).is_some() {
            session.close_query_overlay_at(&path, revision, request)
        } else {
            session.close_module_overlay_at(&path, revision, request)
        }
        .map_err(|error| request_error(&error))?;
        self.open.remove(&path);
        Ok(path)
    }

    /// A change/save/close of an unopened document cannot silently reopen a saved input.
    fn require_open(&self, session: &AnalysisSession, uri: &str) -> Result<SourceKey, Value> {
        let path = source_key(session, uri)?;
        self.check_query(session, &path)?;
        if !self.open.contains(&path) {
            return Err(protocol::error_value(
                -32602,
                "document_not_open",
                Value::Null,
            ));
        }
        Ok(path)
    }

    /// Declared query identity never silently expands a client that did not negotiate syntax sync.
    fn check_query(&self, session: &AnalysisSession, path: &SourceKey) -> Result<(), Value> {
        if session.query_input_at(path).is_some() && !self.query_syntax && !self.query_analysis {
            return Err(query_syntax::not_negotiated());
        }
        Ok(())
    }
}

/// Decode a local file URI without reading it; the caller applies its source or control-file boundary.
pub fn file_path(uri: &str) -> Result<PathBuf, Value> {
    let parsed = Url::parse(uri)
        .map_err(|_| protocol::error_value(-32602, "document_uri_invalid", Value::Null))?;
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(protocol::error_value(
            -32602,
            "document_uri_invalid",
            Value::Null,
        ));
    }
    parsed
        .to_file_path()
        .map_err(|()| protocol::error_value(-32602, "document_uri_invalid", Value::Null))
}

/// Synchronization recognizes the exact declared owner of a URI across the admitted source set.
pub fn source_key(session: &AnalysisSession, uri: &str) -> Result<SourceKey, Value> {
    declared_key(session, &file_path(uri)?)
}

/// Root qualification keeps equal relative paths distinct without filesystem alias probing.
pub fn declared_key(session: &AnalysisSession, file: &Path) -> Result<SourceKey, Value> {
    for root in session.root_ids() {
        let Some(directory) = session.source_root(root) else {
            continue;
        };
        let Ok(relative) = file.strip_prefix(directory) else {
            continue;
        };
        let relative = relative
            .to_str()
            .ok_or_else(|| protocol::error_value(-32602, "document_uri_invalid", Value::Null))?;
        let source = SourcePath::parse(&relative.replace(std::path::MAIN_SEPARATOR, "/"))
            .map_err(|error| protocol::error_value(-32602, error.code, Value::Null))?;
        let key = SourceKey { root, source };
        if session.owner_at(&key).is_some()
            || session.module_at(&key).is_some()
            || session.metadata_resource_at(&key).is_some()
            || session.query_input_at(&key).is_some()
        {
            return Ok(key);
        }
        return Err(protocol::error_value(-32602, "unknown_source", Value::Null));
    }
    Err(protocol::error_value(
        -32602,
        "document_outside_source",
        Value::Null,
    ))
}

/// Resolve only declared inputs under the selected source, without probing filesystem aliases.
pub fn source_path(session: &AnalysisSession, uri: &str) -> Result<SourcePath, Value> {
    let file = file_path(uri)?;
    declared_path(session, &file)
}

/// Reuse the declared-input boundary after a batch has already decoded all local URIs.
pub fn declared_path(session: &AnalysisSession, file: &Path) -> Result<SourcePath, Value> {
    let relative = file
        .strip_prefix(session.root())
        .map_err(|_| protocol::error_value(-32602, "document_outside_source", Value::Null))?;
    let text = relative
        .to_str()
        .ok_or_else(|| protocol::error_value(-32602, "document_uri_invalid", Value::Null))?;
    let path = SourcePath::parse(&text.replace(std::path::MAIN_SEPARATOR, "/"))
        .map_err(|error| protocol::error_value(-32602, error.code, Value::Null))?;
    if session.owner(&path).is_none()
        && session.module(&path).is_none()
        && session.metadata_resource(&path).is_none()
        && session.query_input(&path).is_none()
    {
        return Err(protocol::error_value(-32602, "unknown_source", Value::Null));
    }
    Ok(path)
}

/// YAML, declared metadata resources and BSL enter the same revisioned session and byte budget.
fn overlay(
    session: &mut AnalysisSession,
    path: &SourceKey,
    version: i32,
    text: &str,
    request: &InputRequest,
) -> Result<(), Value> {
    let revision = session.revision();
    if session.owner_at(path).is_some() {
        session.apply_overlay_at(path, revision, i64::from(version), text, request)
    } else if session.metadata_resource_at(path).is_some() {
        session.apply_resource_overlay_at(path, revision, i64::from(version), text, request)
    } else if session.query_input_at(path).is_some() {
        session.apply_query_overlay_at(path, revision, i64::from(version), text, request)
    } else {
        session.apply_module_overlay_at(path, revision, i64::from(version), text, request)
    }
    .map_err(|error| request_error(&error))
}

/// Current input summaries omit the actual editor text; tree consumers request properties separately.
pub fn current(session: &AnalysisSession, path: &SourcePath) -> Value {
    current_at(session, &session.selected_source(path))
}

/// Contribution acknowledgements qualify current bytes without exposing their editor text.
pub fn current_at(session: &AnalysisSession, path: &SourceKey) -> Value {
    if let Ok(metadata) = session.metadata_at(path, session.revision()) {
        return json!({"kind":"metadata", "state":dto::state(&metadata)});
    }
    if let Some(module) = session.module_at(path) {
        return json!({"kind":"module", "state":dto::module(module)});
    }
    if let Some(input) = session.metadata_resource_at(path) {
        return json!({"kind":input.kind, "owner":session.resource_owner_at(path), "state":dto::metadata_resource(input)});
    }
    if let Some(input) = session.query_input_at(path) {
        return json!({"kind":"query","scope":"syntax_only","state":dto::query_text(input),
            "declaration":session.query_declaration_at(path)});
    }
    json!({"kind":"unavailable"})
}

/// Protocol rejection never hides a stale version or reclassifies it as accepted empty input.
pub fn request_error(error: &SessionRequestError) -> Value {
    let (code, kind, details) = match error {
        SessionRequestError::UnknownRoot(root) => (-32602, "unknown_source", json!({"root":root})),
        SessionRequestError::CompositionInputIncomplete(source) => (
            -32000,
            "analysis_composition_incomplete",
            json!({"source":source}),
        ),
        SessionRequestError::ContextRequired => (
            -32000,
            "analysis_context_required",
            json!({"scope":"local_syntax","required_scope":"canonical_configuration"}),
        ),
        SessionRequestError::ConfigurationContextRequired => (
            -32000,
            "analysis_configuration_context_required",
            json!({"analysis_scope":"canonical_external","metadata_context_complete":false,"required_context":"owning-configuration"}),
        ),
        SessionRequestError::StaleDocumentVersion { current, received }
        | SessionRequestError::Metadata(UpdateError::StaleDocumentVersion { current, received }) => {
            (
                -32801,
                "stale_document_version",
                json!({"current":current,"received":received}),
            )
        }
        SessionRequestError::OverlayActive { document_version }
        | SessionRequestError::Metadata(UpdateError::OverlayActive { document_version }) => (
            -32000,
            "overlay_active",
            json!({"document_version":document_version}),
        ),
        SessionRequestError::BinaryModule(_) => (-32602, "binary_module", Value::Null),
        SessionRequestError::UnknownModule(_)
        | SessionRequestError::UnknownQuery(_)
        | SessionRequestError::UnknownMetadataResource(_)
        | SessionRequestError::Metadata(UpdateError::UnknownOwner(_)) => {
            (-32602, "unknown_source", Value::Null)
        }
        SessionRequestError::RevisionExhausted
        | SessionRequestError::Metadata(UpdateError::RevisionExhausted) => {
            (-32603, "revision_exhausted", Value::Null)
        }
        SessionRequestError::StaleRevision { .. }
        | SessionRequestError::Metadata(UpdateError::StaleRevision { .. }) => {
            (-32801, "stale_revision", Value::Null)
        }
        SessionRequestError::QueryReaderMismatch => (-32603, "session_invariant", Value::Null),
    };
    protocol::error_value(code, kind, details)
}
