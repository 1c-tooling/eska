//! Navigation returns only declared, ready BSL targets with ranges validated against current bytes.

#[cfg(test)]
mod tests;

use base_db::{SourceDatabase, SourceRootId};
use eska::project_core::source_format::SourcePath;

use crate::{
    AnalysisSession, ModuleResult, ModuleState, QueryCancellation, QueryError, SessionRevision,
};

/// A target belongs to the same session revision as its caller, without retaining a second buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefinitionTarget {
    pub source: SourcePath,
    pub source_sha256: String,
    pub start_byte: u32,
    pub end_byte: u32,
}

impl AnalysisSession {
    /// Resolve a BSL declaration from current inputs; a missing result may have incomplete coverage.
    ///
    /// # Errors
    /// Returns stale/unread input, invalid offset, cancellation or an unsupported/inconsistent target.
    pub fn module_definition(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        offset: u32,
        cancellation: &QueryCancellation,
    ) -> Result<ModuleResult<Option<DefinitionTarget>>, QueryError> {
        self.query_module(path, expected, cancellation, |analysis, file| {
            validate_offset(analysis, file, path, offset)?;
            let Some(target) = analysis.goto_definition(file, offset) else {
                return Ok(None);
            };
            let source = source_for_file(self, analysis, target.file_id)?;
            let entry = self
                .modules
                .get(&source)
                .filter(|entry| entry.file == target.file_id)
                .ok_or(QueryError::UnsupportedNavigationTarget)?;
            let unavailable = || QueryError::ModuleUnavailable {
                path: source.clone(),
                state: entry.model.state.clone(),
            };
            if !matches!(entry.model.state, ModuleState::Ready { .. }) {
                return Err(unavailable());
            }
            let text = analysis
                .database()
                .try_file_text_input(target.file_id)
                .ok_or_else(unavailable)?;
            let text = text.text(analysis.database());
            let start_byte = u32::from(target.range.start());
            let end_byte = u32::from(target.range.end());
            if start_byte > end_byte
                || !text.is_char_boundary(start_byte as usize)
                || !text.is_char_boundary(end_byte as usize)
            {
                return Err(QueryError::InconsistentNavigation {
                    start_byte,
                    end_byte,
                });
            }
            Ok(Some(DefinitionTarget {
                source: source.clone(),
                source_sha256: entry.model.source_sha256.clone().ok_or_else(unavailable)?,
                start_byte,
                end_byte,
            }))
        })
    }
}

/// Both navigation paths validate the caller's exact UTF-8 boundary before any semantic query.
pub(super) fn validate_offset(
    analysis: &ide::Analysis,
    file: vfs::FileId,
    path: &SourcePath,
    offset: u32,
) -> Result<(), QueryError> {
    let input = analysis
        .database()
        .try_file_text_input(file)
        .ok_or_else(|| QueryError::ModuleUnavailable {
            path: path.clone(),
            state: ModuleState::Pending,
        })?;
    if input
        .text(analysis.database())
        .is_char_boundary(offset as usize)
    {
        Ok(())
    } else {
        Err(QueryError::InvalidOffset { offset })
    }
}

/// Resolve a registered identity through the existing file set, never filesystem or directory scans.
pub(super) fn source_for_file(
    session: &AnalysisSession,
    analysis: &ide::Analysis,
    file: vfs::FileId,
) -> Result<SourcePath, QueryError> {
    let root = analysis.database().source_root_input(SourceRootId(0));
    let native = root
        .root(analysis.database())
        .file_set()
        .path_for_file(&file)
        .map(vfs::VfsPath::as_path)
        .ok_or(QueryError::UnsupportedNavigationTarget)?;
    let relative = native
        .strip_prefix(session.root())
        .map_err(|_| QueryError::UnsupportedNavigationTarget)?;
    SourcePath::parse(
        &relative
            .to_str()
            .ok_or(QueryError::UnsupportedNavigationTarget)?
            .replace(std::path::MAIN_SEPARATOR, "/"),
    )
    .map_err(|_| QueryError::UnsupportedNavigationTarget)
}
