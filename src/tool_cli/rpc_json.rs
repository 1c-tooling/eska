//! Shared bounded JSON validation has no knowledge of either transport's handshake.

#[cfg(test)]
mod tests;

use serde::{
    Deserialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Value, json};
use std::fmt;

/// Envelope validation distinguishes malformed JSON from ambiguous or excessively nested values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseFailure {
    Syntax,
    Invalid,
}

/// Enforce depth before serde recursion and reject duplicate keys at every nesting level.
///
/// # Errors
/// Returns invalid depth/duplicate keys or malformed JSON without exposing input bytes.
pub fn parse_unique(body: &[u8]) -> Result<Value, ParseFailure> {
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
                        return Err(ParseFailure::Invalid);
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
            if failure.to_string().contains("duplicate JSON key") {
                ParseFailure::Invalid
            } else {
                ParseFailure::Syntax
            }
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
