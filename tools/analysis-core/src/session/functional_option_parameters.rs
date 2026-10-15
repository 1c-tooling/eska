//! Current functional-option parameter failures retain their exact admitted root rather than falling back to an old declaration.

use super::AnalysisSession;
use crate::{QueryError, SessionRequestError, composition::SourceKey};

impl AnalysisSession {
    /// Semantic diagnostics require every visible declared parameter to have a current compatible value.
    pub(crate) fn require_functional_option_parameters_context(
        &self,
        database: &ide_db::RootDatabaseImpl,
        file: vfs::FileId,
    ) -> Result<(), QueryError> {
        self.current_functional_option_parameters(database, file)
            .map(|_| ())
    }

    /// Return the same proven declarations to preparation and preflight without suppressing a failed contribution.
    pub(crate) fn current_functional_option_parameters(
        &self,
        database: &ide_db::RootDatabaseImpl,
        file: vfs::FileId,
    ) -> Result<Vec<std::sync::Arc<bsl_metadata::FunctionalOptionsParameter>>, QueryError> {
        salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            database.functional_option_parameters_for_file(file)
        }))
        .map_err(|_| QueryError::Cancelled)?
        .map_err(|main| self.functional_option_parameter_failure(main))
    }

    /// Native refusal identifies the actual current contribution rather than an unrelated same-name owner.
    fn functional_option_parameter_failure(&self, main: vfs::FileId) -> QueryError {
        for root in self.visible_root_ids(self.selected_root_id()) {
            if let Ok(inputs) = self.root_inputs(root)
                && let Some((source, _)) = inputs
                    .owners
                    .iter()
                    .find(|(_, entry)| entry.binding.main == main)
            {
                return QueryError::Request(SessionRequestError::CompositionInputIncomplete(
                    SourceKey {
                        root,
                        source: source.clone(),
                    },
                ));
            }
        }
        QueryError::Request(SessionRequestError::QueryReaderMismatch)
    }
}
