//! Protocol DTOs expose logical metadata and current source locations, never kernel tables or ASTs.

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputState},
    project_core::{
        metadata_parser::ParsedObject,
        source_format::{NativeMetadata, SourcePath},
    },
};
use serde_json::{Value, json};

use crate::{
    AnalysisSession, MetadataResourceInput, MetadataResourceState, MetadataView, ModelState,
    ModuleFailure, ModuleInput, ModuleState,
};

/// The transport's fresh identity disambiguates equal revisions from different server processes.
pub fn snapshot(session: &AnalysisSession, id: &str) -> Value {
    json!({"session_id":id, "revision":session.revision().to_string()})
}

/// Owner pages report current readiness without loading any YAML or inventing a semantic model.
pub fn owner(session: &AnalysisSession, path: &SourcePath) -> Result<Value, Value> {
    let view = session
        .metadata(path, session.revision())
        .map_err(|_| super::protocol::error_value(-32603, "session_invariant", Value::Null))?;
    Ok(json!({"source":path, "native_path":session.native_path(path), "state":state(&view)}))
}

/// Logical children and typed properties retain exact YAML byte ranges and explicit parse issues.
pub fn metadata(view: &MetadataView) -> Value {
    let documents = match &view.input {
        InputState::Ready(input) => input.documents.iter().map(document).collect::<Vec<_>>(),
        InputState::Pending | InputState::Unavailable(_) => Vec::new(),
    };
    json!({"source":view.source, "state":state(view),
        "location_encoding":"utf-8-bytes", "documents":documents})
}

/// Keep descriptor grouping and references without exposing parser implementation fields.
fn document(document: &NativeMetadata) -> Value {
    let objects = document
        .descriptor
        .objects
        .iter()
        .map(object)
        .collect::<Vec<_>>();
    json!({"native_path":document.path, "objects":objects,
        "references":document.descriptor.references,
        "parse_issues":document.descriptor.diagnostics})
}

/// An object's logical identity and property locations refer to the same current YAML snapshot.
fn object(object: &ParsedObject) -> Value {
    json!({"id":object.metadata.id(), "kind":object.metadata.kind().as_str(),
        "name":object.metadata.name(), "uuid":object.metadata.uuid(),
        "parent":object.metadata.parent(), "children":object.children,
        "synonyms":object.synonyms, "range":object.range,
        "properties":object.properties})
}

/// A ready source can still have an unsupported/invalid semantic adapter or incomplete children.
pub fn state(view: &MetadataView) -> Value {
    let origin = origin(view.origin);
    let input = match &view.input {
        InputState::Pending => json!({"kind":"pending"}),
        InputState::Ready(_) => json!({"kind":"ready"}),
        InputState::Unavailable(failure) => {
            json!({"kind":"unavailable", "failure":failure_value(failure)})
        }
    };
    let issue = model_issue(
        &view.model.state,
        matches!(view.input, InputState::Ready(_)),
    );
    let mut value = json!({"origin":origin, "source_sha256":view.source_sha256, "input":input,
        "model":model_kind(&view.model.state), "model_issue":issue, "descriptors":view.model.descriptors,
        "modeled_descriptors":view.model.modeled_descriptors, "parse_issues":view.model.parse_issues});
    if !view.model.forms.is_empty() {
        value["forms"] = json!(view.model.forms.iter().map(|form| {
            json!({"native_path":form.native_path,"dialog":form.dialog,"revision":form.revision.to_string(),
                "model":model_kind(&form.state),"model_issue":model_issue(&form.state, matches!(view.input, InputState::Ready(_)))})
        }).collect::<Vec<_>>());
    }
    value
}

/// Leaf models expose their own failure while the enclosing main object can remain ready.
fn model_issue(state: &ModelState, input_ready: bool) -> Option<Value> {
    match state {
        ModelState::Invalid { error, .. } => Some(
            json!({"code":"analysis_adapter_invalid", "range":error.range,
                "issue":crate::tool_metadata::adapter_cause(&error.issue)}),
        ),
        ModelState::Unsupported(kind) => {
            Some(json!({"code":"analysis_metadata_kind_unsupported", "kind":kind.as_str()}))
        }
        ModelState::Unavailable(failure) if input_ready => Some(
            json!({"code":"analysis_metadata_dependency_unavailable", "failure":failure_value(failure)}),
        ),
        _ => None,
    }
}

/// Ready raw input and a successful semantic projection use separate protocol fields.
const fn model_kind(state: &ModelState) -> &'static str {
    match state {
        ModelState::Pending => "pending",
        ModelState::Ready(_) => "ready",
        ModelState::StructureOnly => "structure_only",
        ModelState::Unsupported(_) => "unsupported",
        ModelState::Unavailable(_) => "unavailable",
        ModelState::Invalid { .. } => "invalid",
    }
}

/// A module's current readability and byte count do not imply clean syntax or full analysis.
pub fn module(input: &ModuleInput) -> Value {
    let state = match &input.state {
        ModuleState::Pending => json!({"kind":"pending"}),
        ModuleState::Ready { bytes } => json!({"kind":"ready", "bytes":bytes}),
        ModuleState::Binary => json!({"kind":"binary"}),
        ModuleState::Unavailable(ModuleFailure::Input(failure)) => {
            json!({"kind":"unavailable", "failure":failure_value(failure)})
        }
        ModuleState::Unavailable(ModuleFailure::LoadedTextBudgetExceeded { limit }) => {
            json!({"kind":"unavailable", "failure":{
                "code":"analysis_module_text_budget_exceeded", "limit":limit}})
        }
    };
    json!({"origin":origin(input.origin), "source_sha256":input.source_sha256, "input":state})
}

/// Resource readiness describes raw bytes; its owner reports native parse failure separately.
pub fn metadata_resource(input: &MetadataResourceInput) -> Value {
    let state = match &input.state {
        MetadataResourceState::Pending => json!({"kind":"pending"}),
        MetadataResourceState::Ready(text) => json!({"kind":"ready","bytes":text.len()}),
        MetadataResourceState::Unavailable(failure) => {
            json!({"kind":"unavailable","failure":failure_value(failure)})
        }
    };
    json!({"origin":origin(input.origin),"source_sha256":input.source_sha256,"input":state})
}

/// Metadata and module summaries use the same editor authority and document-version contract.
pub(super) fn origin(origin: InputOrigin) -> Value {
    match origin {
        InputOrigin::Saved => json!({"kind":"saved"}),
        InputOrigin::Overlay { document_version } => {
            json!({"kind":"overlay", "document_version":document_version})
        }
    }
}

/// Unavailable source remains a coded result for the current revision, never an empty success.
pub(super) fn failure_value(failure: &InputFailure) -> Value {
    match failure {
        InputFailure::Source(error) => json!({"code":error.code, "location":error.location}),
        InputFailure::Cancelled => json!({"code":"analysis_cancelled"}),
        InputFailure::DocumentTooLarge { limit } => {
            json!({"code":"analysis_document_too_large", "limit":limit})
        }
        InputFailure::SourceBudgetExceeded { limit } => {
            json!({"code":"analysis_source_budget_exceeded", "limit":limit})
        }
    }
}
