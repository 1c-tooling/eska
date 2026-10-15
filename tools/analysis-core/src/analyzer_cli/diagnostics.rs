//! Document pulls inspect current metadata or the same selected BSL profile as the checker.

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
    AnalysisSession, Diagnostic, RequestCancellation,
    analysis_settings::Settings,
    batch::{BatchProfile, syntax_profile},
};

use super::{
    documents, dto, language,
    positions::Positions,
    protocol,
    request_control::{self, Method},
};

pub const IDENTIFIER: &str = "eska-syntax";

/// A process has one negotiated BSL profile, so incompatible cached client pulls are rejected.
pub const fn identifier(profile: BatchProfile) -> &'static str {
    match profile {
        BatchProfile::Syntax => IDENTIFIER,
        BatchProfile::Semantic => "eska-semantic",
    }
}

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
    pub profile: BatchProfile,
    pub query_syntax: bool,
    pub query_analysis: bool,
}

impl<'a> Options<'a> {
    /// Freeze the current request's IO bound, stop control and presentation together.
    pub const fn new(
        request: &'a InputRequest,
        cancellation: &'a RequestCancellation,
        timeout: Duration,
        localizer: &'a Localizer,
        profile: BatchProfile,
    ) -> Self {
        Self {
            request,
            cancellation,
            timeout,
            localizer,
            profile,
            query_syntax: false,
            query_analysis: false,
        }
    }

    /// The server's explicit query negotiation is independent of its BSL profile.
    pub const fn with_query_syntax(mut self, negotiated: bool) -> Self {
        self.query_syntax = negotiated;
        self
    }

    /// Full native query rules require their own explicit client admission.
    pub const fn with_query_analysis(mut self, negotiated: bool) -> Self {
        self.query_analysis = negotiated;
        self
    }
}

/// A configuration failure affects language analysis without disabling the independent metadata tree.
pub fn availability(settings: &Result<Settings, Failure>, profile: BatchProfile) -> Value {
    let mut value = match settings {
        Ok(settings) => json!({"profiles":["syntax","metadata"], "metadataEnabled":true,
            "syntaxEnabled":!settings.diagnostics.is_disabled(crate::DiagnosticCode::ParseError),
            "configSha256":settings.sha256, "configPath":settings.path, "issue":null}),
        Err(error) => {
            json!({"profiles":["syntax","metadata"],"metadataEnabled":true,"syntaxEnabled":false,"configSha256":null,"configPath":null,
            "issue":{"code":error.code,"location":error.location}})
        }
    };
    if profile == BatchProfile::Semantic {
        value["profiles"] = json!(["semantic", "metadata"]);
        value["semanticEnabled"] = json!(
            settings
                .as_ref()
                .is_ok_and(|settings| profile.enabled(&settings.diagnostics))
        );
    }
    value
}

/// Standard clients use the current ordered input; ESKA clients may require an exact snapshot too.
pub fn pull(
    session: &mut AnalysisSession,
    id: &str,
    params: &Params,
    settings: &Result<Settings, Failure>,
    options: &Options<'_>,
) -> Result<Value, Value> {
    let path = documents::source_path(session, &params.text_document.uri)?;
    if session.query_input(&path).is_some() {
        if params.identifier.as_deref() == Some(super::query_analysis::IDENTIFIER)
            || (params.identifier.is_none() && options.query_analysis && !options.query_syntax)
        {
            return super::query_analysis::pull(session, id, &path, settings, options);
        }
        return super::query_syntax::pull(
            session,
            id,
            &path,
            params.identifier.as_deref(),
            settings,
            options,
        );
    }
    if params
        .identifier
        .as_deref()
        .is_some_and(|selected| selected != identifier(options.profile))
    {
        return Err(protocol::error_value(
            -32602,
            "analysis_profile_unsupported",
            json!({"supported":identifier(options.profile)}),
        ));
    }
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
    let config = match options.profile {
        BatchProfile::Syntax => syntax_profile(&settings.diagnostics).0,
        BatchProfile::Semantic => settings.diagnostics.clone(),
    };
    if !options.profile.enabled(&config) {
        return Err(protocol::error_value(
            -32000,
            "analysis_profile_disabled",
            json!({"profile":options.profile}),
        ));
    }
    let deadline = request_control::start(
        session,
        id,
        Method::Language(language::Method::Diagnostic),
        options.cancellation,
        options.timeout,
    )?;
    let result = analyze(session, id, &path, settings, &config, options);
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
    options: &Options<'_>,
) -> Result<Value, Value> {
    if options.profile == BatchProfile::Semantic {
        session
            .load_semantic_inputs(session.revision(), options.request)
            .map_err(|error| documents::request_error(&error))?;
    } else {
        language::load_module(session, path, options.request)?;
    }
    if let Some(reason) = options.cancellation.stop_reason() {
        return Err(request_control::stopped(
            session,
            id,
            Method::Language(language::Method::Diagnostic),
            reason,
        ));
    }
    let diagnostics = match options.profile {
        BatchProfile::Syntax => session.module_syntax_diagnostics(
            path,
            session.revision(),
            config,
            &options.cancellation.query,
        ),
        BatchProfile::Semantic => session.module_diagnostics(
            path,
            session.revision(),
            config,
            &options.cancellation.query,
        ),
    };
    let result = diagnostics.map_err(|error| language::query_error(session, id, &error))?;
    let items = session
        .with_module_text(
            path,
            session.revision(),
            &options.cancellation.query,
            |text| items(&result.value, text, options.profile),
        )
        .map_err(|error| language::query_error(session, id, &error))?
        .value?;
    let summary = session.summary();
    let mut coverage = json!({"module_syntax_complete":!config.is_disabled(crate::DiagnosticCode::ParseError),
        "metadata_inputs_complete":summary.inputs.metadata_input_complete,
        "metadata_models_complete":summary.descriptor_models_complete,"module_inputs_complete":result.module_inputs_complete,
        "semantic_complete":false});
    if options.profile == BatchProfile::Semantic {
        session.trim_batch_caches();
        coverage["semantic_profile_complete"] = json!(
            summary.inputs.metadata_input_complete
                && summary.descriptor_models_complete
                && result.module_inputs_complete
        );
    }
    Ok(json!({"kind":"full",
        "resultId":format!("{id}:{}:{}:{}",result.revision,settings.sha256,result.source_sha256),
        "items":items,"eska":{"snapshot":dto::snapshot(session,id),"profile":options.profile,
        "source":result.source,"source_sha256":result.source_sha256,"config_sha256":settings.sha256,
        "analysis_scope":session.scope(),
        "coverage":coverage,"resources":session.resources()}}))
}

/// Build UTF-16 ranges once and preserve canonical byte offsets for checker parity and future plans.
pub(super) fn items(
    diagnostics: &[Diagnostic],
    text: &str,
    profile: BatchProfile,
) -> Result<Vec<Value>, Value> {
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
        // The native semantic prose has no uniform locale contract; rule identity stays readable.
        let message = match profile {
            BatchProfile::Syntax => diagnostic.message.clone(),
            BatchProfile::Semantic => format!("{} ({})", diagnostic.code.as_str(), diagnostic.severity.as_str()),
        };
        Ok(json!({"range":positions.range(start,end)?,"severity":severity,"code":diagnostic.code.as_str(),
            "source":"eska-analyzer","message":message,"data":{"start_byte":start,"end_byte":end,
                "original_severity":diagnostic.severity.as_str()}}))
    }).collect()
}
