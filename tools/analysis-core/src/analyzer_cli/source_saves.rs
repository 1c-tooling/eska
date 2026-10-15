//! A selected open draft is published once under independent policy and exact-byte disk preconditions.

use std::{fs, io::Read, path::Path, time::Duration};

use eska::project_core::source_format::{
    AcceptedBase, SourcePath, prepare_module_edit, prepare_query_edit, prepare_yaml_edit,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AnalysisSession, RequestCancellation, SourceSaveError, SourceSaveInput, SourceSaveKind,
};

use super::{
    documents::{self, Documents},
    dto,
    edit_authority::Authority,
    protocol, request_control,
};

pub const METHOD: &str = "eska/source/save";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub snapshot: protocol::Snapshot,
    source: String,
    document_version: i32,
    draft_sha256: String,
    expected_disk_sha256: String,
    text: String,
}

/// A committed result cannot be replaced by late read-query cancellation or import fencing.
pub struct Committed {
    pub value: Value,
    pub root_publication: Option<RootPublication>,
}

/// One confirmed root-property publication explains its own file event without granting write authority.
pub struct RootPublication {
    source: SourcePath,
    sha256: String,
}

impl RootPublication {
    /// This fingerprint is evidence of one successful typed root publication, never permission approval.
    pub fn fingerprint(&self) -> (&SourcePath, &str) {
        (&self.source, &self.sha256)
    }

    /// Only the same safely read selected root bytes may use the ordinary saved-overlay watch path.
    pub fn matches(&self, session: &AnalysisSession, file: &Path) -> bool {
        self.source
            == session
                .selected_root
                .inputs
                .source_reader()
                .configuration()
                .path
            && file == session.root().join(self.source.as_path())
            && read_saved(
                session.root(),
                &self.source,
                &RequestCancellation::default(),
            )
            .is_ok_and(|text| crate::source_saves::sha256(&text) == self.sha256)
    }
}

pub struct Options<'a> {
    pub id: &'a str,
    pub cancellation: &'a RequestCancellation,
    pub timeout: Duration,
}

/// Consumers must use this mutating method rather than treat save as a retriable language query.
pub fn capability() -> Value {
    json!({"method":METHOD,"scope":"selected-open-document", "sourceKinds":["yaml","bsl","sdbl"],
        "maxRequestBytes":documents::MAX_REQUEST,"cancellation":"before-commit"})
}

/// Validate editor authority before baseline I/O, then preserve it after exact-byte publication.
pub fn save(
    authority: &Authority,
    session: &AnalysisSession,
    documents: &Documents,
    params: &Params,
    options: &Options<'_>,
) -> Result<Committed, Value> {
    let Options {
        id,
        cancellation,
        timeout,
    } = *options;
    let method = request_control::Method::SourceSave;
    let deadline = request_control::start(session, id, method, cancellation, timeout)?;
    let result = (|| {
        let source = SourcePath::parse(&params.source)
            .map_err(|error| protocol::error_value(-32602, error.code, Value::Null))?;
        if params.text.len() > documents::MAX_REQUEST
            || !crate::source_saves::valid_sha256(&params.expected_disk_sha256)
        {
            return Err(protocol::error_value(
                -32602,
                "source_save_input_invalid",
                Value::Null,
            ));
        }
        documents.require_selected_open(session, &source)?;
        let input = session
            .source_save_input(
                &source,
                session.revision(),
                i64::from(params.document_version),
                &params.draft_sha256,
                &params.text,
            )
            .map_err(input_error)?;
        let base = authority.open(cancellation)?;
        let actual = read_saved(session.root(), &source, cancellation)?;
        let actual_sha256 = crate::source_saves::sha256(&actual);
        let outcome = if actual_sha256 == input.disk_sha256() {
            "already_current"
        } else if actual_sha256 == params.expected_disk_sha256 {
            "published"
        } else {
            return Err(conflict());
        };
        let updated = if outcome == "already_current" {
            actual.as_str()
        } else {
            input.text()
        };
        // The response is small and serializable before mutation; it never retains source text.
        let receipt = json!({"snapshot":dto::snapshot(session,id),"source":input.source(),
            "document_version":input.document_version(),"draft_sha256":input.draft_sha256(),
            "disk_sha256":input.disk_sha256(),"outcome":outcome});
        publish(session.root(), &base, &input, &actual, updated, &|| {
            deadline.stop_reason().is_some()
        })?;
        let root_publication = (outcome == "published"
            && input.kind() == SourceSaveKind::Yaml
            && input.source()
                == &session
                    .selected_root
                    .inputs
                    .source_reader()
                    .configuration()
                    .path)
            .then(|| RootPublication {
                source: input.source().clone(),
                sha256: input.disk_sha256().to_owned(),
            });
        Ok(Committed {
            value: receipt,
            root_publication,
        })
    })();
    match result {
        Ok(committed) => Ok(committed),
        Err(error) => {
            request_control::finish(session, id, method, &deadline, Err(error)).map(|value| {
                Committed {
                    value,
                    root_publication: None,
                }
            })
        }
    }
}

