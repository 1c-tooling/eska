//! Unproven current criteria cannot become a complete empty configured namespace or a false Find result.

use super::AnalysisSession;
use crate::{QueryError, SessionRequestError, composition::SourceKey};

impl AnalysisSession {
    /// Use the caller's attached database when validating the native composition shared by inference and completion.
    pub(crate) fn require_filter_criteria_context(
        &self,
        database: &ide_db::RootDatabaseImpl,
        file: vfs::FileId,
    ) -> Result<(), QueryError> {
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            database.filter_criteria_for_file(file)
        }))
        .map_err(|_| QueryError::Cancelled)?;
        if let Err(main) = result {
            for root in self.visible_root_ids(self.selected_root_id()) {
                if let Some((source, _)) = self
                    .root_inputs(root)
                    .map_err(QueryError::Request)?
                    .owners
                    .iter()
                    .find(|(_, owner)| owner.binding.main == main)
                {
                    return Err(QueryError::Request(
                        SessionRequestError::CompositionInputIncomplete(SourceKey {
                            root,
                            source: source.clone(),
                        }),
                    ));
                }
            }
            return Err(QueryError::Request(
                SessionRequestError::QueryReaderMismatch,
            ));
        }
        Ok(())
    }
}
