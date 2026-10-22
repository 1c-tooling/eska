//! JSON-RPC validation independent of domain methods and cache serialization.

use serde::{
    Deserialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Value, json};
use std::fmt;

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

/// Enforce depth before serde recursion and reject duplicate keys at every nesting level.
pub(super) fn parse(body: &[u8]) -> Result<Value, Value> {
    let mut depth = 0_usize;
    let mut string = false;
    let mut escape = false;
    for &byte in body {
        if string {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                string = false;
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > 64 {
                        return Err(error(-32600));
                    }
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => (),
            }
        }
    }
    serde_json::from_slice::<Unique>(body)
        .map(|value| value.0)
        .map_err(|failure| {
            error(if failure.to_string().contains("duplicate JSON key") {
                -32600
            } else {
                -32700
            })
        })
}

struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    /// A custom visitor preserves ordinary JSON values while refusing duplicate map entries.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueVisitor)
    }
}
struct UniqueVisitor;
impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = Unique;
    /// Error descriptions never echo document contents.
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }
    /// Preserve JSON null.
    fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
        Ok(Unique(Value::Null))
    }
    /// Preserve JSON booleans.
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Unique, E> {
        Ok(Unique(json!(value)))
    }
    /// Preserve negative numbers for later ID validation.
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Unique, E> {
        Ok(Unique(json!(value)))
    }
    /// Preserve integer precision.
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Unique, E> {
        Ok(Unique(json!(value)))
    }
    /// Preserve fractional numbers for later type validation.
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Unique, E> {
        Ok(Unique(json!(value)))
    }
    /// Own strings independently of the bounded input buffer.
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Unique, E> {
        Ok(Unique(json!(value)))
    }
    /// Recursively validate arrays without interpreting batch semantics yet.
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Unique, A::Error> {
        let mut values = Vec::new();
        while let Some(Unique(value)) = seq.next_element()? {
            values.push(value);
        }
        Ok(Unique(Value::Array(values)))
    }
    /// Duplicate keys are invalid even inside ignored extension fields.
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Unique, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some((key, Unique(value))) = map.next_entry::<String, Unique>()? {
            if values.insert(key, value).is_some() {
                return Err(de::Error::custom("duplicate JSON key"));
            }
        }
        Ok(Unique(Value::Object(values)))
    }
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