/// Existing core planners retain policy, aliases, current bindings and the final byte comparison.
fn publish(
    root: &Path,
    base: &AcceptedBase,
    input: &SourceSaveInput<'_>,
    original: &str,
    updated: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), Value> {
    let result = match input.kind() {
        SourceSaveKind::Yaml => prepare_yaml_edit(root, base, input.source(), original, updated)
            .and_then(|plan| plan.publish_cancellable(base, cancelled)),
        SourceSaveKind::Bsl => {
            prepare_module_edit(root, base, input.source(), Some(original), updated)
                .and_then(|plan| plan.publish_cancellable(base, cancelled))
        }
        SourceSaveKind::Sdbl => {
            prepare_query_edit(root, base, input.source(), updated).and_then(|plan| {
                // Query preparation reads its own saved original. Never rebase this save onto a concurrent write.
                if plan.original() != Some(original) {
                    return Err(eska::project_core::source_format::Error {
                        code: "source_edit_conflict",
                        location: Vec::new(),
                    });
                }
                plan.publish_cancellable(base, cancelled)
            })
        }
    };
    result.map_err(|error| {
        protocol::error_value(-32000, error.code, json!({"location":error.location}))
    })
}

/// Raw disk fingerprints include BOM and CRLF; every component remains a non-aliased declared path.
fn read_saved(
    root: &Path,
    source: &SourcePath,
    cancellation: &RequestCancellation,
) -> Result<String, Value> {
    let failure = |kind| protocol::error_value(-32000, kind, json!({"source":source}));
    if !fs::symlink_metadata(root).is_ok_and(|metadata| metadata.file_type().is_dir())
        || root.canonicalize().ok().as_deref() != Some(root)
    {
        return Err(failure("source_edit_unsafe_path"));
    }
    let mut path = root.to_owned();
    for component in source.as_path().components() {
        if cancellation.is_cancelled() {
            return Err(failure("source_read_cancelled"));
        }
        path.push(component);
        let metadata = fs::symlink_metadata(&path).map_err(|_| failure("source_read_failed"))?;
        let valid = if path == root.join(source.as_path()) {
            metadata.file_type().is_file()
        } else {
            metadata.file_type().is_dir()
        };
        if !valid {
            return Err(failure("source_edit_unsafe_path"));
        }
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| failure("source_read_failed"))?
        .take(documents::MAX_REQUEST as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| failure("source_read_failed"))?;
    if bytes.len() > documents::MAX_REQUEST {
        return Err(failure("source_save_too_large"));
    }
    if cancellation.is_cancelled() {
        return Err(failure("source_read_cancelled"));
    }
    String::from_utf8(bytes).map_err(|_| failure("source_text_not_utf8"))
}

/// The hash pair detects concurrent disk edits without trusting a stale expected revision.
fn conflict() -> Value {
    protocol::error_value(-32801, "source_edit_conflict", Value::Null)
}

/// Save errors remain machine-readable and retain the existing current-input error taxonomy.
fn input_error(error: SourceSaveError) -> Value {
    match error {
        SourceSaveError::Request(error) => documents::request_error(&error),
        SourceSaveError::DocumentVersion { current, received } => protocol::error_value(
            -32801,
            "source_edit_document_version",
            json!({"current":current,"received":received}),
        ),
        SourceSaveError::SourceConflict => {
            protocol::error_value(-32801, "source_edit_input_conflict", Value::Null)
        }
        other => protocol::error_value(
            -32000,
            match other {
                SourceSaveError::UnsupportedSource => "source_edit_undeclared",
                SourceSaveError::InputUnavailable => "source_edit_input_unavailable",
                SourceSaveError::DocumentNotOpen => "source_edit_document_not_open",
                SourceSaveError::InvalidHash => "source_save_hash_invalid",
                SourceSaveError::UnsupportedBom => "source_save_yaml_bom_unsupported",
                _ => "source_save_input_invalid",
            },
            Value::Null,
        ),
    }
}
