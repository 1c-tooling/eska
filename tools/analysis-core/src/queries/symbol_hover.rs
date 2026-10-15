//! Resolve a typed resident symbol card; transport owns localized headings and markup.

mod external;
#[cfg(test)]
mod tests;

use base_db::{RootQueryDb, SourceDatabase};
use eska::project_core::source_format::SourcePath;
use ide_db::RootDatabase;

use crate::{
    AnalysisSession, Locale, ModuleResult, QueryCancellation, QueryError, SessionRevision,
};

use super::definitions::{navigation_offset, validate_offset};

/// Positional cards use a closed vocabulary independent of their human heading or source language.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HoverKind {
    Method,
    ModuleVariable,
    Parameter,
    LocalVariable,
    PlatformFunction,
    Module,
    MetadataObject,
    ManagerModule,
    Field,
}

/// A card contains current owned display data; native XML definitions and graph ids are not exposed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolHover {
    pub symbol: String,
    pub kind: HoverKind,
    /// The kernel's BSL method declaration; this code is not translated as a UI message.
    pub declaration: Option<String>,
    /// Authored BSL purpose text is preserved verbatim.
    pub documentation: Option<String>,
    /// Methods carry a return type; parameters, locals and fields carry their inferred value type.
    pub return_type: Option<String>,
    pub start_byte: u32,
    pub end_byte: u32,
}

impl AnalysisSession {
    /// Resolve an existing BSL symbol at either identifier boundary in the current kernel snapshot.
    ///
    /// # Errors
    /// Returns stale/unknown/unread input, invalid offset, cancellation or an inconsistent card.
    pub fn module_symbol_hover(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        offset: u32,
        locale: Locale,
        cancellation: &QueryCancellation,
    ) -> Result<ModuleResult<Option<SymbolHover>>, QueryError> {
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
            let start_byte = u32::from(token.text_range().start());
            let end_byte = u32::from(token.text_range().end());
            let text = analysis.database().file_text(file);
            let inconsistent = || QueryError::InconsistentHover {
                start_byte,
                end_byte,
            };
            if start_byte > end_byte
                || !text.is_char_boundary(start_byte as usize)
                || !text.is_char_boundary(end_byte as usize)
            {
                return Err(inconsistent());
            }
            if let Some(card) =
                external::current(self, analysis, file, hir::classify_token(&token), locale)?
            {
                return Ok(Some(card));
            }
            // The positional kernel API uses its own line index, whose newline rules may
            // differ from the transport's. Feed that same index's UTF-16 inverse, not LSP rows.
            let position = analysis
                .database()
                .line_index(base_db::FileIdInput::new(analysis.database(), file))
                .utf16_line_col_range(&text, token.text_range())
                .ok_or_else(inconsistent)?;
            let Some(card) = ide::symbol_info(
                analysis.database(),
                &ide::SymbolInfoRequest {
                    symbol: None,
                    position: Some(ide::SymbolPosition {
                        file_id: file,
                        line: position.start_line,
                        column: position.start_character,
                    }),
                    locale,
                    sections: ide::SymbolInfoSections {
                        definition: false,
                        type_: true,
                        doc: true,
                    },
                    workspace_root: None,
                },
            ) else {
                return Ok(None);
            };
            if card.semantics_unavailable || card.kind == "unresolved" {
                return Ok(None);
            }
            if card.symbol.trim().is_empty() {
                return Err(inconsistent());
            }
            let kind = kind(card.kind)?;
            Ok(Some(SymbolHover {
                symbol: card.symbol,
                kind,
                declaration: if kind == HoverKind::Method {
                    card.signature
                } else {
                    None
                },
                documentation: card.doc,
                return_type: card.return_type,
                start_byte,
                end_byte,
            }))
        })
    }
}

/// An unexpected pinned kind is explicit instead of becoming an untranslated or invented heading.
fn kind(kind: &'static str) -> Result<HoverKind, QueryError> {
    match kind {
        "method" => Ok(HoverKind::Method),
        "module variable" => Ok(HoverKind::ModuleVariable),
        "parameter" => Ok(HoverKind::Parameter),
        "local variable" => Ok(HoverKind::LocalVariable),
        "platform function" => Ok(HoverKind::PlatformFunction),
        "module" => Ok(HoverKind::Module),
        "metadata object" => Ok(HoverKind::MetadataObject),
        "manager module" => Ok(HoverKind::ManagerModule),
        "field" => Ok(HoverKind::Field),
        _ => Err(QueryError::UnsupportedHoverSymbol { kind }),
    }
}
