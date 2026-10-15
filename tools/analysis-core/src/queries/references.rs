//! Native reference enumeration keeps resolved-zero separate from unsupported or missing anchors.

#[cfg(test)]
mod tests;

use base_db::{RootQueryDb, SourceDatabase};
use eska::project_core::source_format::SourcePath;
use ide_db::RootDatabase;

use super::definitions::{DefinitionTarget, current_target, navigation_offset, validate_offset};
use crate::{AnalysisSession, ModuleResult, QueryCancellation, QueryError, SessionRevision};

/// This caps native candidate-file enumeration, not the name-usage index or process RSS.
pub const MAX_REFERENCE_FILES: usize = 200;
/// This caps projected occurrences; native collection happens before this display boundary.
pub const MAX_REFERENCE_HITS: usize = 10_000;

/// Native semantic classification is retained independently of locale and presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Declaration,
    Call,
    Write,
    Read,
}

/// Each occurrence binds one native hit to a captured current, declared BSL target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceOccurrence {
    pub target: DefinitionTarget,
    pub kind: ReferenceKind,
}

/// Counts refer to the native walk; capped results are lower bounds and never rename proofs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceSearch {
    pub occurrences: Vec<ReferenceOccurrence>,
    pub files_scanned: usize,
    pub files_capped: bool,
    pub hits_capped: bool,
}

impl AnalysisSession {
    /// Enumerate references to the current BSL name without loading siblings or matching text locally.
    ///
    /// # Errors
    /// Returns current-input guards, cancellation, unsupported scope or inconsistent native data.
    pub fn module_references(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        offset: u32,
        include_declaration: bool,
        cancellation: &QueryCancellation,
    ) -> Result<ModuleResult<Option<ReferenceSearch>>, QueryError> {
        self.query_module(path, expected, cancellation, |analysis, file| {
            validate_offset(analysis, file, path, offset)?;
            let offset = navigation_offset(analysis, file, offset);
            let parse = analysis.database().parse(file);
            let Some(token) = parse
                .syntax_node()
                .token_at_offset(offset.into())
                .right_biased()
            else {
                return Ok(None);
            };
            if !matches!(
                hir::classify_token(&token),
                hir::NameClass::FreeName { .. }
                    | hir::NameClass::FieldName { .. }
                    | hir::NameClass::TypeRef { .. }
            ) {
                return Ok(None);
            }
            // The native positional anchor is the inverse of its own LF-based UTF-16 index.
            let text = analysis.database().file_text(file);
            let position = analysis
                .database()
                .line_index(base_db::FileIdInput::new(analysis.database(), file))
                .utf16_line_col_range(&text, token.text_range())
                .ok_or(QueryError::InconsistentReferences)?;
            let result = ide::find_references_by_name(
                analysis.database(),
                &ide::ReferencesRequest {
                    anchor: ide::ReferenceAnchor::Position {
                        file_id: file,
                        line: position.start_line,
                        column: position.start_character,
                    },
                    anchor_files: None,
                    area: ide::ReferenceArea {
                        files: None,
                        candidate_roots: self.composition_sha256().map(|_| {
                            self.visible_root_ids(self.selected_root_id())
                                .into_iter()
                                .map(|root| base_db::SourceRootId(root.index()))
                                .collect()
                        }),
                    },
                    kinds: None,
                    include_declaration,
                    max_files: MAX_REFERENCE_FILES,
                },
                &[],
            );
            project(self, analysis, result, cancellation)
        })
    }
}

/// Typed native outcomes determine whether an empty result is meaningful before target projection.
fn project(
    session: &AnalysisSession,
    analysis: &ide::Analysis,
    result: ide::ReferencesResult,
    control: &QueryCancellation,
) -> Result<Option<ReferenceSearch>, QueryError> {
    if result.body_source != ide::BodySource::Resident || result.anchor_candidates_capped {
        return Err(QueryError::InconsistentReferences);
    }
    match result.outcome {
        ide::ReferencesOutcome::NotFound => return Ok(None),
        ide::ReferencesOutcome::UnsupportedSymbol { category } => {
            return Err(QueryError::UnsupportedReferenceSymbol {
                category: category.as_str(),
            });
        }
        ide::ReferencesOutcome::Resolved => (),
        ide::ReferencesOutcome::Ambiguous | ide::ReferencesOutcome::AnchorStale { .. } => {
            return Err(QueryError::InconsistentReferences);
        }
    }
    let hits_capped = result.hits.len() > MAX_REFERENCE_HITS;
    let occurrences = result
        .hits
        .into_iter()
        .take(MAX_REFERENCE_HITS)
        .map(|hit| {
            if control.is_cancelled() {
                return Err(QueryError::Cancelled);
            }
            let target = current_target(
                session,
                analysis,
                hit.file_id,
                hit.range.start().into(),
                hit.range.end().into(),
            )?;
            Ok(ReferenceOccurrence {
                target,
                kind: match hit.kind {
                    ide::ReferenceKind::Declaration => ReferenceKind::Declaration,
                    ide::ReferenceKind::Call => ReferenceKind::Call,
                    ide::ReferenceKind::Write => ReferenceKind::Write,
                    ide::ReferenceKind::Read => ReferenceKind::Read,
                },
            })
        })
        .collect::<Result<Vec<_>, QueryError>>()?;
    Ok(Some(ReferenceSearch {
        occurrences,
        files_scanned: result.files_scanned,
        files_capped: result.files_capped,
        hits_capped,
    }))
}
