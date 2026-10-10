//! Document pulls inspect current metadata or the same explicit BSL syntax profile as the checker.

mod metadata;
#[cfg(test)]
mod tests;

use std::time::Duration;

use eska::{
    analysis_core::InputRequest,
    tool_cli::{Failure, localization::Localizer},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AnalysisSession, Diagnostic, RequestCancellation, analysis_settings::Settings,
    batch::syntax_profile,
};

use super::{
    documents, dto, language,
    positions::Positions,
    protocol,
    request_control::{self, Method},
};

pub const IDENTIFIER: &str = "eska-syntax";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Params {
    text_document: language::Document,
    identifier: Option<String>,
    #[serde(rename = "previousResultId")]
    _previous_result_id: Option<String>,
    #[serde(rename = "workDoneToken")]
    _work_done_token: Option<protocol::Id>,
    #[serde(rename = "partialResultToken")]
    _partial_result_token: Option<protocol::Id>,
    pub eska: Option<language::Context>,
}

/// Metadata checks are independent of BSL rules; one request still uses one shared timer and token.
pub struct Options<'a> {
    pub request: &'a InputRequest,
    pub cancellation: &'a RequestCancellation,
    pub timeout: Duration,
    pub localizer: &'a Localizer,
}

impl<'a> Options<'a> {
    /// Freeze the current request's IO bound, stop control and presentation together.
    pub const fn new(
        request: &'a InputRequest,
        cancellation: &'a RequestCancellation,
        timeout: Duration,
        localizer: &'a Localizer,
    ) -> Self {
        Self {
            request,
            cancellation,
            timeout,
            localizer,
        }
    }
}

/// A configuration failure affects language analysis without disabling the independent metadata tree.
pub fn availability(settings: &Result<Settings, Failure>) -> Value {
    match settings {
        Ok(settings) => json!({"profiles":["syntax","metadata"], "metadataEnabled":true,
            "syntaxEnabled":!settings.diagnostics.is_disabled(crate::DiagnosticCode::ParseError),
            "configSha256":settings.sha256, "configPath":settings.path, "issue":null}),
        Err(error) => {
            json!({"profiles":["syntax","metadata"],"metadataEnabled":true,"syntaxEnabled":false,"configSha256":null,"configPath":null,
            "issue":{"code":error.code,"location":error.location}})
        }
    }
}

/// Standard clients use the current ordered input; ESKA clients may require an exact snapshot too.
pub fn pull(
    session: &mut AnalysisSession,
    id: &str,
    params: &Params,
    settings: &Result<Settings, Failure>,
    options: &Options<'_>,
) -> Result<Value, Value> {
    if params
        .identifier
        .as_deref()
        .is_some_and(|identifier| identifier != IDENTIFIER)
    {
        return Err(protocol::error_value(
            -32602,
            "analysis_profile_unsupported",
            json!({"supported":IDENTIFIER}),
        ));
    }
    let path = documents::source_path(session, &params.text_document.uri)?;
    if session.owner(&path).is_some() {
        let deadline = request_control::start(
            session,
            id,
            Method::Language(language::Method::Diagnostic),
            options.cancellation,
            options.timeout,
        )?;
        let result = metadata::pull(session, id, &path, options);
        return request_control::finish(
            session,
            id,
            Method::Language(language::Method::Diagnostic),
            &deadline,
            result,
        );
    }
    let settings = settings.as_ref().map_err(|failure| {
        protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
    })?;
    let (config, syntax_enabled) = syntax_profile(&settings.diagnostics);
    if !syntax_enabled {
        return Err(protocol::error_value(
            -32000,
            "analysis_profile_disabled",
            json!({"profile":"syntax"}),
        ));
    }
    let deadline = request_control::start(
        session,
        id,
        Method::Language(language::Method::Diagnostic),
        options.cancellation,
        options.timeout,
    )?;
    let result = analyze(
        session,
        id,
        &path,
        settings,
        &config,
        options.request,
        options.cancellation,
    );
    request_control::finish(
        session,
        id,
        Method::Language(language::Method::Diagnostic),
        &deadline,
        result,
    )
}

/// Input reads, kernel queries and range conversion all run within the same cooperative deadline.
fn analyze(
    session: &mut AnalysisSession,
    id: &str,
    path: &eska::project_core::source_format::SourcePath,
    settings: &Settings,
    config: &crate::DiagnosticsConfig,
    request: &InputRequest,
    cancellation: &RequestCancellation,
) -> Result<Value, Value> {
    language::load_module(session, path, request)?;
    if let Some(reason) = cancellation.stop_reason() {
        return Err(request_control::stopped(
            session,
            id,
            Method::Language(language::Method::Diagnostic),
            reason,
        ));
    }
    let result = session
        .module_diagnostics(path, session.revision(), config, &cancellation.query)
        .map_err(|error| language::query_error(session, id, &error))?;
    let items = session
        .with_module_text(path, session.revision(), &cancellation.query, |text| {
            items(&result.value, text)
        })
        .map_err(|error| language::query_error(session, id, &error))?
        .value?;
    let summary = session.summary();
    Ok(json!({"kind":"full",
        "resultId":format!("{id}:{}:{}:{}",result.revision,settings.sha256,result.source_sha256),
        "items":items,"eska":{"snapshot":dto::snapshot(session,id),"profile":"syntax",
        "source":result.source,"source_sha256":result.source_sha256,"config_sha256":settings.sha256,
        "coverage":{"module_syntax_complete":true,"metadata_inputs_complete":summary.inputs.metadata_input_complete,
            "metadata_models_complete":summary.descriptor_models_complete,"module_inputs_complete":result.module_inputs_complete,
            "semantic_complete":false},"resources":session.resources()}}))
}

/// Build UTF-16 ranges once and preserve canonical byte offsets for checker parity and future plans.
fn items(diagnostics: &[Diagnostic], text: &str) -> Result<Vec<Value>, Value> {
    let offsets = diagnostics
        .iter()
        .flat_map(|diagnostic| {
            [
                u32::from(diagnostic.range.start()),
                u32::from(diagnostic.range.end()),
            ]
        })
        .collect::<Vec<_>>();
    let positions = Positions::new(text, &offsets)?;
    diagnostics.iter().map(|diagnostic| {
        let start = u32::from(diagnostic.range.start());
        let end = u32::from(diagnostic.range.end());
        let severity = match diagnostic.severity {
            ide::Severity::Blocker | ide::Severity::Critical | ide::Severity::Major | ide::Severity::Error => 1,
            ide::Severity::Warning => 2,
            ide::Severity::Information => 3,
            ide::Severity::Hint => 4,
        };
        Ok(json!({"range":positions.range(start,end)?,"severity":severity,"code":diagnostic.code.as_str(),
            "source":"eska-analyzer","message":diagnostic.message,"data":{"start_byte":start,"end_byte":end,
                "original_severity":diagnostic.severity.as_str()}}))
    }).collect()
}
