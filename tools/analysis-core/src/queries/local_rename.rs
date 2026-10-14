//! Local rename previews require native method scope, complete resident hits and pinned write policy.

#[cfg(test)]
mod tests;
mod validation;

use base_db::RootQueryDb;
use eska::{
    analysis_core::InputOrigin,
    project_core::source_format::{AcceptedBase, Error, Permission, SourcePath},
};
use hir::{DefWithBodyId, Definition, ReferenceScope, SemanticSymbol, SemanticSymbolKey};

use super::definitions::{DefinitionTarget, navigation_offset, validate_offset};
use crate::{AnalysisSession, ModuleResult, QueryCancellation, QueryError, SessionRevision};

/// A bounded replacement cannot multiply a frame-sized string across thousands of edit locations.
pub const MAX_RENAME_NAME_BYTES: usize = 1024;

/// A rejected preview neither changes inputs nor grants permission from working YAML.
#[derive(Debug)]
pub enum LocalRenameError {
    Query(QueryError),
    Source(Error),
    DocumentNotOpen,
    InputConflict,
    SyntaxInvalid,
    UnsupportedSymbol,
    InvalidName,
    NameTooLarge { bytes: usize, limit: usize },
    NameConflict { start_byte: u32, end_byte: u32 },
    PermissionDenied(Permission),
    SearchIncomplete,
    InconsistentOccurrences,
}

/// Private construction binds eligibility to the exact open document before costly base verification.
#[derive(Debug)]
pub struct LocalRenameInput {
    source: SourcePath,
    revision: SessionRevision,
    source_sha256: String,
    document_version: i64,
    symbol: String,
    parameter: bool,
    start_byte: u32,
    end_byte: u32,
}

/// One current anchor carries both editor version and independently accepted modification authority.
#[derive(Debug)]
pub struct LocalRenameTarget {
    pub source: SourcePath,
    pub revision: SessionRevision,
    pub source_sha256: String,
    pub document_version: i64,
    pub symbol: String,
    pub start_byte: u32,
    pub end_byte: u32,
    pub permission: Permission,
}

/// Only changed, nonoverlapping occurrences belong to a versioned single-document edit preview.
#[derive(Debug)]
pub struct LocalRenamePlan {
    pub target: LocalRenameTarget,
    pub new_name: String,
    pub occurrences: Vec<DefinitionTarget>,
}

impl AnalysisSession {
    /// Select only a native method-local symbol from an open, syntactically valid current module.
    ///
    /// # Errors
    /// Returns current-input guards, closed/invalid text or a symbol outside method-local scope.
    pub fn module_local_rename_input(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        offset: u32,
        control: &QueryCancellation,
    ) -> Result<ModuleResult<Option<LocalRenameInput>>, LocalRenameError> {
        self.query_module(path, expected, control, |analysis, file| {
            let entry = self
                .modules
                .get(path)
                .ok_or(QueryError::UnsupportedNavigationTarget)?;
            let InputOrigin::Overlay { document_version } = entry.model.origin else {
                return Ok(Err(LocalRenameError::DocumentNotOpen));
            };
            validate_offset(analysis, file, path, offset)?;
            let parse = analysis.database().parse(file);
            if parse.has_errors() {
                return Ok(Err(LocalRenameError::SyntaxInvalid));
            }
            let offset = navigation_offset(analysis, file, offset);
            let Some(token) = parse
                .syntax_node()
                .token_at_offset(offset.into())
                .right_biased()
            else {
                return Ok(Ok(None));
            };
            if !matches!(hir::classify_token(&token), hir::NameClass::FreeName { .. }) {
                return Ok(Ok(None));
            }
            let context =
                hir::FileSymbolCtx::new(analysis.database(), file).without_binding_types();
            let Some(symbol) = context.symbol_for_token(&token) else {
                return Ok(Err(LocalRenameError::UnsupportedSymbol));
            };
            if !method_local(&symbol)
                || symbol.reference_scope(analysis.database()) != ReferenceScope::FileLocal
            {
                return Ok(Err(LocalRenameError::UnsupportedSymbol));
            }
            let source_sha256 =
                entry
                    .model
                    .source_sha256
                    .clone()
                    .ok_or_else(|| QueryError::ModuleUnavailable {
                        path: path.clone(),
                        state: entry.model.state.clone(),
                    })?;
            Ok(Ok(Some(LocalRenameInput {
                source: path.clone(),
                revision: expected,
                source_sha256,
                document_version,
                symbol: token.text().to_owned(),
                parameter: matches!(symbol.definition, Some(Definition::Parameter { .. })),
                start_byte: token.text_range().start().into(),
                end_byte: token.text_range().end().into(),
            })))
        })
        .map_err(LocalRenameError::Query)
        .and_then(|result| {
            Ok(ModuleResult {
                revision: result.revision,
                source: result.source,
                source_sha256: result.source_sha256,
                metadata_models_complete: result.metadata_models_complete,
                metadata_context_complete: result.metadata_context_complete,
                module_inputs_complete: result.module_inputs_complete,
                value: result.value?,
            })
        })
    }
}

