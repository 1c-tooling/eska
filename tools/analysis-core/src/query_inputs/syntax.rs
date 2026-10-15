//! Standalone native query diagnostics have explicit syntax-only coverage and late publication guards.

use std::collections::BTreeMap;

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    project_core::source_format::SourcePath,
};
use ide::{Diagnostic, DiagnosticCode, DiagnosticsConfig, Locale};

use super::{QueryTextInput, QueryTextState};
use crate::SessionRevision;

/// Syntax-only never grants metadata, parameter-type or owner-consumer completeness.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuerySyntaxScope {
    SyntaxOnly,
}

/// Identity, code and UTF-8 ranges are independent of human locale and travel with this exact input.
#[derive(Clone, Debug)]
pub struct QuerySyntaxResult {
    pub input_revision: SessionRevision,
    pub source: SourcePath,
    pub source_sha256: String,
    pub origin: InputOrigin,
    pub scope: QuerySyntaxScope,
    pub parse_enabled: bool,
    pub diagnostics: Vec<Diagnostic>,
}

/// Unread input, cancellation and inconsistent native ranges cannot become an empty successful result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QuerySyntaxError {
    Pending,
    Unavailable(InputFailure),
    Cancelled,
    FingerprintMissing,
    DiagnosticRangeInvalid {
        code: DiagnosticCode,
        start_byte: u32,
        end_byte: u32,
    },
}

impl QueryTextInput {
    /// Run only native query parse diagnostics; contextual and quality rules require separate admission.
    ///
    /// Cancellation/deadline publication checks run before and after native work. The pinned parser
    /// accepts no cancellation hook, so this method does not promise interruption inside parsing.
    /// # Errors
    /// Rejects current pending/unavailable input, cancellation or an invalid native diagnostic range.
    pub fn query_syntax(
        &self,
        config: &DiagnosticsConfig,
        request: &InputRequest,
    ) -> Result<QuerySyntaxResult, QuerySyntaxError> {
        if request.cancellation.is_cancelled() {
            return Err(QuerySyntaxError::Cancelled);
        }
        let text = match &self.snapshot.state {
            QueryTextState::Pending => return Err(QuerySyntaxError::Pending),
            QueryTextState::Unavailable(error) => {
                return Err(QuerySyntaxError::Unavailable(error.clone()));
            }
            QueryTextState::Ready(text) => text,
        };
        let config = syntax_profile(config);
        let mut diagnostics = ide::validate_query_text(&config, None, text);
        if request.cancellation.is_cancelled() {
            return Err(QuerySyntaxError::Cancelled);
        }
        if config.locale == Locale::En
            && diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::QueryParseError)
        {
            translate_parse_errors(text, &mut diagnostics);
        }
        publish(self, &config, text, diagnostics, request)
    }
}

/// Match the BSL syntax batch: caller policy selects parse errors, while all other rules stay outside scope.
pub(super) fn syntax_profile(config: &DiagnosticsConfig) -> DiagnosticsConfig {
    let mut result = config.clone();
    result.only_enabled = Some(if config.is_disabled(DiagnosticCode::QueryParseError) {
        vec![]
    } else {
        vec![DiagnosticCode::QueryParseError]
    });
    result
}

/// Preserve native codes/ranges while using structured English or a generic fallback for synthetic errors.
fn translate_parse_errors(text: &str, diagnostics: &mut [Diagnostic]) {
    let parse = parser::parse_sdbl(text);
    let messages: BTreeMap<_, _> = parse
        .errors()
        .iter()
        .map(|error| {
            (
                (
                    u32::from(error.range().start()),
                    u32::from(error.range().end()),
                    error.structured().format_ru(),
                ),
                error.structured().format_en(),
            )
        })
        .collect();
    for diagnostic in diagnostics
        .iter_mut()
        .filter(|diagnostic| diagnostic.code == DiagnosticCode::QueryParseError)
    {
        let key = (
            u32::from(diagnostic.range.start()),
            u32::from(diagnostic.range.end()),
            diagnostic.message.clone(),
        );
        diagnostic.message = messages
            .get(&key)
            .cloned()
            .unwrap_or_else(|| "Syntax error".to_owned());
    }
}

/// A native call completed before cancellation still cannot publish a current successful result.
pub(super) fn publish(
    input: &QueryTextInput,
    config: &DiagnosticsConfig,
    text: &str,
    diagnostics: Vec<Diagnostic>,
    request: &InputRequest,
) -> Result<QuerySyntaxResult, QuerySyntaxError> {
    if request.cancellation.is_cancelled() {
        return Err(QuerySyntaxError::Cancelled);
    }
    for diagnostic in &diagnostics {
        let start_byte = u32::from(diagnostic.range.start());
        let end_byte = u32::from(diagnostic.range.end());
        if start_byte > end_byte || text.get(start_byte as usize..end_byte as usize).is_none() {
            return Err(QuerySyntaxError::DiagnosticRangeInvalid {
                code: diagnostic.code,
                start_byte,
                end_byte,
            });
        }
    }
    if request.cancellation.is_cancelled() {
        return Err(QuerySyntaxError::Cancelled);
    }
    Ok(QuerySyntaxResult {
        input_revision: input.snapshot.revision,
        source: input.declaration.source.clone(),
        source_sha256: input
            .snapshot
            .source_sha256
            .clone()
            .ok_or(QuerySyntaxError::FingerprintMissing)?,
        origin: input.snapshot.origin,
        scope: QuerySyntaxScope::SyntaxOnly,
        parse_enabled: !config.is_disabled(DiagnosticCode::QueryParseError),
        diagnostics,
    })
}
