//! Navigation returns only declared, ready BSL targets with ranges validated against current bytes.

#[cfg(test)]
mod tests;

use base_db::{RootQueryDb, SourceDatabase};
use eska::project_core::source_format::SourcePath;

use crate::{
    AnalysisSession, ModuleResult, ModuleState, QueryCancellation, QueryError, SessionRevision,
};

/// A target belongs to the same session revision as its caller, without retaining a second buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefinitionTarget {
    pub source: SourcePath,
    pub root: crate::composition::RootId,
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
            let offset = navigation_offset(analysis, file, offset);
            let Some(target) = analysis.goto_definition(file, offset) else {
                return Ok(None);
            };
            current_target(
                self,
                analysis,
                target.file_id,
                target.range.start().into(),
                target.range.end().into(),
            )
            .map(Some)
        })
    }
}

/// Navigation and references capture only declared ready targets, with verbatim bytes and hashes.
pub(super) fn current_target(
    session: &AnalysisSession,
    analysis: &ide::Analysis,
    file: vfs::FileId,
    start_byte: u32,
    end_byte: u32,
) -> Result<DefinitionTarget, QueryError> {
    let source_root = analysis
        .database()
        .file_source_root_input(file)
        .source_root_id(analysis.database());
    let root = crate::composition::RootId::from_index(source_root.0 as usize)
        .map_err(|_| QueryError::UnsupportedNavigationTarget)?;
    let inputs = session.root_inputs(root).map_err(QueryError::Request)?;
    let source = inputs
        .module_sources
        .get(&file)
        .cloned()
        .ok_or(QueryError::UnsupportedNavigationTarget)?;
    let entry = inputs
        .modules
        .get(&source)
        .filter(|entry| entry.file == file)
        .ok_or(QueryError::UnsupportedNavigationTarget)?;
    let unavailable = || QueryError::ModuleUnavailable {
        path: source.clone(),
        state: entry.model.state.clone(),
    };
    if !matches!(entry.model.state, ModuleState::Ready { .. }) {
        return Err(unavailable());
    }
    let input = analysis
        .database()
        .try_file_text_input(file)
        .ok_or_else(unavailable)?;
    let text = input.text(analysis.database());
    if start_byte > end_byte
        || !text.is_char_boundary(start_byte as usize)
        || !text.is_char_boundary(end_byte as usize)
    {
        return Err(QueryError::InconsistentNavigation {
            start_byte,
            end_byte,
        });
    }
    let source_sha256 = entry.model.source_sha256.clone().ok_or_else(unavailable)?;
    Ok(DefinitionTarget {
        source,
        root,
        source_sha256,
        start_byte,
        end_byte,
    })
}

/// Prefer an adjacent name at either cursor boundary; retain original positions for non-name queries.
pub(super) fn navigation_offset(analysis: &ide::Analysis, file: vfs::FileId, offset: u32) -> u32 {
    analysis
        .database()
        .parse(file)
        .syntax_node()
        .token_at_offset(offset.into())
        .find(|token| {
            matches!(
                hir::classify_token(token),
                hir::NameClass::FreeName { .. }
                    | hir::NameClass::FieldName { .. }
                    | hir::NameClass::TypeRef { .. }
            )
        })
        .map_or(offset, |token| u32::from(token.text_range().start()))
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
) -> Result<crate::composition::SourceKey, QueryError> {
    let source_root = analysis
        .database()
        .file_source_root_input(file)
        .source_root_id(analysis.database());
    let id = crate::composition::RootId::from_index(source_root.0 as usize)
        .map_err(|_| QueryError::UnsupportedNavigationTarget)?;
    let directory = session
        .source_root(id)
        .ok_or(QueryError::UnsupportedNavigationTarget)?;
    let root = analysis.database().source_root_input(source_root);
    let native = root
        .root(analysis.database())
        .file_set()
        .path_for_file(&file)
        .map(vfs::VfsPath::as_path)
        .ok_or(QueryError::UnsupportedNavigationTarget)?;
    let relative = native
        .strip_prefix(directory)
        .map_err(|_| QueryError::UnsupportedNavigationTarget)?;
    let source = SourcePath::parse(
        &relative
            .to_str()
            .ok_or(QueryError::UnsupportedNavigationTarget)?
            .replace(std::path::MAIN_SEPARATOR, "/"),
    )
    .map_err(|_| QueryError::UnsupportedNavigationTarget)?;
    Ok(crate::composition::SourceKey { root: id, source })
}
