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
        self.require_configuration_context(expected)?;
        let roots = self.visible_root_ids(self.selected_root_id());
        for root in roots {
            self.load_root_semantic_inputs(root, request)?;
            if request.cancellation.is_cancelled() {
                break;
            }
        }
        Ok(())
    }

    /// Prepare the exact current contributions of one visible root without refreshing editor authority.
    fn load_root_semantic_inputs(
        &mut self,
        id: crate::composition::RootId,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.load_root_metadata_inputs(id, request)?;
        self.load_root_module_inputs(id, request)
    }

    /// Query-only consumers prepare the same metadata inputs without opening BSL bodies.
    pub(crate) fn load_root_metadata_inputs(
        &mut self,
        id: crate::composition::RootId,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        use crate::{MetadataResourceState, composition::SourceKey};
        let resources = self
            .root_inputs(id)?
            .metadata_resources
            .iter()
            .filter(|(_, entry)| {
                entry.input.origin == InputOrigin::Saved
                    && matches!(
                        entry.input.state,
                        MetadataResourceState::Pending
                            | MetadataResourceState::Unavailable(InputFailure::Cancelled)
                    )
            })
            .map(|(source, _)| SourceKey {
                root: id,
                source: source.clone(),
            })
            .collect::<Vec<_>>();
        for key in resources {
            if request.cancellation.is_cancelled() {
                return Ok(());
            }
            self.load_resource_saved_at(&key, self.revision(), request)?;
        }
        let owners = self
            .root_inputs(id)?
            .owners
            .iter()
            .filter(|(_, entry)| {
                entry.model.origin == InputOrigin::Saved
                    && matches!(
                        entry.model.state,
                        ModelState::Pending | ModelState::Unavailable(InputFailure::Cancelled)
                    )
            })
            .map(|(source, _)| SourceKey {
                root: id,
                source: source.clone(),
            })
            .collect::<Vec<_>>();
        for key in owners {
            if request.cancellation.is_cancelled() {
                return Ok(());
            }
            self.load_saved_at(&key, self.revision(), request)?;
        }
        Ok(())
    }

    /// Body preparation stays shared with existing semantic requests after their metadata prefix.
    fn load_root_module_inputs(
        &mut self,
        id: crate::composition::RootId,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        use crate::composition::SourceKey;
        let modules = self
            .root_inputs(id)?
            .modules
            .iter()
            .filter(|(_, entry)| {
                entry.model.origin == InputOrigin::Saved
                    && matches!(
                        entry.model.state,
                        ModuleState::Pending
                            | ModuleState::Unavailable(ModuleFailure::Input(
                                InputFailure::Cancelled
                            ))
                    )
            })
            .map(|(source, _)| SourceKey {
                root: id,
                source: source.clone(),
            })
            .collect::<Vec<_>>();
        for key in modules {
            if request.cancellation.is_cancelled() {
                return Ok(());
            }
            self.load_module_saved_at(&key, self.revision(), request)?;
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
            .selected_root
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
