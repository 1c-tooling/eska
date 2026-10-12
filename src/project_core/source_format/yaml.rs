//! One strict YAML 1.2 document; CST acceptance never implies source-profile acceptance.

use std::ops::Range;

use serde_json::{Map, Value};
use yaml_edit::{AsYaml, Scalar, SyntaxKind, YamlFile, YamlNode};

use super::Error;

const MAX_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

/// Decode YAML through its CST, rejecting duplicate keys and unsupported syntax first.
pub(super) fn decode(input: &str) -> Result<Value, Error> {
    decode_with_tree(input).map(|(value, _)| value)
}

/// Retain the same validated CST for source coordinates instead of parsing an owner twice.
pub(super) fn decode_with_tree(input: &str) -> Result<(Value, YamlFile), Error> {
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
    let value = convert(&YamlNode::Mapping(mapping), 0)?;
    Ok((value, file))
}

/// Schema coordinates use actual mapping fields and sequence positions, never string searches.
#[derive(Clone, Debug)]
pub(super) enum Step {
    Key(&'static str),
    Index(usize),
}

/// Resolve a node in the exact CST which produced the validated semantic document.
pub(super) fn at(file: &YamlFile, steps: &[Step]) -> Option<YamlNode> {
    let mut node = YamlNode::Mapping(file.document()?.as_mapping()?);
    for step in steps {
        node = match step {
            Step::Key(key) => node.as_mapping()?.get(*key),
            Step::Index(index) => node.as_sequence()?.get(*index),
        }?;
    }
    Some(node)
}

/// CST offsets are UTF-8 bytes in the exact text that passed strict decoding.
pub(super) fn range_at(file: &YamlFile, steps: &[Step]) -> Result<Range<usize>, Error> {
    let node = at(file, steps).ok_or_else(|| Error::root("source_location_invalid"))?;
    let range = node
        .as_node()
        .ok_or_else(|| Error::root("source_location_invalid"))?
        .text_range();
    Ok(usize::from(range.start())..usize::from(range.end()))
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
    encode_order(value, false)
}

/// Ordinary property values precede reconstruction details only in the explicitly new envelope.
pub(super) fn encode_readable(value: &Value) -> Result<String, Error> {
    encode_order(value, true)
}

/// Both wire versions use the same strict scalar rendering and document quota.
fn encode_order(value: &Value, readable: bool) -> Result<String, Error> {
    let mut output = String::new();
    write(value, 0, &mut output, readable)?;
    output.push('\n');
    if output.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::root("source_document_too_large"));
    }
    Ok(output)
}

/// Render nonempty collections in block form and all scalar strings with JSON escaping.
fn write(value: &Value, indent: usize, output: &mut String, readable: bool) -> Result<(), Error> {
    match value {
        Value::Object(mapping) if !mapping.is_empty() => {
            let mut entries: Vec<_> = mapping.iter().collect();
            if readable {
                entries.sort_by_key(|(key, _)| field_order(key));
            }
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    output.push('\n');
                }
                output.push_str(&" ".repeat(indent));
                output.push_str(
                    &serde_json::to_string(key).map_err(|_| Error::root("source_yaml_invalid"))?,
                );
                output.push(':');
                write_entry(value, indent, output, readable)?;
            }
        }
        Value::Array(values) if !values.is_empty() => {
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push('\n');
                }
                output.push_str(&" ".repeat(indent));
                output.push('-');
                write_entry(value, indent, output, readable)?;
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
fn write_entry(
    value: &Value,
    indent: usize,
    output: &mut String,
    readable: bool,
) -> Result<(), Error> {
    let block = matches!(value, Value::Object(values) if !values.is_empty())
        || matches!(value, Value::Array(values) if !values.is_empty());
    if block {
        output.push('\n');
    } else {
        output.push(' ');
    }
    write(value, indent + 2, output, readable)?;
    Ok(())
}

/// Stable priorities keep identity and ordinary values before native XML and ordering slots.
fn field_order(key: &str) -> usize {
    [
        "schema_version",
        "rules_version",
        "profile",
        "document",
        "path",
        "descriptor",
        "object",
        "kind",
        "uuid",
        "Name",
        "Synonym",
        "Comment",
        "Version",
        "properties",
        "children",
        "nested",
        "native",
        "native_xml",
    ]
    .iter()
    .position(|field| *field == key)
    .unwrap_or(12)
}
