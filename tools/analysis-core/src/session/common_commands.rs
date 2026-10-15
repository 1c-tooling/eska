//! Native command-owner rejection remains a root-qualified current-input error.

use super::AnalysisSession;
use crate::{QueryError, SessionRequestError, composition::SourceKey};

impl AnalysisSession {
    /// Check the same native composition used by inference before a missing type can hide bad adoption.
    pub(crate) fn require_common_command_owner(
        &self,
        database: &ide_db::RootDatabaseImpl,
        file: vfs::FileId,
    ) -> Result<(), QueryError> {
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            database.common_command_for_file(file)
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
