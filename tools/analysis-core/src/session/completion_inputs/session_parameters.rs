//! The native receiver chooses this family; source I/O remains in the existing input controller.

use super::{AnalysisSession, InputRequest, QueryCancellation, QueryError, SourcePath};

impl AnalysisSession {
    /// Cold configured members require only the declared session-value family, with current failures retained.
    pub(super) fn prepare_session_parameters(
        &mut self,
        path: &SourcePath,
        offset: u32,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<(), QueryError> {
        let uses_values = self
            .query_module(path, self.revision(), cancellation, |analysis, file| {
                Ok(analysis.completion_uses_session_parameters(file, offset))
            })?
            .value;
        if !uses_values {
            return Ok(());
        }
        let owners = self
            .selected_root
            .owners
            .iter()
            .filter(|(_, owner)| {
                owner.binding.kind == eska::project_core::MetadataKind::SessionParameter
                    && !owner.binding.nested
            })
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        for owner in owners {
            if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
                return Err(QueryError::Cancelled);
            }
            self.load_completion_owner(&owner, request)?;
        }
        Ok(())
    }
}
