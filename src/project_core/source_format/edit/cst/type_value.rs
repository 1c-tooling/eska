//! Type edits replace existing scalar leaves; composition and native bindings stay explicit.

use serde_json::Value;
use yaml_edit::{Mapping, YamlNode};

use super::{set_direct_string, unsupported};
use crate::project_core::source_format::{Error, TypeDescription};

#[cfg(test)]
mod tests;

/// Keep every native slot and authored annotation while changing a value's existing leaves.
pub(super) fn replace(
    mapping: &Mapping,
    old: &TypeDescription,
    new: &TypeDescription,
) -> Result<(), Error> {
    if old.native != new.native {
        return Err(unsupported());
    }
    let old = serde_json::to_value(&old.data).map_err(|_| unsupported())?;
    let new = serde_json::to_value(&new.data).map_err(|_| unsupported())?;
    fields(mapping, &old, &new)
}

/// Named fields must retain identical presence; no qualifier defaults are manufactured.
fn fields(mapping: &Mapping, old: &Value, new: &Value) -> Result<(), Error> {
    let (Some(old), Some(new)) = (old.as_object(), new.as_object()) else {
        return Err(unsupported());
    };
    if old.len() != new.len() {
        return Err(unsupported());
    }
    for (key, previous) in old {
        let next = new.get(key).ok_or_else(unsupported)?;
        if previous == next {
            continue;
        }
        let node = mapping.get(key.as_str()).ok_or_else(unsupported)?;
        if let (Value::String(_), Value::String(value)) = (previous, next) {
            let scalar = node.as_scalar().ok_or_else(unsupported)?;
            set_direct_string(mapping, key, value, Some(scalar));
        } else {
            collection(&node, previous, next)?;
        }
    }
    Ok(())
}

/// Alternatives retain their sequence length and position, so only declared leaves can be edited.
fn collection(node: &YamlNode, old: &Value, new: &Value) -> Result<(), Error> {
    match (old, new) {
        (Value::Object(_), Value::Object(_)) => {
            fields(node.as_mapping().ok_or_else(unsupported)?, old, new)
        }
        (Value::Array(old), Value::Array(new)) if old.len() == new.len() => {
            let sequence = node.as_sequence().ok_or_else(unsupported)?;
            for (index, (old, new)) in old.iter().zip(new).enumerate() {
                if old != new {
                    let item = sequence.get(index).ok_or_else(unsupported)?;
                    fields(item.as_mapping().ok_or_else(unsupported)?, old, new)?;
                }
            }
            Ok(())
        }
        _ => Err(unsupported()),
    }
}
