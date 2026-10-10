//! Rename previews use one UTF-16 projection over captured current bytes, including editor version.

use serde_json::{Value, json};

use super::super::{dto, positions::Positions};
use super::{Options, language, protocol};
use crate::{AnalysisSession, LocalRenamePlan, LocalRenameTarget, QueryError};

/// Coverage is not promoted by local scope proof, and the exact policy remains visible to the client.
fn context(session: &AnalysisSession, target: &LocalRenameTarget, id: &str) -> Value {
    json!({"snapshot":dto::snapshot(session,id),"source":target.source,"source_sha256":target.source_sha256,
        "document_version":target.document_version,"profile":"local-rename","scope":"method-local-bsl",
        "scope_verified":true,"permission":target.permission,
        "coverage":{"metadata_models_complete":session.summary().descriptor_models_complete,
            "module_inputs_complete":session.module_summary().module_input_complete,"semantic_complete":false},
        "resources":session.resources()})
}

/// A prepare placeholder and range name the current occurrence rather than an earlier declaration.
pub(super) fn prepare(
    session: &AnalysisSession,
    target: &LocalRenameTarget,
    source_sha256: &str,
    options: &Options<'_>,
) -> Result<Value, Value> {
    if target.source_sha256 != source_sha256 {
        return Err(protocol::error_value(
            -32603,
            "rename_inconsistent",
            Value::Null,
        ));
    }
    session.with_module_text(&target.source,target.revision,&options.cancellation.query,|text| {
        let positions = Positions::new(text,&[target.start_byte,target.end_byte])?;
        Ok(json!({"range":positions.range(target.start_byte,target.end_byte)?,"placeholder":target.symbol,
            "eska":context(session,target,options.session_id)}))
    }).map_err(|error|language::query_error(session,options.session_id,&error))?.value
}

/// A versioned `WorkspaceEdit` has no server write authority or unversioned fallback.
pub(super) fn rename(
    session: &AnalysisSession,
    plan: &LocalRenamePlan,
    options: &Options<'_>,
) -> Result<Value, Value> {
    let target = &plan.target;
    let version = i32::try_from(target.document_version).map_err(|_| {
        protocol::error_value(-32603, "rename_document_version_unsupported", Value::Null)
    })?;
    let uri = url::Url::from_file_path(session.root().join(target.source.as_path()))
        .map_err(|()| protocol::error_value(-32603, "path_encoding_unsupported", Value::Null))?;
    let offsets = plan
        .occurrences
        .iter()
        .flat_map(|hit| [hit.start_byte, hit.end_byte])
        .collect::<Vec<_>>();
    session.with_module_text(&target.source,target.revision,&options.cancellation.query,|text| {
        let positions = Positions::new(text,&offsets)?;
        let mut edits = Vec::with_capacity(plan.occurrences.len());
        for hit in &plan.occurrences {
            if options.cancellation.query.is_cancelled() { return Err(language::query_error(session,options.session_id,&QueryError::Cancelled)); }
            edits.push(json!({"range":positions.range(hit.start_byte,hit.end_byte)?,"newText":plan.new_name}));
        }
        let mut context = context(session,target,options.session_id);
        context["occurrences"] = json!(edits.len());
        context["reference_search_complete"] = json!(true);
        context["unchanged"] = json!(edits.is_empty());
        Ok(json!({"documentChanges":[{"textDocument":{"uri":uri.as_str(),"version":version},"edits":edits}],"eska":context}))
    }).map_err(|error|language::query_error(session,options.session_id,&error))?.value
}
