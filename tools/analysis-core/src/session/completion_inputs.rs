//! Native completion discovery selects current dependencies; the host retains source I/O and authority.

#[cfg(test)]
mod common_module_tests;
#[cfg(test)]
mod hover_tests;
#[cfg(test)]
mod manager_tests;
mod session_parameters;
#[cfg(test)]
mod signature_tests;
#[cfg(test)]
mod tests;

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest, InputState},
    project_core::source_format::SourcePath,
};
use ide_db::metadata::{common_module_index, config_index};

use super::{AnalysisSession, metadata_resources};
use crate::{
    ModuleFailure, ModuleState, QueryCancellation, QueryError, SessionRequestError, SessionRevision,
};

impl AnalysisSession {
    /// Prepare the module's own metadata and a direct manager/common-module target only.
    ///
    /// This also prepares the admitted target body, keeping other modules unread.
    /// Explicit compositions first prepare every topology-visible current contribution.
    /// It does not refresh ready or editor-owned input, load sibling dialogs, scan payloads,
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
        self.validate_preparation(path, expected, offset, request, cancellation)?;
        if self.composition_sha256.is_some() {
            self.load_semantic_inputs(self.revision(), request)
                .map_err(QueryError::Request)?;
        }
        self.prepare_module_metadata(path, request)?;
        self.prepare_session_parameters(path, offset, request, cancellation)?;
        if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        let owner = self
            .query_module(path, self.revision(), cancellation, |analysis, file| {
                let Some((kind, name)) = analysis.completion_metadata_target(file, offset) else {
                    return Ok(None);
                };
                self.require_own_external_target(kind, &name)?;
                let main = if kind == bsl_metadata::MdoType::CommonModule {
                    common_module_index(analysis.database(), self.selected_root.listing)
                        .lookup(&name)
                } else {
                    config_index(analysis.database(), self.selected_root.listing)
                        .lookup(kind, &name)
                        .map(|files| files.main)
                };
                let Some(main) = main else {
                    return Ok(None);
                };
                // Frozen identities are resident; resolving a FileId never examines owner payloads.
                Ok(self
                    .selected_root
                    .owners
                    .iter()
                    .find(|(_, entry)| entry.binding.main == main)
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
        let dependency = self
            .query_module(path, self.revision(), cancellation, |analysis, file| {
                let target = analysis
                    .completion_manager_module(file, offset)
                    .or_else(|| analysis.completion_common_module(file, offset));
                Ok(target.and_then(|file| {
                    self.selected_root
                        .modules
                        .iter()
                        .find(|(_, entry)| entry.file == file)
                        .map(|(path, _)| path.clone())
                }))
            })?
            .value;
        if let Some(dependency) = dependency {
            self.load_completion_module(&dependency, request)?;
        }
        if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        Ok(())
    }

    /// Prepare current own context and only an active member call's admitted target before read-only signature help.
    ///
    /// Argument expressions never select a different dependency; non-member or inactive calls perform no metadata reads.
    /// Explicit compositions first prepare all visible current contributions under the same request control.
    /// # Errors
    /// Rejects stale/unavailable caller input, invalid offset, cancellation or source publication failure.
    pub fn prepare_signature_help_metadata(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        offset: u32,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<(), QueryError> {
        self.validate_preparation(path, expected, offset, request, cancellation)?;
        if self.composition_sha256.is_some() {
            self.load_semantic_inputs(self.revision(), request)
                .map_err(QueryError::Request)?;
        }
        self.prepare_member_call_metadata(
            path,
            offset,
            request,
            cancellation,
            ide::Analysis::signature_help_dependency_offset,
        )
    }

    /// Prepare only a current hovered member callee's own context and admitted direct target before its read-only card.
    ///
    /// Non-callee tokens do not load metadata; native resolution retains ownership of symbols, docs and types.
    /// Explicit compositions first prepare all visible current contributions under the same request control.
    /// # Errors
    /// Rejects stale/unavailable caller input, invalid offset, cancellation or source publication failure.
    pub fn prepare_symbol_hover_metadata(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        offset: u32,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<(), QueryError> {
        self.validate_preparation(path, expected, offset, request, cancellation)?;
        if self.composition_sha256.is_some() {
            self.load_semantic_inputs(self.revision(), request)
                .map_err(QueryError::Request)?;
        }
        if self.selected_root.external_kind.is_some() {
            self.prepare_module_metadata(path, request)?;
        }
        self.prepare_member_call_metadata(
            path,
            offset,
            request,
            cancellation,
            ide::Analysis::symbol_hover_dependency_offset,
        )
    }

    /// Every preparation consumer rejects the same invalid input before reading a dependency or publishing metadata.
    fn validate_preparation(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        offset: u32,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<(), QueryError> {
        self.require_semantic_context(expected)
            .map_err(QueryError::Request)?;
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
        Ok(())
    }

    /// Signature and hover discovery choose their own native anchor and share the exact current-input publication path.
    fn prepare_member_call_metadata(
        &mut self,
        path: &SourcePath,
        offset: u32,
        request: &InputRequest,
        cancellation: &QueryCancellation,
        discover: fn(&ide::Analysis, vfs::FileId, u32) -> Option<u32>,
    ) -> Result<(), QueryError> {
        let dependency = self
            .query_module(path, self.revision(), cancellation, |analysis, file| {
                Ok(discover(analysis, file, offset))
            })?
            .value;
        if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        if let Some(dependency) = dependency {
            self.prepare_completion_metadata(
                path,
                self.revision(),
                dependency,
                request,
                cancellation,
            )?;
        }
        if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        Ok(())
    }

    /// Own metadata and the selected form dialog define the caller's native scope independently of its query kind.
    fn prepare_module_metadata(
        &mut self,
        path: &SourcePath,
        request: &InputRequest,
    ) -> Result<(), QueryError> {
        self.prepare_common_attributes(request)?;
        let module = self
            .selected_root
            .inputs
            .source_reader()
            .declared_module(path)
            .ok_or_else(|| QueryError::Request(SessionRequestError::UnknownModule(path.clone())))?;
        let owner = module.owner.clone();
        let dialog = self
            .selected_root
            .inputs
            .source_reader()
            .declared_form(module.native_descriptor)
            .and_then(|form| form.dialog)
            .cloned();
        self.load_completion_owner(&owner, request)?;
        if let Some(dialog) = dialog {
            self.load_completion_resource(&dialog, request)?;
        }
        let file = self
            .selected_root
            .modules
            .get(path)
            .ok_or_else(|| QueryError::Request(SessionRequestError::UnknownModule(path.clone())))?
            .file;
        self.require_common_attribute_membership(file)?;
        Ok(())
    }

    /// Current common membership can affect any object field, so cold native lookups need this family first.
    fn prepare_common_attributes(&mut self, request: &InputRequest) -> Result<(), QueryError> {
        let owners = self
            .selected_root
            .owners
            .iter()
            .filter(|(_, owner)| {
                owner.binding.kind == eska::project_core::MetadataKind::CommonAttribute
                    && !owner.binding.nested
            })
            .map(|(source, _)| source.clone())
            .collect::<Vec<_>>();
        for owner in owners {
            if request.cancellation.is_cancelled() {
                return Err(QueryError::Cancelled);
            }
            self.load_completion_owner(&owner, request)?;
            if !self
                .selected_root
                .owners
                .get(&owner)
                .is_some_and(|owner| matches!(owner.model.state, crate::ModelState::Ready(_)))
            {
                return Err(QueryError::Request(
                    SessionRequestError::CompositionInputIncomplete(self.selected_source(&owner)),
                ));
            }
        }
        Ok(())
    }

    /// Inspect input readiness rather than projection readiness so a pending resource cannot reread ready YAML.
    fn load_completion_owner(
        &mut self,
        path: &SourcePath,
        request: &InputRequest,
    ) -> Result<(), QueryError> {
        let Some(input) = self.selected_root.inputs.owner(path) else {
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
        let resource =
            metadata_resources::dependency(self.selected_root.inputs.source_reader(), path)
                .cloned();
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

    /// Only declared saved pending/transiently cancelled bodies may be read; editor and binary input stay authoritative.
    fn load_completion_module(
        &mut self,
        path: &SourcePath,
        request: &InputRequest,
    ) -> Result<(), QueryError> {
        if self.module(path).is_some_and(|input| {
            input.origin == InputOrigin::Saved
                && matches!(
                    input.state,
                    ModuleState::Pending
                        | ModuleState::Unavailable(ModuleFailure::Input(InputFailure::Cancelled))
                )
        }) && !request.cancellation.is_cancelled()
        {
            self.load_module_saved(path, self.revision(), request)
                .map_err(QueryError::Request)?;
        }
        Ok(())
    }
}
