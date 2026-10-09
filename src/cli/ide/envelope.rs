//! JSON-RPC validation independent of domain methods and cache serialization.

use serde_json::{Value, json};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum Id {
    Number(u64),
    Text(String),
}

impl Id {
    /// Numeric and textual identifiers remain different correlation keys.
    pub(super) fn parse(value: &Value) -> Option<Self> {
        match value {
            Value::String(text) if !text.is_empty() && text.is_ascii() && text.len() <= 64 => {
                Some(Self::Text(text.clone()))
            }
            Value::Number(number) => number
                .as_u64()
                .filter(|n| *n <= 9_007_199_254_740_991)
                .map(Self::Number),
            _ => None,
        }
    }

    /// Convert a validated correlation key back to its original JSON type.
    pub(super) fn value(&self) -> Value {
        match self {
            Self::Number(n) => json!(n),
            Self::Text(s) => json!(s),
        }
    }
}

/// Stable errors contain no localized messages or Rust diagnostic formatting.
pub(super) fn error(code: i32) -> Value {
    let message = match code {
        -32700 => "Parse error",
        -32600 => "Invalid Request",
        -32601 => "Method not found",
        -32602 => "Invalid params",
        _ => "Internal error",
    };
    json!({"code":code,"message":message})
}

/// Domain failures remain distinct from malformed JSON-RPC envelopes.
pub(super) fn domain(kind: &str, details: Value) -> Value {
    let mut result = json!({"code":-32000,"message":"Request failed","data":{"kind":kind}});
    result["data"]["details"] = details;
    result
}

/// A response always echoes a valid ID, or null for an uncorrelatable request.
pub(super) fn response(id: Option<&Id>, result: Result<Value, Value>) -> Value {
    let id = id.map_or(Value::Null, Id::value);
    match result {
        Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
        Err(error) => json!({"jsonrpc":"2.0","id":id,"error":error}),
    }
}

pub(super) struct Request<'a> {
    pub id: Option<Id>,
    pub method: &'a str,
    pub params: Value,
}

/// Validate the envelope before considering any method or side effect.
pub(super) fn request(value: &Value) -> Result<Request<'_>, Value> {
    let object = value.as_object().ok_or_else(|| error(-32600))?;
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| error(-32600))?;
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(error(-32600));
    }
    let id = object
        .get("id")
        .map(|value| Id::parse(value).ok_or_else(|| error(-32600)))
        .transpose()?;
    Ok(Request {
        id,
        method,
        params: object.get("params").cloned().unwrap_or_else(|| json!({})),
    })
}

/// Preserve the existing protocol's error codes while sharing JSON validation with other tools.
pub(super) fn parse(body: &[u8]) -> Result<Value, Value> {
    crate::tool_cli::rpc_json::parse_unique(body).map_err(|failure| {
        error(match failure {
            crate::tool_cli::rpc_json::ParseFailure::Invalid => -32600,
            crate::tool_cli::rpc_json::ParseFailure::Syntax => -32700,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Duplicate nested keys and excess depth cannot silently alter request meaning.
    #[test]
    fn rejects_duplicate_keys_and_depth() {
        assert_eq!(
            parse(br#"{"x":{"a":1,"a":2}}"#).unwrap_err()["code"],
            -32600
        );
        assert_eq!(
            parse(format!("{}0{}", "[".repeat(65), "]".repeat(65)).as_bytes()).unwrap_err()["code"],
            -32600
        );
        assert_eq!(parse(b"{").unwrap_err()["code"], -32700);
        assert!(parse(br#"{"ignored":[1,true,null,"{}"]}"#).is_ok());
    }
    /// Correlation keys have exact JSON types and the JavaScript safe integer bound.
    #[test]
    fn validates_identifiers() {
        for value in [
            json!(null),
            json!(""),
            json!("я"),
            json!(1.5),
            json!(-1),
            json!(9_007_199_254_740_992_u64),
        ] {
            assert!(Id::parse(&value).is_none());
        }
        assert_ne!(Id::parse(&json!(1)), Id::parse(&json!("1")));
    }
}
