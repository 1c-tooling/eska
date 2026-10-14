//! Semantic consumers share readiness and retry policy instead of discovering a second input set.

#[cfg(test)]
mod tests;

use eska::analysis_core::{InputFailure, InputOrigin, InputRequest};

use crate::{
    AnalysisSession, ModelState, ModuleFailure, ModuleState, SessionRequestError, SessionRevision,
};

impl AnalysisSession {
    /// Load pending or transiently cancelled saved YAML, metadata resources and BSL before cross-module queries.
    ///
    /// Ready inputs and editor-owned failures remain authoritative until an explicit change.
    /// Cancellation leaves unvisited inputs pending; callers inspect current coverage and stop state.
    ///
    /// # Errors
    /// Returns a stale revision or an input publication failure; no stale request reads source bytes.
    pub fn load_semantic_inputs(
        &mut self,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.require_semantic_context(expected)?;
        self.load_pending_metadata_inputs(request)?;
        let paths = self
            .modules
            .iter()
            .filter(|(_, entry)| {
                let input = &entry.model;
                input.origin == InputOrigin::Saved
                    && matches!(
                        input.state,
                        ModuleState::Pending
                            | ModuleState::Unavailable(ModuleFailure::Input(
                                InputFailure::Cancelled
                            ))
                    )
            })
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        for path in paths {
            if request.cancellation.is_cancelled() {
                break;
            }
            self.load_module_saved(&path, self.revision(), request)?;
        }
        Ok(())
    }

    /// Independent syntax batches use the same pending metadata policy without loading other bodies.
    pub(crate) fn load_pending_metadata_inputs(
        &mut self,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.load_pending_resources(request)?;
        let paths = self
            .owners
            .iter()
            .filter(|(_, entry)| {
                let input = &entry.model;
                input.origin == InputOrigin::Saved
                    && matches!(
                        input.state,
                        ModelState::Pending | ModelState::Unavailable(InputFailure::Cancelled)
                    )
            })
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        for path in paths {
            if request.cancellation.is_cancelled() {
                break;
            }
            self.load_saved(&path, self.revision(), request)?;
        }
        Ok(())
    }
}
