//! The metadata server validates and invokes the optional component; no formatter implementation is linked.

use super::{envelope::domain, params};
use eska_project_core::formatting::{
    Error,
    protocol::{Request, Response},
};
use eska_tool_runtime::{Component, QueryPolicy, capture_input, contract, resolve, resolve_at};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::Path, process::Command, sync::atomic::AtomicBool, time::Duration};

#[cfg(all(test, unix))]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Failure {
    schema_version: u32,
    error: Error,
}

/// Additive capability does not imply an installed formatter or an analyzer requirement.
pub(super) fn capability() -> Value {
    json!({"schemaVersion":1,"component":"eska-fmt","protocolVersion":1,"planOnly":true,"writesOwner":false,"languages":["bsl","sdbl"],"document":true,"range":true,"queryDraft":true,"formatOnSave":"opt_in","maxTextBytes":4*1024*1024,"maxInlineBytes":128*1024})
}

/// Explicit machine entries are independent of metadata membership and analyzer availability.
pub(super) fn is_method(method: &str) -> bool {
    matches!(
        method,
        "format/document"
            | "format/query"
            | "format/bufferBegin"
            | "format/bufferAppend"
            | "format/bufferClose"
    )
}

/// Probe exact component identity before submitting text, then validate the returned source coordinates.
pub(super) fn invoke(
    request: &Request,
    binary: Option<&params::Path>,
    root: &Path,
    cancelled: &AtomicBool,
    closed: &AtomicBool,
) -> Result<(Response, String), Value> {
    request.validate().map_err(failure)?;
    let binary = binary
        .map(params::Path::native)
        .transpose()?
        .map_or_else(
            || resolve(Component::Fmt),
            |path| resolve_at(Component::Fmt, &path),
        )
        .map_err(|error| transport(&error))?;
    let probe = execute(
        &binary,
        root,
        "__tool-contract",
        Vec::new(),
        16 * 1024,
        cancelled,
        closed,
    )?;
    let value: Value = serde_json::from_slice(&probe)
        .map_err(|_| failure(Error::new("component_response_invalid", "eska-fmt")))?;
    if !contract::compatible(&value, "eska-fmt") {
        return Err(failure(Error::new("component_incompatible", "eska-fmt")));
    }
    let version = value["version"]
        .as_str()
        .ok_or_else(|| failure(Error::new("component_response_invalid", "version")))?
        .to_owned();
    let input = serde_json::to_vec(request)
        .map_err(|_| failure(Error::new("invalid_request", "request")))?;
    let output = execute(
        &binary,
        root,
        "__format-text",
        input,
        128 * 1024 * 1024,
        cancelled,
        closed,
    )?;
    let response: Response = serde_json::from_slice(&output)
        .map_err(|_| failure(Error::new("component_response_invalid", "eska-fmt")))?;
    response.validate(request).map_err(failure)?;
    Ok((response, version))
}

/// Pure formatting owns no native descendants or writes, so cancellation immediately reaps its exact child.
fn execute(
    binary: &Path,
    root: &Path,
    entry: &str,
    input: Vec<u8>,
    output_limit: u64,
    cancelled: &AtomicBool,
    closed: &AtomicBool,
) -> Result<Vec<u8>, Value> {
    let mut command = Command::new(binary);
    command.arg(entry).current_dir(root);
    let output = capture_input(
        Component::Fmt,
        &mut command,
        input,
        QueryPolicy {
            timeout: Duration::from_secs(30),
            output_limit,
            cancellation_grace: Duration::ZERO,
        },
        cancelled,
        closed,
    )
    .map_err(|error| transport(&error))?;
    if !output.status.success() {
        if let Ok(error) = serde_json::from_slice::<Failure>(&output.stdout)
            && error.schema_version == 1
        {
            return Err(failure(error.error));
        }
        return Err(failure(Error::new("component_failed", "eska-fmt")));
    }
    if !output.stderr.is_empty() {
        return Err(failure(Error::new("component_response_invalid", "stderr")));
    }
    Ok(output.stdout)
}

/// Missing components and cancellation retain machine categories, never analyzer fallbacks or localized subprocess output.
fn transport(error: &eska_tool_runtime::Error) -> Value {
    failure(Error::new(error.code(), "eska-fmt"))
}

/// The adapter localizes this stable envelope and keeps source contents out of error diagnostics.
pub(super) fn failure(error: Error) -> Value {
    domain(
        "formatting_failed",
        serde_json::to_value(error)
            .unwrap_or_else(|_| json!({"code":"invalid_response","field":"error"})),
    )
}
