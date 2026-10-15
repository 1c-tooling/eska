//! Negotiate capabilities before publishing a selected project session to the worker.

use eska::{
    project_core::eska_source::EskaSource,
    tool_cli::{self, rpc_framing},
};
use serde_json::{Value, json};

use super::super::{
    args::Cli, completion, diagnostics, document_symbols, documents, dto, edit_authority,
    external_context, hover, kernel_memory, language, metadata_locations, navigation::Navigation,
    permissions, project_changes, property_edits, protocol, query_syntax,
    read_permissions::ReadPermissions, references, rename, request_control, signature_help, stream,
    watched_files, watcher_registration,
};
use super::{Phase, Server};
use crate::{AnalysisScope, AnalysisSession, analysis_settings, batch::BatchProfile};

impl Server<'_> {
    /// Reject an incompatible ESKA protocol before project discovery or source reads.
    pub(super) fn initialize(&mut self, params: &Value) -> Result<Value, Value> {
        self.check_initialize(params)?;
        let profile: BatchProfile = self.cli.profile.into();
        let (sources, settings) = open_sources(self.cli, self.localizer.locale(), profile)?;
        let project_inputs = composition_controls(&sources)?;
        let readers = sources.iter().map(EskaSource::reader).collect::<Vec<_>>();
        let watcher =
            watcher_registration::Registration::sources(&project_inputs, &readers, params)?;
        let (scope, edit_plans) =
            selected_admission(sources.selected(), self.cli, params, sources.configured())?;
        let (navigation, read_permissions) = capture_navigation(sources.selected(), self.cli)?;
        let options = profile.session_options(self.cli.max_retained_source_bytes);
        let session = open_session(sources, &settings, options, scope)?;
        project_inputs.require_idle()?;
        let session_id = bsl_metadata::Uuid::new_v4().to_string();
        let hierarchical_symbols = document_symbols::hierarchical(params);
        let hover_format = hover::format(params);
        let signature = signature_help::Capabilities::negotiate(params);
        let mut result = json!({
            "capabilities":{"textDocumentSync":{"openClose":true,"change":1,"save":{"includeText":false}},
                "positionEncoding":"utf-16",
                "experimental":{"eska":{
                "protocolVersion":protocol::VERSION,
                "sourceCoreVersion":tool_cli::PACKAGE_VERSION,
                "sourceFormat":"eska", "scope":"selected-project",
                "project":navigation.project(),
                "metadata":{"owners":true, "read":true, "refresh":true,
                    "permissions":permissions::capability(),
                    "locate":{"method":metadata_locations::METHOD,"scope":"current-ready-owner","positionEncoding":"utf-16","lazyRead":false}},
                "documentSync":true, "roleRightsSync":true, "formDialogSync":true, "nestedFormModels":true, "predefinedDataSync":true, "targetedCompletionMetadata":true, "managerModuleCompletion":true, "commonModuleCompletion":true, "languageQueries":true, "requestCancellation":true,
                "cancellableMethods":request_control::METHODS,"maxQueuedMessages":stream::MAX_QUEUED_MESSAGES,
                "requestTimeoutSeconds":self.cli.timeout_seconds,
                "languageMethods":language::METHODS,
                "kernelMemory":kernel_memory::capability(),
                "analysis":diagnostics::availability(&settings, profile),
                "querySyntax":query_syntax::capability(query_syntax::negotiate(params)?, &settings),
                "inputNotifications":["eska/inputChanged", "eska/inputRejected", watched_files::EVENT, project_changes::EVENT, watcher_registration::EVENT],
                "watchedFiles":watched_files::capabilities(watcher.is_some()),
                "projectChanges":project_inputs.capability()?,
                "watchRegistration":watcher_registration::capability(params),
                "editPlans":edit_plans.is_some(), "locationEncoding":"utf-8-bytes",
                "maxRequestBytes":documents::MAX_REQUEST,
                "maxResponseBytes":rpc_framing::MAX_RESPONSE,
                "snapshot":dto::snapshot(&session, &session_id),
            }}},
            "serverInfo":{"name":"eska-analyzer", "version":env!("CARGO_PKG_VERSION")},
        });
        if let Some(composition) = session.composition_summary() {
            result["capabilities"]["experimental"]["eska"]["composition"] = json!(composition);
        }
        if edit_plans.is_some() {
            result["capabilities"]["experimental"]["eska"]["propertyEditPlans"] =
                json!({"method":property_edits::METHOD,"scope":"open-document","versioned":true});
        }
        result["capabilities"]["diagnosticProvider"] = json!({
            "identifier":diagnostics::identifier(profile), "interFileDependencies":profile==BatchProfile::Semantic,
            "workspaceDiagnostics":false, "workDoneProgress":false});
        if settings.is_ok() {
            if edit_plans.is_some() {
                rename::advertise(&mut result, params);
            }
            result["capabilities"]["completionProvider"] =
                json!({"resolveProvider":false,"triggerCharacters":["."],"workDoneProgress":false});
            result["capabilities"]["definitionProvider"] = json!(true);
            result["capabilities"]["documentSymbolProvider"] = json!(true);
            result["capabilities"]["hoverProvider"] = json!(true);
            result["capabilities"]["referencesProvider"] = json!(true);
            result["capabilities"]["experimental"]["eska"]["references"] = references::capability();
            result["capabilities"]["signatureHelpProvider"] = signature.provider();
            result["capabilities"]["experimental"]["eska"]["signatureHelp"] = json!({
                "scope":"current-bsl-call","documentationFormat":signature.documentation.as_str(),
                "parameterLabel":if signature.parameters.label_offsets {"utf16-offsets"} else {"string"},
                "incompleteResults":"error","clientHints":"recompute",
                "inputPreparation":if session.composition_sha256().is_some() {"visible-current-composition"} else {"direct-member-targets"}});
            result["capabilities"]["experimental"]["eska"]["hover"] = json!({
                "scope":"resolved-bsl-symbol","contentFormat":hover_format.as_str(),
                "incompleteResults":"error","inputPreparation":if session.composition_sha256().is_some() {"visible-current-composition"} else {"direct-member-callees"}});
            result["capabilities"]["experimental"]["eska"]["documentSymbols"] = json!({
                "scope":"current-bsl-module","profile":"structure",
                "result":if hierarchical_symbols { "hierarchical" } else { "flat" }});
            result["capabilities"]["experimental"]["eska"]["definition"] = json!({"scope":"ready-bsl-and-canonical-metadata","metadataTargets":"typed-objects-and-members","incompleteResults":"error"});
        }
        advertise_scope(&mut result, scope);
        external_context::advertise(&mut result, scope);
        self.publish_editor_capabilities(params, hierarchical_symbols, hover_format, signature)?;
        self.project_inputs = Some(project_inputs);
        self.watcher = watcher;
        self.edit_plans = edit_plans;
        self.navigation = Some(navigation);
        self.read_permissions = Some(read_permissions);
        self.session = Some(session);
        self.settings = Some(settings);
        self.session_id = session_id;
        self.phase = Phase::Initializing;
        Ok(result)
    }

    /// Freeze the negotiated editor transport state with its query synchronization boundary.
    fn publish_editor_capabilities(
        &mut self,
        params: &Value,
        hierarchical_symbols: bool,
        hover_format: hover::Format,
        signature: signature_help::Capabilities,
    ) -> Result<(), Value> {
        self.completion = completion::Capabilities::negotiate(params);
        self.hierarchical_symbols = hierarchical_symbols;
        self.hover_format = hover_format;
        self.signature = signature;
        self.documents = documents::Documents::with_query_syntax(query_syntax::negotiate(params)?);
        Ok(())
    }

    /// All negotiation preconditions are checked before opening the selected source or changing server state.
    fn check_initialize(&self, params: &Value) -> Result<(), Value> {
        if self.phase != Phase::New {
            return Err(protocol::error_value(
                -32600,
                "already_initialized",
                Value::Null,
            ));
        }
        if !params.get("capabilities").is_some_and(Value::is_object) {
            return Err(protocol::error_value(
                -32602,
                "rpc_invalid_params",
                Value::Null,
            ));
        }
        if params.pointer("/initializationOptions/eska/protocolVersion")
            != Some(&json!(protocol::VERSION))
        {
            return Err(protocol::error_value(
                -32602,
                "protocol_version_unsupported",
                json!({"supported":[protocol::VERSION]}),
            ));
        }
        query_syntax::negotiate(params)?;
        Ok(())
    }
}

