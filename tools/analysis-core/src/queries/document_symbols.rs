//! A module's structure uses the kernel item/region trees of its current text, without metadata IO.

#[cfg(test)]
mod tests;

use base_db::SourceDatabase;
use eska::project_core::source_format::SourcePath;

use crate::{
    AnalysisSession, DocumentSymbol, ModuleResult, ModuleState, QueryCancellation, QueryError,
    SessionRevision,
};

impl AnalysisSession {
    /// Read the current module's declarations and regions from the same shared kernel.
    ///
    /// # Errors
    /// Returns stale/unknown/unread input, cancellation or inconsistent kernel names/ranges.
    pub fn module_document_symbols(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        cancellation: &QueryCancellation,
    ) -> Result<ModuleResult<Vec<DocumentSymbol>>, QueryError> {
        self.query_module(path, expected, cancellation, |analysis, file| {
            let input = analysis
                .database()
                .try_file_text_input(file)
                .ok_or_else(|| QueryError::ModuleUnavailable {
                    path: path.clone(),
                    state: ModuleState::Pending,
                })?;
            let symbols = analysis.document_symbols(file);
            validate(&symbols, input.text(analysis.database()), cancellation)?;
            Ok(symbols)
        })
    }
}

/// Validate every endpoint against the same borrowed text; malformed spans are never clamped.
fn validate(
    symbols: &[DocumentSymbol],
    text: &str,
    cancellation: &QueryCancellation,
) -> Result<(), QueryError> {
    let mut pending = vec![(symbols, None)];
    while let Some((symbols, parent)) = pending.pop() {
        for symbol in symbols {
            if cancellation.is_cancelled() {
                return Err(QueryError::Cancelled);
            }
            let start_byte = u32::from(symbol.range.start());
            let end_byte = u32::from(symbol.range.end());
            let selection_start_byte = u32::from(symbol.selection_range.start());
            let selection_end_byte = u32::from(symbol.selection_range.end());
            let endpoints = [
                start_byte,
                end_byte,
                selection_start_byte,
                selection_end_byte,
            ];
            if symbol.name.trim().is_empty()
                || !symbol.range.contains_range(symbol.selection_range)
                || parent.is_some_and(|range: ide::TextRange| !range.contains_range(symbol.range))
                || endpoints
                    .iter()
                    .any(|offset| !text.is_char_boundary(*offset as usize))
            {
                return Err(QueryError::InconsistentDocumentSymbol {
                    start_byte,
                    end_byte,
                    selection_start_byte,
                    selection_end_byte,
                });
            }
            pending.push((&symbol.children, Some(symbol.range)));
        }
    }
    Ok(())
}
