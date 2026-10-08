//! One strict YAML 1.2 document; CST acceptance never implies source-profile acceptance.

use serde_json::{Map, Value};
use yaml_edit::{Scalar, SyntaxKind, YamlFile, YamlNode};

use super::Error;

const MAX_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

/// Decode YAML through its CST, rejecting duplicate keys and unsupported syntax first.
pub(super) fn decode(input: &str) -> Result<Value, Error> {
    if input.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::root("source_document_too_large"));
    }
    if input.starts_with('\u{feff}') {
        return Err(Error::root("source_yaml_bom_forbidden"));
    }
    for (kind, _) in yaml_edit::lex(input) {
        if matches!(
            kind,
            SyntaxKind::ANCHOR | SyntaxKind::REFERENCE | SyntaxKind::TAG | SyntaxKind::DIRECTIVE
        ) {
            return Err(Error::root("source_yaml_construct_forbidden"));
        }
    }
    let parsed = YamlFile::parse(input);
    if !parsed.ok() {
        return Err(Error::root("source_yaml_invalid"));
    }
    let file = parsed.tree();
    let documents: Vec<_> = file.documents().collect();
    if documents.len() != 1 {
        return Err(Error::root("source_yaml_document_count"));
    }
    let mapping = documents[0]
        .as_mapping()
        .ok_or_else(|| Error::root("source_yaml_root_invalid"))?;
    convert(&YamlNode::Mapping(mapping), 0)
}

/// Traverse strict values without letting a mapping silently overwrite an earlier key.
fn convert(node: &YamlNode, depth: usize) -> Result<Value, Error> {
    if depth > MAX_DEPTH {
        return Err(Error::root("source_depth_exceeded"));
    }
    if let Some(mapping) = node.as_mapping() {
        let mut result = Map::new();
        for (key, value) in mapping {
            let key = key
                .as_scalar()
                .ok_or_else(|| Error::root("source_yaml_key_invalid"))?;
            let Value::String(key) = scalar(key)? else {
                return Err(Error::root("source_yaml_key_invalid"));
            };
            if key == "<<" {
                return Err(Error::root("source_yaml_construct_forbidden"));
            }
            let value = convert(&value, depth + 1).map_err(|error| error.in_field(key.clone()))?;
            if result.insert(key.clone(), value).is_some() {
                return Err(Error::root("source_yaml_key_duplicate").in_field(key));
            }
        }
        Ok(Value::Object(result))
    } else if let Some(sequence) = node.as_sequence() {
        sequence
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                convert(&value, depth + 1).map_err(|error| error.in_field(index.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array)
    } else if let Some(value) = node.as_scalar() {
        scalar(value)
    } else {
        Err(Error::root("source_yaml_construct_forbidden"))
    }
}

/// Resolve only JSON-compatible scalar types; exact native numbers remain quoted strings.
fn scalar(value: &Scalar) -> Result<Value, Error> {
    let raw = value.value();
    if raw.starts_with('>') {
        return Err(Error::root("source_yaml_folding_forbidden"));
    }
    if raw.starts_with('"') || raw.starts_with('\'') || raw.starts_with('|') {
        return Ok(Value::String(value.as_string()));
    }
    match raw.as_str() {
        "null" | "Null" | "NULL" | "~" | "" => Ok(Value::Null),
        "true" | "True" | "TRUE" => Ok(Value::Bool(true)),
        "false" | "False" | "FALSE" => Ok(Value::Bool(false)),
        _ => {
            if raw.bytes().all(|byte| byte.is_ascii_digit()) {
                raw.parse::<u64>()
                    .map(Value::from)
                    .map_err(|_| Error::root("source_yaml_integer_overflow"))
            } else if raw.starts_with('-') && raw[1..].bytes().all(|byte| byte.is_ascii_digit()) {
                raw.parse::<i64>()
                    .map(Value::from)
                    .map_err(|_| Error::root("source_yaml_integer_overflow"))
            } else {
                Ok(Value::String(value.as_string()))
            }
        }
    }
}

/// Emit the accepted subset deterministically, without floating-point or timestamp coercion.
pub(super) fn encode(value: &Value) -> Result<String, Error> {
    let mut output = String::new();
    write(value, 0, &mut output)?;
    output.push('\n');
    Ok(output)
}

/// Render nonempty collections in block form and all scalar strings with JSON escaping.
fn write(value: &Value, indent: usize, output: &mut String) -> Result<(), Error> {
    match value {
        Value::Object(mapping) if !mapping.is_empty() => {
            for (index, (key, value)) in mapping.iter().enumerate() {
                if index > 0 {
                    output.push('\n');
                }
                output.push_str(&" ".repeat(indent));
                output.push_str(
                    &serde_json::to_string(key).map_err(|_| Error::root("source_yaml_invalid"))?,
                );
                output.push(':');
                write_entry(value, indent, output)?;
            }
        }
        Value::Array(values) if !values.is_empty() => {
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push('\n');
                }
                output.push_str(&" ".repeat(indent));
                output.push('-');
                write_entry(value, indent, output)?;
            }
        }
        _ => {
            output.push_str(
                &serde_json::to_string(value).map_err(|_| Error::root("source_yaml_invalid"))?,
            );
        }
    }
    Ok(())
}

/// Keep scalar entries inline; nested blocks receive an explicit increased indentation.
fn write_entry(value: &Value, indent: usize, output: &mut String) -> Result<(), Error> {
    let block = matches!(value, Value::Object(values) if !values.is_empty())
        || matches!(value, Value::Array(values) if !values.is_empty());
    if block {
        output.push('\n');
    } else {
        output.push(' ');
    }
    write(value, indent + 2, output)?;
    Ok(())
}
