//! Native receiver discovery selects criteria; the ordinary current input controller owns every read.

use super::{
    AnalysisSession, InputRequest, QueryCancellation, QueryError, SessionRequestError, SourcePath,
};

impl AnalysisSession {
    /// Prepare the declared family before a named manager or Find result is discovered, without refreshing dirty input.
    pub(super) fn prepare_filter_criteria(
        &mut self,
        path: &SourcePath,
        offset: u32,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<(), QueryError> {
        let needed = self
            .query_module(path, self.revision(), cancellation, |analysis, file| {
                Ok(analysis.completion_uses_filter_criteria(file, offset))
            })?
            .value;
        if !needed {
            return Ok(());
        }
        let owners = self
            .selected_root
            .owners
            .iter()
            .filter(|(_, entry)| {
                entry.binding.kind == eska::project_core::MetadataKind::FilterCriterion
                    && !entry.binding.nested
            })
            .map(|(source, _)| source.clone())
            .collect::<Vec<_>>();
        for source in owners {
            if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
                return Err(QueryError::Cancelled);
            }
            self.load_completion_owner(&source, request)?;
        }
        let file = self
            .selected_root
            .modules
            .get(path)
            .ok_or_else(|| QueryError::Request(SessionRequestError::UnknownModule(path.clone())))?
            .file;
        self.require_filter_criteria_context(&self.database, file)
    }
}
