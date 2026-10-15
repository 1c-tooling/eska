//! Native standalone rules use current admitted metadata without changing the parse-only contract.

use bsl_metadata::QueryMetadataResolver;
use eska::{
    analysis_core::{InputOrigin, InputRequest},
    project_core::source_format::SourcePath,
};
use ide::{Diagnostic, DiagnosticCode, DiagnosticsConfig, Locale};

use super::{QuerySyntaxError, QueryTextInput, QueryTextState, syntax};
use crate::SessionRevision;

/// Native rules are checked against current metadata; parameter and consumer semantics remain outside scope.
#[derive(Clone, Debug)]
pub struct QueryAnalysisResult {
    pub input_revision: SessionRevision,
    pub source: SourcePath,
    pub source_sha256: String,
    pub origin: InputOrigin,
    pub enabled_rules: Vec<DiagnosticCode>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Rule preflight precedes source reads; the native dispatcher remains the authoritative rule inventory.
#[must_use]
pub fn query_analysis_enabled(config: &DiagnosticsConfig) -> bool {
    ide::sdbl_query_codes()
        .into_iter()
        .any(|code| !config.is_disabled(code))
}

impl QueryTextInput {
    /// Validate the same leaf input with every enabled standalone native rule and an admitted resolver.
    pub(crate) fn query_analysis(
        &self,
        config: &DiagnosticsConfig,
        resolver: &dyn QueryMetadataResolver,
        request: &InputRequest,
    ) -> Result<QueryAnalysisResult, QuerySyntaxError> {
        if request.cancellation.is_cancelled() {
            return Err(QuerySyntaxError::Cancelled);
        }
        let text = match &self.snapshot.state {
            QueryTextState::Ready(text) => text,
            QueryTextState::Pending => return Err(QuerySyntaxError::Pending),
            QueryTextState::Unavailable(error) => {
                return Err(QuerySyntaxError::Unavailable(error.clone()));
            }
        };
        let enabled_rules = ide::sdbl_query_codes()
            .into_iter()
            .filter(|code| !config.is_disabled(*code))
            .collect::<Vec<_>>();
        let mut diagnostics = ide::validate_query_text(config, Some(resolver), text);
        if config.locale == Locale::En {
            if diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::QueryParseError)
            {
                syntax::translate_parse_errors(text, &mut diagnostics);
            }
            for diagnostic in &mut diagnostics {
                if let Some(message) = english_rule(diagnostic.code) {
                    diagnostic.message = message.into();
                }
            }
        }
        let result = syntax::publish(self, config, text, diagnostics, request)?;
        Ok(QueryAnalysisResult {
            input_revision: result.input_revision,
            source: result.source,
            source_sha256: result.source_sha256,
            origin: result.origin,
            enabled_rules,
            diagnostics: result.diagnostics,
        })
    }
}

/// Native rule identity and ranges stay intact; English avoids parsing localized upstream messages.
const fn english_rule(code: DiagnosticCode) -> Option<&'static str> {
    Some(match code {
        DiagnosticCode::AmbiguousFieldInQuery => "Ambiguous query field",
        DiagnosticCode::DuplicateAliasInQuery => "A table alias is declared more than once",
        DiagnosticCode::AssignAliasFieldsInQuery => "Assign an alias to the query output field",
        DiagnosticCode::FieldsFromJoinsWithoutIsNull => "Handle NULL for fields from an outer join",
        DiagnosticCode::FullOuterJoinQuery => "Review the full outer join",
        DiagnosticCode::IncorrectUseLikeInQuery => "Invalid use of LIKE in the query",
        DiagnosticCode::JoinWithSubQuery => "A join uses a subquery",
        DiagnosticCode::JoinWithVirtualTable => "A join uses a virtual table",
        DiagnosticCode::LogicalOrInJoinQuerySection => "Logical OR in a join condition",
        DiagnosticCode::LogicalOrInTheWhereSectionOfQuery => "Logical OR in a query filter",
        DiagnosticCode::MultilineStringInQuery => "A query string literal spans multiple lines",
        DiagnosticCode::QueryNestedFieldsByDot => "Nested query field access",
        DiagnosticCode::QueryToMissingMetadata => "The query references a missing metadata table",
        DiagnosticCode::UnknownFieldInQuery => "Unknown query field",
        DiagnosticCode::UnlimitedLengthStringUsageInQuery => {
            "An unlimited-length string is used in the query"
        }
        DiagnosticCode::RefOveruse => "Unnecessary reference field access",
        DiagnosticCode::SelectTopWithoutOrderBy => "TOP is used without ORDER BY",
        DiagnosticCode::UnionAll => "Consider UNION ALL",
        DiagnosticCode::UsingLikeInQuery => "Review the LIKE query filter",
        DiagnosticCode::VirtualTableCallWithoutParameters => {
            "A virtual table is used without parameters"
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticCode, english_rule};

    /// A newly enabled native rule cannot silently leak untranslated upstream prose.
    #[test]
    fn every_non_parse_native_rule_has_an_english_message() {
        for code in ide::sdbl_query_codes() {
            if code != DiagnosticCode::QueryParseError {
                assert!(english_rule(code).is_some(), "{code:?}");
            }
        }
    }
}
