//! Explicit migration preserves unrelated manifest entries and refuses conflicting policies.

use super::{Config, Error, Layer, Patch, policy::legacy};
use std::path::Path;

mod write;
pub use write::apply;

/// Preview retains the exact original bytes for a later compare-before-write operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    pub before: String,
    pub after: String,
}

/// Translate the two documented legacy settings without rewriting unrelated TOML or comments.
///
/// # Errors
/// Rejects unknown policy fields, incompatible explicit options and ambiguous legacy tab counts.
pub fn preview(input: &str) -> Result<Option<Plan>, Error> {
    let bom = input.starts_with('\u{feff}');
    let body = input.trim_start_matches('\u{feff}');
    let table: toml::Table =
        toml::from_str(body).map_err(|_| Error::new("invalid_config", "eska.toml"))?;
    let patch = legacy(&table)?;
    if patch.is_empty() {
        return Ok(None);
    }
    let config: Config = table
        .get("format")
        .cloned()
        .map(toml::Value::try_into)
        .transpose()
        .map_err(|_| Error::new("invalid_config", "format"))?
        .unwrap_or_default();
    Layer::new(Path::new("."), "manifest", &config, body)?;
    let mut document: toml_edit::DocumentMut = body
        .parse()
        .map_err(|_| Error::new("invalid_config", "eska.toml"))?;
    document["tools"]["bsl-analyzer"]
        .as_table_like_mut()
        .ok_or_else(|| Error::new("invalid_legacy", "formatting"))?
        .remove("formatting");
    insert(&mut document, &patch)?;
    let rendered = document.to_string();
    let rendered = if body.contains("\r\n") {
        rendered.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        rendered
    };
    Ok(Some(Plan {
        before: input.into(),
        after: format!("{}{rendered}", if bom { "\u{feff}" } else { "" }),
    }))
}

/// Insert only missing equivalent values; profiles and unrelated table decorations stay intact.
fn insert(document: &mut toml_edit::DocumentMut, patch: &Patch) -> Result<(), Error> {
    if document.get("format").is_none() {
        let mut table = toml_edit::Table::new();
        table.set_implicit(true);
        table.set_position(Some(isize::MAX));
        document["format"] = toml_edit::Item::Table(table);
    }
    if document["format"].get("bsl").is_none() {
        let mut table = toml_edit::Table::new();
        table.set_position(Some(isize::MAX));
        document["format"]["bsl"] = toml_edit::Item::Table(table);
    }
    let value =
        serde_json::to_value(patch).map_err(|_| Error::new("invalid_config", "format.bsl"))?;
    for (key, value) in value
        .as_object()
        .ok_or_else(|| Error::new("invalid_config", "format.bsl"))?
    {
        if document
            .get("format")
            .and_then(|item| item.get("bsl"))
            .and_then(|item| item.get(key))
            .is_some()
        {
            continue;
        }
        let value = match value {
            serde_json::Value::String(value) => toml_edit::value(value),
            serde_json::Value::Bool(value) => toml_edit::value(*value),
            serde_json::Value::Number(value) => toml_edit::value(
                value
                    .as_i64()
                    .ok_or_else(|| Error::new("invalid_option", key))?,
            ),
            _ => return Err(Error::new("invalid_option", key)),
        };
        document["format"]["bsl"][key] = value;
    }
    Ok(())
}
