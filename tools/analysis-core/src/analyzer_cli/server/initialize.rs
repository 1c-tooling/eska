//! Negotiate capabilities before publishing a selected project session to the worker.

use eska::{
    project_core::eska_source::EskaSource,
    tool_cli::{self, rpc_framing},
};
use serde_json::{Value, json};

use super::super::{
    completion, diagnostics, document_symbols, documents, dto, edit_authority, hover,
    kernel_memory, language, metadata_locations, project_changes, property_edits, protocol,
    references, rename, request_control, signature_help, stream, watched_files,
    watcher_registration,
};
use super::{Phase, Server};
use crate::{AnalysisSession, analysis_settings, batch::BatchProfile};

impl Server<'_> {
    /// Reject an incompatible ESKA protocol before project discovery or source reads.
    pub(super) fn initialize(&mut self, params: &Value) -> Result<Value, Value> {
        self.check_initialize(params)?;
        let selected = tool_cli::selected(&self.cli.project_dir, self.cli.project.as_deref())
            .map_err(|failure| {
                protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
            })?;
        let workspace = selected.workspace_root;
        let locale = self.localizer.locale();
        let profile: BatchProfile = self.cli.profile.into();
        let settings = analysis_settings::load(selected.project.root(), None, locale, profile);
        let source = EskaSource::open(selected.project, selected.scope).map_err(|error| {
            protocol::error_value(-32000, error.code, json!({"location":error.location}))
        })?;
        let project_inputs = project_changes::Inputs::selected(&source, workspace.as_deref());
        let watcher =
            watcher_registration::Registration::selected(&project_inputs, source.reader(), params)?;
        let edit_plans = edit_authority::Authority::selected(
            &source,
            self.cli.accepted_base_sha256.as_deref(),
            params,
        )?;
        let options = profile.session_options(self.cli.max_retained_source_bytes);
        let session =
            AnalysisSession::from_reader(source.into_reader(), options).map_err(|_| {
                protocol::error_value(-32000, "analysis_session_open_failed", Value::Null)
            })?;
        let session_id = bsl_metadata::Uuid::new_v4().to_string();
        let hierarchical_symbols = document_symbols::hierarchical(params);
        let hover_format = hover::format(params);
        let signature = signature_help::Capabilities::negotiate(params);
        let mut result = json!({
            "capabilities":{"textDocumentSync":{"openClose":true,"change":1,"save":{"includeText":false}},
                "positionEncoding":"utf-16",
                "experimental":{"eska":{
                "protocolVersion":protocol::VERSION,
                "sourceFormat":"eska", "scope":"selected-project",
                "metadata":{"owners":true, "read":true, "refresh":true,
                    "locate":{"method":metadata_locations::METHOD,"scope":"current-ready-owner","positionEncoding":"utf-16","lazyRead":false}},
                "documentSync":true, "roleRightsSync":true, "formDialogSync":true, "nestedFormModels":true, "predefinedDataSync":true, "targetedCompletionMetadata":true, "managerModuleCompletion":true, "commonModuleCompletion":true, "languageQueries":true, "requestCancellation":true,
                "cancellableMethods":request_control::METHODS,"maxQueuedMessages":stream::MAX_QUEUED_MESSAGES,
                "requestTimeoutSeconds":self.cli.timeout_seconds,
                "languageMethods":language::METHODS,
                "kernelMemory":kernel_memory::capability(),
                "analysis":diagnostics::availability(&settings, profile),
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
                "inputPreparation":"direct-member-targets"});
            result["capabilities"]["experimental"]["eska"]["hover"] = json!({
                "scope":"resolved-bsl-symbol","contentFormat":hover_format.as_str(),
                "incompleteResults":"error"});
            result["capabilities"]["experimental"]["eska"]["documentSymbols"] = json!({
                "scope":"current-bsl-module","profile":"structure",
                "result":if hierarchical_symbols { "hierarchical" } else { "flat" }});
            result["capabilities"]["experimental"]["eska"]["definition"] = json!({"scope":"ready-bsl-and-canonical-metadata","metadataTargets":"typed-objects-and-members","incompleteResults":"error"});
        }
        self.completion = completion::Capabilities::negotiate(params);
        self.hierarchical_symbols = hierarchical_symbols;
        self.hover_format = hover_format;
        self.signature = signature;
        self.project_inputs = Some(project_inputs);
        self.watcher = watcher;
        self.edit_plans = edit_plans;
        self.session = Some(session);
        self.settings = Some(settings);
        self.session_id = session_id;
        self.phase = Phase::Initializing;
        Ok(result)
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
        Ok(())
    }
}
