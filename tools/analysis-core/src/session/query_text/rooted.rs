//! Declared queries preserve root identity without receiving kernel file IDs or metadata semantics.

use super::{
    AnalysisSession, DiagnosticsConfig, InputRequest, QuerySyntaxDiagnostics,
    QuerySyntaxRequestError, SessionRequestError, SessionRevision, input_error,
};
use crate::composition::SourceKey;

impl AnalysisSession {
    /// Preserve declared query identity and current authority across root-qualified host input.
    ///
    /// # Errors
    /// Rejects stale or undeclared input, conflicting editor authority or unavailable query text.
    pub fn load_query_saved_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let path = &key.source;
        let (next, request) = self.query_update_at(key, expected, request)?;
        let root = self.root_inputs_mut(key.root)?;
        let reader = root.inputs.source_reader();
        let change = root
            .query_inputs
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownQuery(path.clone()))?
            .load_saved(reader, next, &request)
            .map_err(input_error)?;
        self.publish_query_change(next, change);
        Ok(())
    }

    /// Preserve declared query identity and current authority across root-qualified host input.
    ///
    /// # Errors
    /// Rejects stale or undeclared input, conflicting editor authority or unavailable query text.
    pub fn apply_query_overlay_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        document_version: i64,
        text: &str,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let path = &key.source;
        let (next, request) = self.query_update_at(key, expected, request)?;
        let change = self
            .root_inputs_mut(key.root)?
            .query_inputs
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownQuery(path.clone()))?
            .apply_overlay(next, document_version, text, &request)
            .map_err(input_error)?;
        self.publish_query_change(next, change);
        Ok(())
    }

    /// Preserve declared query identity and current authority across root-qualified host input.
    ///
    /// # Errors
    /// Rejects stale or undeclared input, conflicting editor authority or unavailable query text.
    pub fn close_query_overlay_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let path = &key.source;
        let (next, request) = self.query_update_at(key, expected, request)?;
        let root = self.root_inputs_mut(key.root)?;
        let reader = root.inputs.source_reader();
        let change = root
            .query_inputs
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownQuery(path.clone()))?
            .close_overlay(reader, next, &request)
            .map_err(input_error)?;
        self.publish_query_change(next, change);
        Ok(())
    }

    /// Preserve declared query identity and current authority across root-qualified host input.
    ///
    /// # Errors
    /// Rejects stale or undeclared input, conflicting editor authority or unavailable query text.
    pub fn release_query_saved_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        let path = &key.source;
        self.validate_revision(expected)?;
        let next = self.revision.next()?;
        let change = self
            .root_inputs_mut(key.root)?
            .query_inputs
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownQuery(path.clone()))?
            .release_saved(next)
            .map_err(input_error)?;
        self.publish_query_change(next, change);
        Ok(())
    }

    /// Preserve declared query identity and current authority across root-qualified host input.
    ///
    /// # Errors
    /// Rejects stale or undeclared input, conflicting editor authority or unavailable query text.
    pub fn query_syntax_diagnostics_at(
        &self,
        key: &SourceKey,
        expected: SessionRevision,
        config: &DiagnosticsConfig,
        request: &InputRequest,
    ) -> Result<QuerySyntaxDiagnostics, QuerySyntaxRequestError> {
        let path = &key.source;
        self.validate_revision(expected)
            .map_err(QuerySyntaxRequestError::Request)?;
        let input = self
            .root_inputs(key.root)
            .map_err(QuerySyntaxRequestError::Request)?
            .query_inputs
            .get(path)
            .ok_or_else(|| {
                QuerySyntaxRequestError::Request(SessionRequestError::UnknownQuery(path.clone()))
            })?;
        let result = input
            .query_syntax(config, request)
            .map_err(QuerySyntaxRequestError::Syntax)?;
        Ok(QuerySyntaxDiagnostics {
            revision: expected,
            result,
        })
    }

    /// Preserve declared query identity and current authority across root-qualified host input.
    fn query_update_at(
        &self,
        key: &SourceKey,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(SessionRevision, InputRequest), SessionRequestError> {
        let path = &key.source;
        self.validate_revision(expected)?;
        let input = self
            .root_inputs(key.root)?
            .query_inputs
            .get(path)
            .ok_or_else(|| SessionRequestError::UnknownQuery(path.clone()))?;
        Ok((
            self.revision.next()?,
            self.source_budget_request(input.bytes(), request),
        ))
    }
}
