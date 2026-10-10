//! Machine-facing rename refusal preserves current snapshot and structured policy/conflict details.

use serde_json::{Value, json};

use super::super::{dto, language, protocol};
use crate::{AnalysisSession, LocalRenameError};

/// Native query guards retain their established codes; new plan errors remain locale-independent.
pub(super) fn value(session: &AnalysisSession, id: &str, error: LocalRenameError) -> Value {
    let (code, message, mut details) = match error {
        LocalRenameError::Query(error) => return language::query_error(session, id, &error),
        LocalRenameError::Source(error) => (-32000, error.code, json!({"location":error.location})),
        LocalRenameError::DocumentNotOpen => (-32000, "rename_document_not_open", json!({})),
        LocalRenameError::InputConflict => (-32801, "rename_input_conflict", json!({})),
        LocalRenameError::SyntaxInvalid => (-32000, "rename_syntax_invalid", json!({})),
        LocalRenameError::UnsupportedSymbol => (-32000, "rename_symbol_unsupported", json!({})),
        LocalRenameError::InvalidName => (-32602, "rename_name_invalid", json!({})),
        LocalRenameError::NameTooLarge { bytes, limit } => (
            -32602,
            "rename_name_too_large",
            json!({"bytes":bytes,"limit":limit}),
        ),
        LocalRenameError::NameConflict {
            start_byte,
            end_byte,
        } => (
            -32000,
            "rename_name_conflict",
            json!({"start_byte":start_byte,"end_byte":end_byte}),
        ),
        LocalRenameError::PermissionDenied(permission) => (
            -32000,
            "rename_permission_denied",
            json!({"permission":permission}),
        ),
        LocalRenameError::SearchIncomplete => (-32000, "rename_search_incomplete", json!({})),
        LocalRenameError::InconsistentOccurrences => (-32603, "rename_inconsistent", json!({})),
    };
    details["snapshot"] = dto::snapshot(session, id);
    protocol::error_value(code, message, details)
}
