//! Native completion discovery selects current dependencies; the host retains source I/O and authority.

#[cfg(test)]
mod tests;

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest, InputState},
    project_core::source_format::SourcePath,
};
use ide_db::metadata::config_index;

use super::{AnalysisSession, metadata_resources};
use crate::{QueryCancellation, QueryError, SessionRequestError, SessionRevision};

impl AnalysisSession {
    /// Prepare the module's own metadata and an admitted direct manager-access target only.
    ///
    /// This does not refresh ready or editor-owned input, load sibling dialogs, scan payloads,
    /// or claim transitive metadata inference is complete. The read-only query stays separate.
    /// # Errors
    /// Rejects stale/unknown/unavailable module input, an invalid offset, cancellation or publication failure.
    pub fn prepare_completion_metadata(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        offset: u32,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<(), QueryError> {
        if !self
            .with_module_text(path, expected, cancellation, |text| {
                text.is_char_boundary(offset as usize)
            })?
            .value
        {
            return Err(QueryError::InvalidOffset { offset });
        }
        if request.cancellation.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        let module = self
            .inputs
            .source_reader()
            .declared_module(path)
            .ok_or_else(|| QueryError::Request(SessionRequestError::UnknownModule(path.clone())))?;
        let owner = module.owner.clone();
        let dialog = self
            .inputs
            .source_reader()
            .declared_form(module.native_descriptor)
            .and_then(|form| form.dialog)
            .cloned();
        self.load_completion_owner(&owner, request)?;
        if let Some(dialog) = dialog {
            self.load_completion_resource(&dialog, request)?;
        }
        if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        let owner = self
            .query_module(path, self.revision(), cancellation, |analysis, file| {
                let Some((kind, name)) = analysis.completion_metadata_target(file, offset) else {
                    return Ok(None);
                };
                let Some(files) =
                    config_index(analysis.database(), self.listing).lookup(kind, &name)
                else {
                    return Ok(None);
                };
                // Frozen identities are resident; resolving a FileId never examines owner payloads.
                Ok(self
                    .owners
                    .iter()
                    .find(|(_, entry)| entry.binding.main == files.main)
                    .map(|(path, _)| path.clone()))
            })?
            .value;
        if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        if let Some(owner) = owner {
            self.load_completion_owner(&owner, request)?;
        }
        if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        Ok(())
    }

    /// Inspect input readiness rather than projection readiness so a pending resource cannot reread ready YAML.
    fn load_completion_owner(
        &mut self,
        path: &SourcePath,
        request: &InputRequest,
    ) -> Result<(), QueryError> {
        let Some(input) = self.inputs.owner(path) else {
            return Ok(());
        };
        if input.origin == InputOrigin::Saved
            && matches!(
                input.state,
                InputState::Pending | InputState::Unavailable(InputFailure::Cancelled)
            )
            && !request.cancellation.is_cancelled()
        {
            self.load_saved(path, self.revision(), request)
                .map_err(QueryError::Request)?;
        }
        let resource = metadata_resources::dependency(self.inputs.source_reader(), path).cloned();
        if let Some(resource) = resource {
            self.load_completion_resource(&resource, request)?;
        }
        Ok(())
    }

    /// Preserve ready/editor-owned failure authority while retrying only transient saved cancellation.
    fn load_completion_resource(
        &mut self,
        path: &SourcePath,
        request: &InputRequest,
    ) -> Result<(), QueryError> {
        if self.metadata_resource(path).is_some_and(|input| {
            input.origin == InputOrigin::Saved
                && matches!(
                    input.state,
                    crate::MetadataResourceState::Pending
                        | crate::MetadataResourceState::Unavailable(InputFailure::Cancelled)
                )
        }) && !request.cancellation.is_cancelled()
        {
            self.load_resource_saved(path, self.revision(), request)
                .map_err(QueryError::Request)?;
        }
        Ok(())
    }
}
