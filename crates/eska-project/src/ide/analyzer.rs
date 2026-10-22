//! Analyzer configuration RPCs share CLI plans and retain a lease independently of the metadata tree.

use std::{sync::atomic::AtomicBool, time::Duration};

use eska_analyzer::{configuration, migration, process::Analyzer};
use eska_project_core::application;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    envelope::{domain, error},
    params,
    server::Server,
};

/// Keep analysis-config requests separate from BSL LSP and from metadata generation numbers.
pub(super) fn is_method(method: &str) -> bool {
    matches!(
        method,
        "analyzer/context"
            | "analyzer/configStatus"
            | "analyzer/configPrepare"
            | "analyzer/migrationPreview"
            | "analyzer/migrationApply"
            | "analyzer/acquire"
            | "analyzer/release"
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    start: params::Path,
    application: Option<String>,
    analyzer: Option<params::Path>,
    timeout_seconds: Option<u64>,
    switch_context: Option<bool>,
    preview_token: Option<String>,
}

impl Server {
    /// A connection can release only its own live lease identity.
    fn release_analyzer(&mut self, args: &Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Release {
            lease_id: String,
        }
        let input: Release = params::decode(args)?;
        if self
            .analyzer_lease
            .as_ref()
            .is_none_or(|(id, _)| *id != input.lease_id)
        {
            return Err(domain(
                "unknown_analyzer_lease",
                json!({"leaseId":input.lease_id}),
            ));
        }
        self.analyzer_lease = None;
        Ok(Value::Null)
    }

    /// Explicit requests may write derived files; ordinary metadata initialization never runs an analyzer.
    pub(super) fn analyzer(
        &mut self,
        method: &str,
        args: &Value,
        cancelled: &AtomicBool,
        connection_closed: &AtomicBool,
        events: &mut Vec<Value>,
    ) -> Result<Value, Value> {
        if method == "analyzer/release" {
            return self.release_analyzer(args);
        }
        params::optional_fields(
            args,
            &[
                "application",
                "analyzer",
                "timeoutSeconds",
                "switchContext",
                "previewToken",
            ],
        )?;
        let input: Input = params::decode(args)?;
        let start = input.start.native()?;
        if !start.is_absolute() || input.application.as_ref().is_some_and(String::is_empty) {
            return Err(error(-32602));
        }
        if method == "analyzer/context" {
            return application::resolve(&start, input.application.as_deref())
                .map(|context| crate::context::document(&context))
                .map_err(|failure| {
                    failure_value(&eska_analyzer::Error::Context(Box::new(failure)), events)
                });
        }
        if method == "analyzer/acquire" {
            if self.analyzer_lease.is_some() {
                return Err(domain("analyzer_already_leased", json!({})));
            }
            let (plan, lease) = configuration::inspect_leased(&start, input.application.as_deref())
                .map_err(|failure| failure_value(&failure, events))?;
            let mut document = crate::analyzer_config::document(&plan, false);
            document["leaseId"] = json!(plan.token);
            self.analyzer_lease = Some((plan.token, lease));
            return Ok(document);
        }
        let plan = configuration::inspect(&start, input.application.as_deref())
            .map_err(|failure| failure_value(&failure, events))?;
        if method == "analyzer/configStatus" {
            return Ok(crate::analyzer_config::document(&plan, false));
        }
        let binary = input
            .analyzer
            .as_ref()
            .ok_or_else(|| error(-32602))?
            .native()?;
        if !binary.is_absolute() {
            return Err(error(-32602));
        }
        let timeout = input.timeout_seconds.unwrap_or(60);
        if !(1..=600).contains(&timeout) {
            return Err(error(-32602));
        }
        let analyzer = Analyzer {
            binary,
            timeout: Duration::from_secs(timeout),
            cancelled,
            connection_closed: Some(connection_closed),
        };
        match method {
            "analyzer/configPrepare" => {
                configuration::prepare(&plan, &analyzer, input.switch_context.unwrap_or(false))
                    .map(|prepared| {
                        crate::analyzer_config::document(
                            &prepared,
                            plan.status != configuration::Status::Current,
                        )
                    })
                    .map_err(|failure| failure_value(&failure, events))
            }
            "analyzer/migrationPreview" => {
                let preview = migration::preview(&start, input.application.as_deref())
                    .map_err(|failure| failure_value(&failure, events))?;
                migration::validate_preview(&preview, &analyzer)
                    .map_err(|failure| failure_value(&failure, events))?;
                Ok(
                    json!({"schema_version":1,"kind":"analyzer-migration","status":"preview","preview":preview}),
                )
            }
            "analyzer/migrationApply" => {
                let token = input
                    .preview_token
                    .as_deref()
                    .filter(|token| !token.is_empty())
                    .ok_or_else(|| error(-32602))?;
                let preview = migration::preview(&start, input.application.as_deref())
                    .map_err(|failure| failure_value(&failure, events))?;
                let (prepared, backup) = migration::apply(&preview, token, &analyzer)
                    .map_err(|failure| failure_value(&failure, events))?;
                Ok(
                    json!({"schema_version":1,"kind":"analyzer-migration","status":"migrated","backup":super::dto::path(&backup),"configuration":crate::analyzer_config::document(&prepared,true)}),
                )
            }
            _ => Err(error(-32601)),
        }
    }
}

/// RPC errors use the same stable category/data pair as the standalone CLI.
fn failure_value(failure: &eska_analyzer::Error, events: &mut Vec<Value>) -> Value {
    if let eska_analyzer::Error::CheckConfig { stdout, stderr, .. } = failure {
        events.push(json!({"jsonrpc":"2.0","method":"analyzer/validationOutput","params":{"stdout":stdout,"stderr":stderr}}));
    }
    domain(
        "analyzer_configuration_failed",
        crate::analyzer_config::error_document(failure),
    )
}
