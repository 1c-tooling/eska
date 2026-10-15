//! Native literal-call discovery selects current option storage; the ordinary controller owns source I/O.

use super::{
    AnalysisSession, InputRequest, QueryCancellation, QueryError, SourcePath, config_index,
};

impl AnalysisSession {
    /// Prepare only named current declarations and their admitted storage owners, preserving dirty authority.
    pub(super) fn prepare_functional_options(
        &mut self,
        path: &SourcePath,
        offset: u32,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<(), QueryError> {
        let names = self
            .query_module(path, self.revision(), cancellation, |analysis, file| {
                Ok(analysis.functional_option_dependencies(file, offset))
            })?
            .value;
        if names.is_empty() {
            return Ok(());
        }
        let sources = self
            .selected_root
            .owners
            .iter()
            .filter(|(_, entry)| {
                entry.binding.kind == eska::project_core::MetadataKind::FunctionalOption
                    && !entry.binding.nested
                    && entry.binding.name.as_ref().is_some_and(|name| {
                        names
                            .iter()
                            .any(|value| value.to_lowercase() == name.to_lowercase())
                    })
            })
            .map(|(source, _)| source.clone())
            .collect::<Vec<_>>();
        for source in sources {
            if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
                return Err(QueryError::Cancelled);
            }
            self.load_completion_owner(&source, request)?;
        }
        for name in names {
            let target = self
                .query_module(path, self.revision(), cancellation, |analysis, file| {
                    let value = self.current_functional_option(analysis.database(), file, &name)?;
                    Ok(value.and_then(|value| {
                        value
                            .location()
                            .and_then(bsl_metadata::FunctionalOptionLocation::owner)
                            .map(|(kind, name)| (kind, name.to_owned()))
                    }))
                })?
                .value;
            if let Some((kind, name)) = target {
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
        }
        Ok(())
    }
}
