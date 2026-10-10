//! Typed namespaces and metadata members resolve to current canonical slots and YAML coordinates.

#[cfg(test)]
mod member_tests;
mod members;
#[cfg(test)]
mod namespace_tests;
#[cfg(test)]
mod register_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod union_tests;

use base_db::RootQueryDb;
use bsl_types::kind::ConfigId;
use hir::{NameClass, Semantics, TypeKernelDb, TypeKind};
use ide_db::metadata::{common_module_index, config_index, resolve_common_module};

use eska::project_core::source_format::SourcePath;

use crate::{
    AnalysisSession, MetadataLocation, ModelState, ModuleResult, QueryCancellation, QueryError,
    SessionRevision,
};

use super::definitions::{source_for_file, validate_offset};

impl AnalysisSession {
    /// Navigate a typed metadata object or declared member to current ready YAML.
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
            let Some(symbol) = metadata_symbol(analysis, file, offset) else {
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
            let main = if symbol.kind == bsl_metadata::MdoType::CommonModule {
                if resolve_common_module(analysis.database(), listing, symbol.name.clone())
                    .is_none()
                {
                    return Ok(None);
                }
                common_module_index(analysis.database(), listing).lookup(&symbol.name)
            } else {
                config_index(analysis.database(), listing)
                    .lookup(symbol.kind, &symbol.name)
                    .map(|ids| ids.main)
            };
            let Some(main) = main else {
                return Ok(None);
            };
            let native = source_for_file(self, analysis, main)?;
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
            let Some(uuid) =
                members::selected_uuid(self, owner, expected, declared.uuid, &symbol.children)?
            else {
                return Ok(None);
            };
            self.metadata_location(owner, expected, &uuid, None)
                .map(Some)
                .map_err(|_| QueryError::UnsupportedNavigationTarget)
        })
    }
}

/// One typed symbol selects an owner and an exact path of declared metadata children.
struct MetadataSymbol {
    kind: bsl_metadata::MdoType,
    name: String,
    children: Vec<(eska::project_core::MetadataKind, String)>,
}

/// Collection/member facts come from the existing semantic facade, never a spelling heuristic.
fn metadata_symbol(
    analysis: &ide::Analysis,
    file: vfs::FileId,
    offset: u32,
) -> Option<MetadataSymbol> {
    let parse = analysis.database().parse(file);
    let token = parse
        .syntax_node()
        .token_at_offset(offset.into())
        .right_biased()?;
    let semantics = Semantics::new(analysis.database());
    match hir::classify_token(&token) {
        NameClass::FreeName { token } => {
            if hir::FileSymbolCtx::new(analysis.database(), file)
                .without_binding_types()
                .symbol_for_token(&token)
                .is_some_and(|symbol| {
                    !matches!(symbol.definition, Some(hir::Definition::Module(_)))
                })
            {
                return None;
            }
            // Namespace types do not require a readable target body, while nearer declarations retain BSL ownership.
            let receiver = token.parent()?;
            let receiver_type = semantics.type_of_expr(file, &receiver);
            let TypeKind::CommonModule(facet) = analysis.database().lookup_type(receiver_type)
            else {
                return None;
            };
            if facet.config_id != ConfigId::Root {
                return None;
            }
            Some(MetadataSymbol {
                kind: bsl_metadata::MdoType::CommonModule,
                name: facet.name.clone(),
                children: Vec::new(),
            })
        }
        NameClass::FieldName {
            receiver,
            token,
            is_call: false,
        } => {
            let receiver_type = semantics.type_of_expr(file, &receiver);
            if let TypeKind::ManagerCollection(kind) =
                analysis.database().lookup_type(receiver_type)
            {
                Some(MetadataSymbol {
                    kind: *kind,
                    name: token.text().to_owned(),
                    children: Vec::new(),
                })
            } else {
                members::symbol(analysis, file, receiver_type, token.text())
            }
        }
        _ => None,
    }
}