/// Selection retains each source's actual project and workspace control provenance.
fn open_sources(
    cli: &Cli,
    locale: tool_cli::localization::Locale,
    profile: BatchProfile,
) -> Result<
    (
        crate::composition::cli::Sources,
        Result<analysis_settings::Settings, tool_cli::Failure>,
    ),
    Value,
> {
    let selected =
        tool_cli::selected(&cli.project_dir, cli.project.as_deref()).map_err(|failure| {
            protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
        })?;
    let settings = analysis_settings::load(selected.project.root(), None, locale, profile);
    let source =
        EskaSource::open(selected.project, selected.scope).map_err(|error| source_error(&error))?;
    let sources = crate::composition::cli::Sources::open(
        source,
        selected.workspace_root,
        settings.as_ref().ok().map(|settings| &settings.source),
    )
    .map_err(|failure| {
        protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
    })?;
    Ok((sources, settings))
}

/// All admitted control chains participate even when their modules are not semantically visible.
fn composition_controls(
    sources: &crate::composition::cli::Sources,
) -> Result<project_changes::Inputs, Value> {
    let mut controls = sources.controls();
    let (source, workspace) = controls
        .next()
        .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
    let mut inputs = project_changes::Inputs::selected(source, workspace);
    for (source, workspace) in controls {
        inputs.include(source, workspace);
    }
    Ok(inputs)
}

