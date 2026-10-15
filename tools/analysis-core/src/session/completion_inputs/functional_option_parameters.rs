//! The native interface getter prepares only configured parameter declarations and their current Use owners.

use super::{
    AnalysisSession, InputRequest, QueryCancellation, QueryError, SourcePath, config_index,
};

impl AnalysisSession {
    /// Current interface parameter shapes do not authorize unrelated option/storage or hidden-root reads.
    pub(super) fn prepare_functional_option_parameters(
        &mut self,
        path: &SourcePath,
        offset: u32,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<(), QueryError> {
        let needed = self
            .query_module(path, self.revision(), cancellation, |analysis, file| {
                Ok(analysis.interface_functional_option_parameters_dependency(file, offset))
            })?
            .value;
        if !needed {
            return Ok(());
        }
        let sources = self
            .selected_root
            .owners
            .iter()
            .filter(|(_, entry)| {
                entry.binding.kind == eska::project_core::MetadataKind::FunctionalOptionsParameter
                    && !entry.binding.nested
            })
            .map(|(source, _)| source.clone())
            .collect::<Vec<_>>();
        for source in sources {
            if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
                return Err(QueryError::Cancelled);
            }
            self.load_completion_owner(&source, request)?;
        }
        let targets = self
            .query_module(path, self.revision(), cancellation, |analysis, file| {
                Ok(self
                    .current_functional_option_parameters(analysis.database(), file)?
                    .iter()
                    .flat_map(|parameter| {
                        parameter.uses().unwrap_or_default().iter().map(|target| {
                            let (kind, name) = target.owner();
                            (kind, name.to_owned())
                        })
                    })
                    .collect::<Vec<_>>())
            })?
            .value;
        for (kind, name) in targets {
            if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
                return Err(QueryError::Cancelled);
            }
            let main = config_index(&self.database, self.selected_root.listing)
                .lookup(kind, &name)
                .map(|entry| entry.main);
            if let Some(source) = self
                .selected_root
                .owners
                .iter()
                .find(|(_, entry)| Some(entry.binding.main) == main)
                .map(|(source, _)| source.clone())
            {
                self.load_completion_owner(&source, request)?;
            }
        }
        Ok(())
    }
}