impl LocalRenameInput {
    /// Validate bounded declaration/read/assignment spelling without opening accepted storage.
    ///
    /// # Errors
    /// Returns invalid native grammar or a replacement exceeding the preview's name-byte limit.
    pub fn validate_new_name(&self, new_name: &str) -> Result<(), LocalRenameError> {
        validation::identifier(new_name, self.parameter)
    }

    /// Recheck the captured document before returning a prepare-rename range with pinned permission.
    ///
    /// # Errors
    /// Returns changed input, cancellation or a denied/unknown accepted module.
    pub fn prepare(
        self,
        session: &AnalysisSession,
        base: &AcceptedBase,
        control: &QueryCancellation,
    ) -> Result<LocalRenameTarget, LocalRenameError> {
        self.current(session, control)?;
        self.authorize(base)
    }

    /// Validate a replacement and complete native local search without applying or caching edits.
    ///
    /// # Errors
    /// Returns current-input/policy guards, invalid/colliding names or incomplete/inconsistent hits.
    pub fn rename(
        self,
        session: &AnalysisSession,
        base: &AcceptedBase,
        new_name: &str,
        control: &QueryCancellation,
    ) -> Result<LocalRenamePlan, LocalRenameError> {
        self.current(session, control)?;
        self.validate_new_name(new_name)?;
        let target = self.authorize(base)?;
        let search = session
            .module_references(
                &target.source,
                target.revision,
                target.start_byte,
                true,
                control,
            )
            .map_err(LocalRenameError::Query)?
            .value
            .ok_or(LocalRenameError::InconsistentOccurrences)?;
        if search.files_capped || search.hits_capped || search.files_scanned != 1 {
            return Err(LocalRenameError::SearchIncomplete);
        }
        let occurrences =
            validation::occurrences(session, &target, search.occurrences, new_name, control)?;
        Ok(LocalRenamePlan {
            target,
            new_name: new_name.to_owned(),
            occurrences,
        })
    }

    /// A retained input from an earlier query cannot authorize a new document or a different host body.
    fn current(
        &self,
        session: &AnalysisSession,
        control: &QueryCancellation,
    ) -> Result<(), LocalRenameError> {
        let result = session
            .with_module_text(&self.source, self.revision, control, |_| ())
            .map_err(LocalRenameError::Query)?;
        if result.source_sha256 != self.source_sha256
            || session.module(&self.source).is_none_or(|input| {
                input.origin
                    != InputOrigin::Overlay {
                        document_version: self.document_version,
                    }
            })
        {
            return Err(LocalRenameError::InputConflict);
        }
        Ok(())
    }

    /// Neither a current YAML UUID nor an open editor can relax accepted supplier/freeze restrictions.
    fn authorize(self, base: &AcceptedBase) -> Result<LocalRenameTarget, LocalRenameError> {
        let permission = base
            .module_permission(&self.source)
            .map_err(LocalRenameError::Source)?;
        if !permission.allowed {
            return Err(LocalRenameError::PermissionDenied(permission));
        }
        Ok(LocalRenameTarget {
            source: self.source,
            revision: self.revision,
            source_sha256: self.source_sha256,
            document_version: self.document_version,
            symbol: self.symbol,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
            permission,
        })
    }
}

/// Module variables and methods remain excluded even when their native reference scope is file-local.
const fn method_local(symbol: &SemanticSymbol) -> bool {
    matches!(
        symbol.definition,
        Some(Definition::Parameter { .. } | Definition::Local { .. })
    ) || matches!(
        symbol.key,
        SemanticSymbolKey::BodyLocal {
            owner: DefWithBodyId::Method(_),
            ..
        } | SemanticSymbolKey::ImplicitLocal {
            owner: DefWithBodyId::Method(_),
            ..
        }
    )
}
