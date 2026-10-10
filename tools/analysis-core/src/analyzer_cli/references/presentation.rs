//! Map every target file once while preserving the native occurrence order across files.

use super::super::{language, positions::Positions, protocol};
use super::{Options, Value, json};
use crate::{AnalysisSession, QueryError, ReferenceOccurrence};
use eska::project_core::source_format::SourcePath;
use std::collections::BTreeMap;

/// Groups are request-local coordinate work, not a persistent semantic index or text cache.
pub(super) fn render(
    session: &AnalysisSession,
    occurrences: &[ReferenceOccurrence],
    context: &Value,
    options: &Options<'_>,
) -> Result<Value, Value> {
    let mut files = BTreeMap::<&SourcePath, Vec<(usize, &ReferenceOccurrence)>>::new();
    for (index, occurrence) in occurrences.iter().enumerate() {
        if options.cancellation.query.is_cancelled() {
            return Err(language::query_error(
                session,
                options.session_id,
                &QueryError::Cancelled,
            ));
        }
        files
            .entry(&occurrence.target.source)
            .or_default()
            .push((index, occurrence));
    }
    let mut locations = vec![Value::Null; occurrences.len()];
    for (source, hits) in files {
        let uri =
            url::Url::from_file_path(session.root().join(source.as_path())).map_err(|()| {
                protocol::error_value(-32603, "path_encoding_unsupported", Value::Null)
            })?;
        let offsets = hits
            .iter()
            .flat_map(|(_, hit)| [hit.target.start_byte, hit.target.end_byte])
            .collect::<Vec<_>>();
        session.with_module_text(source, session.revision(), &options.cancellation.query, |text| {
            let positions = Positions::new(text, &offsets)?;
            for (index, occurrence) in &hits {
                if options.cancellation.query.is_cancelled() { return Err(language::query_error(session,options.session_id,&QueryError::Cancelled)); }
                let target = &occurrence.target;
                let mut context = context.clone();
                context["kind"] = json!(occurrence.kind);
                context["target"] = json!({"source":target.source,"source_sha256":target.source_sha256,"start_byte":target.start_byte,"end_byte":target.end_byte});
                locations[*index] = json!({"uri":uri.as_str(),"range":positions.range(target.start_byte,target.end_byte)?,"eska":context});
            }
            Ok(())
        }).map_err(|error| language::query_error(session, options.session_id, &error))?.value?;
    }
    Ok(json!(locations))
}
