//! Explicit ESKA negotiation is independent of the existing application's IDE protocol 1.x.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const VERSION: &str = "0.1";

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
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

pub enum Incoming {
    Message(Message),
    Response(ClientResponse),
}

pub struct ClientResponse {
    pub id: Option<Id>,
    pub result: Result<Value, ClientError>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientError {
    pub code: i32,
    #[serde(rename = "message")]
    _message: String,
    #[serde(default, rename = "data")]
    _data: Value,
}

/// Distinguish requests from client replies before cancellation tracking; JSON-RPC batches stay unsupported.
pub fn parse(body: &[u8]) -> Result<Incoming, Value> {
    use eska::tool_cli::rpc_json::{ParseFailure, parse_unique};
    let value = parse_unique(body).map_err(|error| match error {
        ParseFailure::Syntax => error_value(-32700, "rpc_parse_error", Value::Null),
        ParseFailure::Invalid => error_value(-32600, "rpc_invalid_request", Value::Null),
    })?;
    if value.get("method").is_none() {
        return client_response(value).map(Incoming::Response);
    }
    if value.get("id").is_some_and(Value::is_null) {
        return Err(error_value(-32600, "rpc_invalid_request", Value::Null));
    }
    let message: Message = serde_json::from_value(value)
        .map_err(|_| error_value(-32600, "rpc_invalid_request", Value::Null))?;
    if message.jsonrpc != "2.0" || message.method.is_empty() {
        return Err(error_value(-32600, "rpc_invalid_request", Value::Null));
    }
    Ok(Incoming::Message(message))
}

/// Require one result or error while preserving null success and response IDs independently of requests.
fn client_response(value: Value) -> Result<ClientResponse, Value> {
    let invalid = || error_value(-32600, "rpc_invalid_request", Value::Null);
    let Value::Object(mut fields) = value else {
        return Err(invalid());
    };
    if fields.remove("jsonrpc") != Some(json!("2.0")) {
        return Err(invalid());
    }
    let id =
        serde_json::from_value(fields.remove("id").ok_or_else(invalid)?).map_err(|_| invalid())?;
    let result = fields.remove("result");
    let error = fields.remove("error");
    if !fields.is_empty() {
        return Err(invalid());
    }
    let result = match (result, error) {
        (Some(result), None) => Ok(result),
        (None, Some(error)) => Err(serde_json::from_value(error).map_err(|_| invalid())?),
        _ => return Err(invalid()),
    };
    Ok(ClientResponse { id, result })
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
        -32800 => "Request cancelled",
        -32802 => "Server cancelled",
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
            let Incoming::Message(message) = parse(&body).unwrap() else {
                panic!("request parsed as a reply");
            };
            assert_eq!(serde_json::to_value(message.id).unwrap(), id);
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

    /// Replies preserve null results and response-only null IDs, while ambiguous/error envelopes fail.
    #[test]
    fn client_replies_have_exactly_one_result_or_structured_error() {
        for id in [json!(null), json!(-1), json!("ответ")] {
            let Incoming::Response(response) = parse(
                &serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"result":null})).unwrap(),
            )
            .unwrap() else {
                panic!("reply parsed as a request");
            };
            assert_eq!(serde_json::to_value(response.id).unwrap(), id);
            assert_eq!(response.result.ok().unwrap(), Value::Null);
        }
        let Incoming::Response(response) = parse(
            br#"{"jsonrpc":"2.0","id":"registration","error":{"code":-32601,"message":"missing","data":null}}"#,
        ).unwrap() else {
            panic!("error reply parsed as a request");
        };
        assert_eq!(response.result.err().unwrap().code, -32601);
        for body in [
            br#"{"jsonrpc":"2.0","id":1,"result":null,"error":null}"#.as_slice(),
            br#"{"jsonrpc":"2.0","id":1}"#,
            br#"{"jsonrpc":"2.0","result":null}"#,
            br#"{"jsonrpc":"2.0","id":true,"result":null}"#,
            br#"{"jsonrpc":"2.0","id":1,"error":{"code":false,"message":"bad"}}"#,
            br#"{"jsonrpc":"2.0","id":1,"error":{"code":1}}"#,
            br#"{"jsonrpc":"2.0","id":1,"result":null,"method":"initialize"}"#,
            br#"{"jsonrpc":"2.0","id":1,"result":null,"extra":1}"#,
            br#"{"jsonrpc":"2.0","id":1,"result":null,"result":false}"#,
        ] {
            assert_eq!(parse(body).err().unwrap()["code"], -32600);
        }
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