/// Every verified reader moves into the same host before that host is exposed to a query.
fn open_session(
    sources: crate::composition::cli::Sources,
    settings: &Result<analysis_settings::Settings, tool_cli::Failure>,
    options: crate::SessionOptions,
    scope: AnalysisScope,
) -> Result<AnalysisSession, Value> {
    let session = if sources.configured() {
        let config = &settings
            .as_ref()
            .map_err(|failure| protocol::error_value(-32000, failure.code, Value::Null))?
            .source;
        let composition = sources.into_composition(config).map_err(|failure| {
            protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
        })?;
        AnalysisSession::from_composition(composition, options)
    } else {
        let reader = sources
            .into_single()
            .map_err(|failure| protocol::error_value(-32000, failure.code, Value::Null))?;
        AnalysisSession::from_reader_with_scope(reader, options, scope)
    };
    session.map_err(|error| session_open_error(&error))
}

/// Select one source scope and its edit authority together, preserving canonical pin checks.
fn selected_admission(
    source: &EskaSource,
    cli: &Cli,
    params: &Value,
    configured: bool,
) -> Result<(AnalysisScope, Option<edit_authority::Authority>), Value> {
    let profile: BatchProfile = cli.profile.into();
    let scope = if configured {
        AnalysisScope::CanonicalConfiguration
    } else {
        profile
            .session_scope(source.reader())
            .map_err(|error| session_open_error(&error))?
    };
    let authority = if scope == AnalysisScope::CanonicalConfiguration {
        edit_authority::Authority::selected(source, cli.accepted_base_sha256.as_deref(), params)?
    } else {
        None
    };
    Ok((scope, authority))
}

/// Source declarations share one structured protocol error without leaking native payload text.
fn source_error(error: &eska::project_core::source_format::Error) -> Value {
    protocol::error_value(-32000, error.code, json!({"location":error.location}))
}

/// Navigation and policy retain the same selected source before its reader moves into the session.
fn capture_navigation(
    source: &EskaSource,
    cli: &Cli,
) -> Result<(Navigation, ReadPermissions), Value> {
    Ok((
        Navigation::selected(source).map_err(|error| source_error(&error))?,
        permissions::selected(source, cli.accepted_base_sha256.as_deref())?,
    ))
}

/// Local syntax advertises only operations that cannot infer unavailable metadata or produce edit plans.
fn advertise_scope(result: &mut Value, scope: AnalysisScope) {
    result["capabilities"]["experimental"]["eska"]["analysisScope"] = json!(scope);
    if scope != AnalysisScope::LocalSyntax {
        return;
    }
    if let Some(capabilities) = result["capabilities"].as_object_mut() {
        for name in [
            "completionProvider",
            "definitionProvider",
            "hoverProvider",
            "referencesProvider",
            "signatureHelpProvider",
            "renameProvider",
        ] {
            capabilities.remove(name);
        }
    }
    let eska = &mut result["capabilities"]["experimental"]["eska"];
    eska["languageMethods"] = json!(language::LOCAL_METHODS);
    let mut cancellable = language::LOCAL_METHODS.to_vec();
    cancellable.push(permissions::METHOD);
    eska["cancellableMethods"] = json!(cancellable);
    for name in [
        "nestedFormModels",
        "targetedCompletionMetadata",
        "managerModuleCompletion",
        "commonModuleCompletion",
    ] {
        eska[name] = json!(false);
    }
    if let Some(eska) = eska.as_object_mut() {
        for name in [
            "references",
            "signatureHelp",
            "hover",
            "definition",
            "localRename",
            "propertyEditPlans",
        ] {
            eska.remove(name);
        }
    }
}

/// A newly admitted source profile keeps its explicit refusal; other setup errors retain the protocol.
fn session_open_error(error: &crate::SessionError) -> Value {
    match error {
        crate::SessionError::UnsupportedProjectType(project_type) => protocol::error_value(
            -32000,
            "analysis_scope_unsupported",
            json!({"location":"project.type","project_type":project_type.as_str(),"supported_types":["configuration"]}),
        ),
        _ => protocol::error_value(-32000, "analysis_session_open_failed", Value::Null),
    }
}
