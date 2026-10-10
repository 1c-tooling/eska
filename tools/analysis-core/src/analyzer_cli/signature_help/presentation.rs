//! Parameter offsets are UTF-16 spans within a display label, independent of source-file positions.

use serde_json::{Value, json};

use crate::{CallSignature, CallSignatures};

use super::{Capabilities, protocol};

/// Preserve candidate order and emit only active/null fields supported by the negotiated client.
pub(super) fn render(help: CallSignatures, capabilities: Capabilities) -> Result<Value, Value> {
    let selected = help.active_signature.unwrap_or(0);
    let signatures = help
        .signatures
        .into_iter()
        .enumerate()
        .map(|(index, signature)| {
            let mut signature = render_signature(&signature, capabilities)?;
            if index == selected && capabilities.parameters.active_per_signature {
                active_parameter(
                    &mut signature,
                    help.active_parameter,
                    capabilities.parameters.no_active,
                )?;
            }
            Ok(signature)
        })
        .collect::<Result<Vec<_>, Value>>()?;
    let mut result = json!({"signatures":signatures});
    if let Some(active) = help.active_signature {
        result["activeSignature"] = unsigned(active)?;
    }
    active_parameter(
        &mut result,
        help.active_parameter,
        capabilities.parameters.no_active,
    )?;
    Ok(result)
}

/// Borrow current generated label data once; every span advances the UTF-16 cursor monotonically.
fn render_signature(signature: &CallSignature, capabilities: Capabilities) -> Result<Value, Value> {
    let mut rendered = json!({"label":signature.label});
    if let Some(doc) = &signature.documentation {
        rendered["documentation"] = capabilities.documentation.document(doc);
    }
    let mut cursor = 0;
    let mut column = 0;
    let mut parameters = Vec::with_capacity(signature.parameters.len());
    for parameter in &signature.parameters {
        let start = parameter.byte_range.start as usize;
        let end = parameter.byte_range.end as usize;
        let prefix = signature
            .label
            .get(cursor..start)
            .ok_or_else(inconsistent)?;
        let label = signature.label.get(start..end).ok_or_else(inconsistent)?;
        column += prefix.encode_utf16().count();
        let end_column = column + label.encode_utf16().count();
        let label = if capabilities.parameters.label_offsets {
            json!([unsigned(column)?, unsigned(end_column)?])
        } else {
            json!(parameter.label)
        };
        let mut rendered = json!({"label":label});
        if let Some(doc) = &parameter.documentation {
            rendered["documentation"] = capabilities.documentation.document(doc);
        }
        parameters.push(rendered);
        cursor = end;
        column = end_column;
    }
    rendered["parameters"] = json!(parameters);
    Ok(rendered)
}

/// Omitted active parameters follow legacy LSP defaults; explicit null is negotiated separately.
fn active_parameter(
    value: &mut Value,
    active: Option<usize>,
    allow_null: bool,
) -> Result<(), Value> {
    if let Some(active) = active {
        value["activeParameter"] = unsigned(active)?;
    } else if allow_null {
        value["activeParameter"] = Value::Null;
    }
    Ok(())
}

/// LSP unsigned values fit the protocol's signed 31-bit positive range.
fn unsigned(value: usize) -> Result<Value, Value> {
    i32::try_from(value)
        .ok()
        .map(|value| json!(value))
        .ok_or_else(inconsistent)
}

/// Inconsistent generated display data never receives plausible clamped indices or spans.
fn inconsistent() -> Value {
    protocol::error_value(-32603, "signature_help_inconsistent", Value::Null)
}
