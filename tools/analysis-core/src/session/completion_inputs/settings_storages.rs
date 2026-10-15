//! Native receiver discovery selects settings storages; the ordinary current input controller owns every read.

use super::{
    AnalysisSession, InputRequest, QueryCancellation, QueryError, SessionRequestError, SourcePath,
};

impl AnalysisSession {
    /// Prepare the declared family before a named storage manager is discovered, without refreshing dirty input.
    pub(super) fn prepare_settings_storages(
        &mut self,
        path: &SourcePath,
        offset: u32,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<(), QueryError> {
        let needed = self
            .query_module(path, self.revision(), cancellation, |analysis, file| {
                Ok(analysis.completion_uses_settings_storages(file, offset))
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
                entry.binding.kind == eska::project_core::MetadataKind::SettingsStorage
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
        self.require_settings_storages_context(&self.database, file)?;
        self.prepare_settings_storage_manager_bodies(path, offset, request, cancellation)
    }

    /// Native receiver discovery selects exact enrolled bodies; editor-owned input stays authoritative.
    fn prepare_settings_storage_manager_bodies(
        &mut self,
        path: &SourcePath,
        offset: u32,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<(), QueryError> {
        use crate::{ModuleFailure, ModuleState, composition::SourceKey};
        use eska::analysis_core::{InputFailure, InputOrigin};

        let files = self
            .query_module(path, self.revision(), cancellation, |analysis, file| {
                Ok(analysis.completion_settings_storage_manager_modules(file, offset))
            })?
            .value;
        let roots = self.visible_root_ids(self.selected_root_id());
        let mut keys = Vec::new();
        for file in files {
            let mut found = None;
            for root in &roots {
                if let Some((source, _)) = self
                    .root_inputs(*root)
                    .map_err(QueryError::Request)?
                    .modules
                    .iter()
                    .find(|(_, entry)| entry.file == file)
                {
                    found = Some(SourceKey {
                        root: *root,
                        source: source.clone(),
                    });
                    break;
                }
            }
            keys.push(found.ok_or(QueryError::Request(
                SessionRequestError::QueryReaderMismatch,
            ))?);
        }
        for key in keys {
            if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
                return Err(QueryError::Cancelled);
            }
            if self.module_at(&key).is_some_and(|input| {
                input.origin == InputOrigin::Saved
                    && matches!(
                        input.state,
                        ModuleState::Pending
                            | ModuleState::Unavailable(ModuleFailure::Input(
                                InputFailure::Cancelled
                            ))
                    )
            }) {
                self.load_module_saved_at(&key, self.revision(), request)
                    .map_err(QueryError::Request)?;
            }
        }
        Ok(())
    }
}
