//! Typed metadata collection members resolve to current canonical slots and YAML coordinates.

#[cfg(test)]
mod tests;

use base_db::RootQueryDb;
use hir::{NameClass, Semantics, TypeKernelDb, TypeKind};
use ide_db::metadata::config_index;

use eska::project_core::source_format::SourcePath;

use crate::{
    AnalysisSession, MetadataLocation, ModelState, ModuleResult, QueryCancellation, QueryError,
    SessionRevision,
};

use super::definitions::{source_for_file, validate_offset};

impl AnalysisSession {
    /// Navigate a typed collection member to current ready YAML.
    ///
    /// Receiver inference respects local shadowing; the existing canonical index resolves the name.
    /// Invalid/unread canonical metadata cannot confirm a symbol even if its tree is readable.
    ///
    /// # Errors
    /// Returns stale/unread caller, bad offset, cancellation or an inconsistent declared target.
    pub fn module_metadata_definition(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        offset: u32,
        cancellation: &QueryCancellation,
    ) -> Result<ModuleResult<Option<MetadataLocation>>, QueryError> {
        self.query_module(path, expected, cancellation, |analysis, file| {
            validate_offset(analysis, file, path, offset)?;
            let Some((kind, name)) = metadata_symbol(analysis, file, offset) else {
                return Ok(None);
            };
            let root = self
                .root()
                .to_str()
                .ok_or(QueryError::UnsupportedNavigationTarget)?;
            let listing = analysis
                .database()
                .metadata_listing(root)
                .ok_or(QueryError::UnsupportedNavigationTarget)?;
            let Some(ids) = config_index(analysis.database(), listing).lookup(kind, &name) else {
                return Ok(None);
            };
            let native = source_for_file(self, analysis, ids.main)?;
            let owner = self
                .inputs
                .source_reader()
                .owner_for_native_main(&native)
                .ok_or(QueryError::UnsupportedNavigationTarget)?;
            if !matches!(
                self.owner(owner).map(|model| &model.state),
                Some(ModelState::Ready(_))
            ) {
                return Ok(None);
            }
            let declared = self
                .inputs
                .source_reader()
                .declared_owner(owner)
                .map_err(|_| QueryError::UnsupportedNavigationTarget)?;
            self.metadata_location(owner, expected, declared.uuid, None)
                .map(Some)
                .map_err(|_| QueryError::UnsupportedNavigationTarget)
        })
    }
}

/// Only an actual collection receiver can designate a top-level metadata object.
fn metadata_symbol(
    analysis: &ide::Analysis,
    file: vfs::FileId,
    offset: u32,
) -> Option<(bsl_metadata::MdoType, String)> {
    let parse = analysis.database().parse(file);
    let token = parse
        .syntax_node()
        .token_at_offset(offset.into())
        .right_biased()?;
    let semantics = Semantics::new(analysis.database());
    match hir::classify_token(&token) {
        NameClass::FieldName {
            receiver,
            token,
            is_call: false,
        } => {
            let receiver_type = semantics.type_of_expr(file, &receiver);
            let TypeKind::ManagerCollection(kind) = analysis.database().lookup_type(receiver_type)
            else {
                return None;
            };
            Some((*kind, token.text().to_owned()))
        }
        _ => None,
    }
}
