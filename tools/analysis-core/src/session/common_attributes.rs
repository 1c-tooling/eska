//! Native membership failure is current context incompleteness, never evidence of a missing query table.

use super::AnalysisSession;
use crate::{QueryError, SessionRequestError, composition::SourceKey};

impl AnalysisSession {
    /// Validate native adoption and availability after source preparation, without a second membership model.
    pub(crate) fn require_common_attribute_membership(
        &self,
        file: vfs::FileId,
    ) -> Result<(), QueryError> {
        salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            self.check_common_attribute_membership(file)
        }))
        .map_err(|_| QueryError::Cancelled)?
        .map_err(QueryError::Request)
    }

    /// Keep Salsa reads inside the caller's native cancellation boundary.
    fn check_common_attribute_membership(
        &self,
        file: vfs::FileId,
    ) -> Result<(), SessionRequestError> {
        let roots = self.visible_root_ids(self.selected_root_id());
        let mut declared = false;
        for id in &roots {
            declared |= !self
                .root_inputs(*id)?
                .listing
                .common_attributes(&self.database)
                .is_empty();
        }
        if !declared {
            return Ok(());
        }
        if let Err(main) = self.database.common_attributes_for_file(file) {
            for id in roots {
                if let Some((source, _)) = self
                    .root_inputs(id)?
                    .owners
                    .iter()
                    .find(|(_, owner)| owner.binding.main == main)
                {
                    return Err(SessionRequestError::CompositionInputIncomplete(SourceKey {
                        root: id,
                        source: source.clone(),
                    }));
                }
            }
            return Err(SessionRequestError::QueryReaderMismatch);
        }
        Ok(())
    }
}
