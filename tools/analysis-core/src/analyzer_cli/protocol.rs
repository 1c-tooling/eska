//! Explicit ESKA negotiation is independent of the existing application's IDE protocol 1.x.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const VERSION: &str = "0.1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Id {
    Number(i32),
    String(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    jsonrpc: String,
    pub id: Option<Id>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Distinguish syntax from envelope failures; batch messages and server responses are unsupported.
pub fn parse(body: &[u8]) -> Result<Message, Value> {
    use eska::tool_cli::rpc_json::{ParseFailure, parse_unique};
    let value = parse_unique(body).map_err(|error| match error {
        ParseFailure::Syntax => error_value(-32700, "rpc_parse_error", Value::Null),
        ParseFailure::Invalid => error_value(-32600, "rpc_invalid_request", Value::Null),
    })?;
    if value.get("id").is_some_and(Value::is_null) {
        return Err(error_value(-32600, "rpc_invalid_request", Value::Null));
    }
    let message: Message = serde_json::from_value(value)
        .map_err(|_| error_value(-32600, "rpc_invalid_request", Value::Null))?;
    if message.jsonrpc != "2.0" || message.method.is_empty() {
        return Err(error_value(-32600, "rpc_invalid_request", Value::Null));
    }
    Ok(message)
}

/// Machine errors carry stable codes; standard messages do not depend on process locale.
pub fn error_value(code: i32, kind: &str, details: Value) -> Value {
    let message = match code {
        -32700 => "Parse error",
        -32600 => "Invalid Request",
        -32601 => "Method not found",
        -32602 => "Invalid params",
        -32603 => "Internal error",
        -32002 => "Server not initialized",
        -32801 => "Content modified",
        _ => "Request failed",
    };
    let mut error = json!({"code":code, "message":message, "data":{"kind":kind}});
    error["data"]["details"] = details;
    error
}

/// The response echoes the original string/integer ID; invalid envelopes have a null ID.
pub fn response(id: Option<&Id>, result: Result<Value, Value>) -> Value {
    match result {
        Ok(value) => json!({"jsonrpc":"2.0", "id":id, "result":value}),
        Err(error) => json!({"jsonrpc":"2.0", "id":id, "error":error}),
    }
}

/// Deserialize exact custom parameters before a request can mutate lazy input state.
pub fn params<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, Value> {
    serde_json::from_value(value.clone())
        .map_err(|_| error_value(-32602, "rpc_invalid_params", Value::Null))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub session_id: String,
    pub revision: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owners {
    pub snapshot: Snapshot,
    #[serde(default)]
    pub offset: usize,
    pub limit: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadOwner {
    pub snapshot: Snapshot,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LSP IDs preserve signed integer and Unicode string identity; null is never a notification ID.
    #[test]
    fn identifiers_and_ambiguous_envelopes_are_checked_before_dispatch() {
        for id in [json!(-1), json!("запрос"), json!("")] {
            let body =
                serde_json::to_vec(&json!({"jsonrpc":"2.0", "id":id, "method":"initialize"}))
                    .unwrap();
            assert_eq!(serde_json::to_value(parse(&body).unwrap().id).unwrap(), id);
        }
        for body in [
            br#"{"jsonrpc":"2.0","id":null,"method":"initialize"}"#.as_slice(),
            br#"{"jsonrpc":"2.0","id":true,"method":"initialize"}"#,
            br#"{"jsonrpc":"2.0","id":2147483648,"method":"initialize"}"#,
            br#"{"jsonrpc":"2.0","method":"initialize","params":{"x":1,"x":2}}"#,
            br#"[{"jsonrpc":"2.0","id":1,"method":"initialize"}]"#,
        ] {
            assert_eq!(parse(body).err().unwrap()["code"], -32600);
        }
        assert_eq!(parse(b"{").err().unwrap()["code"], -32700);
    }

    /// Decimal revision tokens retain all bits even when a client's JSON numbers are binary64.
    #[test]
    fn revision_tokens_do_not_truncate_to_javascript_integer_precision() {
        assert_eq!(
            crate::SessionRevision(u64::MAX).to_string(),
            "18446744073709551615"
        );
    }
}
