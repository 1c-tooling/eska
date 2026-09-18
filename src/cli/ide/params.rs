//! Strict input types ignore extension fields but reject unknown enum values and wrong types.

use super::envelope::error;
use crate::project::metadata_model::{CollectionKind, MetadataKind, ModuleRole, NodeId, ObjectId};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

/// Decode typed method parameters, keeping parser failures out of wire diagnostics.
pub(super) fn decode<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, Value> {
    serde_json::from_value(value.clone()).map_err(|_| error(-32602))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Context {
    pub session_id: String,
    pub project_id: String,
    pub generation: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Session {
    pub session_id: String,
}
#[derive(Deserialize)]
pub(super) struct Path {
    value: String,
    encoding: Encoding,
}
#[derive(Deserialize)]
enum Encoding {
    #[serde(rename = "utf-8")]
    Utf8,
    #[serde(rename = "percent")]
    Percent,
    #[serde(rename = "utf-16-percent")]
    Wide,
}
impl Path {
    /// Decode all native bytes/code units, rejecting encodings belonging to another OS.
    pub(super) fn native(&self) -> Result<PathBuf, Value> {
        match self.encoding {
            Encoding::Utf8 => Ok(PathBuf::from(&self.value)),
            Encoding::Percent => decode_percent(&self.value),
            Encoding::Wide => decode_wide(&self.value),
        }
    }
}

/// Require a complete percent encoding, including ASCII and literal percent characters.
fn units(value: &str, width: usize) -> Result<Vec<u16>, Value> {
    if !value.is_ascii() || !value.len().is_multiple_of(width + 1) {
        return Err(error(-32602));
    }
    value
        .as_bytes()
        .chunks(width + 1)
        .map(|chunk| {
            if chunk[0] != b'%' {
                return Err(error(-32602));
            }
            let digits = std::str::from_utf8(&chunk[1..]).map_err(|_| error(-32602))?;
            u16::from_str_radix(digits, 16).map_err(|_| error(-32602))
        })
        .collect()
}

#[cfg(unix)]
/// Unix paths preserve arbitrary non-UTF-8 bytes.
fn decode_percent(value: &str) -> Result<PathBuf, Value> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let bytes = units(value, 2)?
        .into_iter()
        .map(|unit| u8::try_from(unit).map_err(|_| error(-32602)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OsString::from_vec(bytes).into())
}
#[cfg(not(unix))]
/// Unix encodings cannot be reinterpreted as native Windows paths.
fn decode_percent(_: &str) -> Result<PathBuf, Value> {
    Err(error(-32602))
}
#[cfg(windows)]
/// Windows paths preserve unpaired UTF-16 surrogates.
fn decode_wide(value: &str) -> Result<PathBuf, Value> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    Ok(OsString::from_wide(&units(value, 4)?).into())
}
#[cfg(not(windows))]
/// Windows encodings cannot be reinterpreted as native Unix paths.
fn decode_wide(_: &str) -> Result<PathBuf, Value> {
    Err(error(-32602))
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Node {
    #[serde(rename_all = "camelCase")]
    Object {
        object_id: ObjectId,
    },
    Module {
        owner: ObjectId,
        role: String,
    },
    Collection {
        owner: ObjectId,
        collection: Collection,
    },
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Collection {
    #[serde(rename_all = "camelCase")]
    Metadata {
        metadata_kind: String,
    },
    Common,
    Modules,
    Unsupported,
}

/// Convert only recognized wire roles and kinds into typed core identities.
pub(super) fn node(value: &Value) -> Result<NodeId, Value> {
    Ok(match decode::<Node>(value)? {
        Node::Object { object_id } => NodeId::Object(object_id),
        Node::Module { owner, role } => {
            let roles = [
                ModuleRole::Module,
                ModuleRole::Object,
                ModuleRole::Manager,
                ModuleRole::RecordSet,
                ModuleRole::ValueManager,
                ModuleRole::ManagedApplication,
                ModuleRole::OrdinaryApplication,
                ModuleRole::Session,
                ModuleRole::ExternalConnection,
                ModuleRole::Command,
            ];
            NodeId::Module {
                owner,
                role: roles
                    .into_iter()
                    .find(|item| item.as_str() == role)
                    .ok_or_else(|| error(-32602))?,
            }
        }
        Node::Collection { owner, collection } => NodeId::Collection {
            owner,
            kind: match collection {
                Collection::Common => CollectionKind::Common,
                Collection::Modules => CollectionKind::Modules,
                Collection::Unsupported => CollectionKind::Unsupported,
                Collection::Metadata { metadata_kind } => CollectionKind::Metadata(
                    MetadataKind::ALL
                        .iter()
                        .copied()
                        .find(|kind| kind.as_str() == metadata_kind)
                        .ok_or_else(|| error(-32602))?,
                ),
            },
        },
    })
}

/// Generations and sequences are decimal u64 strings, never floating-point JSON numbers.
pub(super) fn counter(value: &str) -> Result<u64, Value> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(error(-32602));
    }
    value.parse().map_err(|_| error(-32602))
}

/// Optional fields may be omitted, but explicit null is not a value of their declared type.
pub(super) fn optional_fields(value: &Value, names: &[&str]) -> Result<(), Value> {
    if names
        .iter()
        .any(|name| value.get(*name).is_some_and(Value::is_null))
    {
        Err(error(-32602))
    } else {
        Ok(())
    }
}
