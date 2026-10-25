//! The existing private v1 checkpoint format remains readable after moving storage out of the IDE.

use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;

use super::{Error, MAX_FILE_BYTES, MAX_METADATA_BYTES, Prepared};
use crate::project::{metadata_edit::snapshot, query::MAX_SNAPSHOT_BYTES};

const METADATA_FIELDS: [&str; 14] = [
    "schemaVersion",
    "id",
    "projectKey",
    "node",
    "source",
    "descriptor",
    "descriptorSnapshot",
    "diskSnapshot",
    "ownerSnapshot",
    "ownerVersion",
    "contextFingerprint",
    "application",
    "identity",
    "writable",
];

/// Checkpoint components can be transferred independently without repeating the full owner on each edit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Part {
    Metadata,
    Owner,
    Base,
    Draft,
}

impl Part {
    /// All adapters share these limits, including adapters without JavaScript strings.
    #[must_use]
    pub const fn max_bytes(self) -> usize {
        match self {
            Self::Metadata => MAX_METADATA_BYTES,
            Self::Owner | Self::Base | Self::Draft => MAX_SNAPSHOT_BYTES,
        }
    }
}

/// A checkpoint is inert recovery data; its paths and permissions must be rebound before applying edits.
#[derive(Clone, Debug)]
pub struct Record {
    id: String,
    metadata: Value,
    texts: [Arc<str>; 3],
    prepared: Option<Prepared>,
}

impl Record {
    /// Validate a complete owner and independent draft before admitting any persistent state.
    ///
    /// # Errors
    /// Rejects invalid bindings, a mismatching full-owner hash and oversized payloads.
    pub fn new(metadata: Value, texts: [Arc<str>; 3]) -> Result<Self, Error> {
        if texts.iter().any(|text| text.len() > MAX_SNAPSHOT_BYTES) {
            return Err(Error::ResourceLimit);
        }
        validate_metadata(&metadata, &texts[0])?;
        let id = metadata["id"]
            .as_str()
            .ok_or(Error::InvalidRecord)?
            .to_owned();
        Ok(Self {
            id,
            metadata,
            texts,
            prepared: None,
        })
    }

    /// Attach or clear a checked intent while retaining the complete original checkpoint.
    ///
    /// # Errors
    /// Rejects inconsistent no-op output, a BSL disk-save request or invalid intent bounds.
    pub fn with_prepared(mut self, prepared: Option<Prepared>) -> Result<Self, Error> {
        self.prepared = prepared
            .map(|mut intent| {
                intent.validate()?;
                if intent.no_op != (intent.output == self.texts[0])
                    || (intent.save_owner && self.metadata["identity"]["kind"] == "bslLiteral")
                {
                    return Err(Error::InvalidRecord);
                }
                if intent.no_op {
                    intent.output = Arc::clone(&self.texts[0]);
                }
                if let Some(shared) = self
                    .texts
                    .iter()
                    .find(|text| **text == intent.applied_draft)
                {
                    intent.applied_draft = Arc::clone(shared);
                }
                Ok(intent)
            })
            .transpose()?;
        Ok(self)
    }

    /// An unfinished intent survives generic draft uploads and requires explicit reconciliation or abort.
    #[must_use]
    pub const fn prepared(&self) -> Option<&Prepared> {
        self.prepared.as_ref()
    }

    /// The filename is derived only from the validated checkpoint ID, never from a source path.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Reading a binding never turns its remembered writable flag into current write authority.
    #[must_use]
    pub const fn metadata(&self) -> &Value {
        &self.metadata
    }

    /// Count ordinary texts and independently retained intent texts, excluding shared immutable copies.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.texts.iter().map(|text| text.len()).sum::<usize>()
            + self.prepared.as_ref().map_or(0, |intent| {
                let output = if intent.no_op { 0 } else { intent.output.len() };
                output
                    + if self
                        .texts
                        .iter()
                        .any(|text| Arc::ptr_eq(text, &intent.applied_draft))
                    {
                        0
                    } else {
                        intent.applied_draft.len()
                    }
            })
    }

    /// Metadata is projected on demand; shared immutable owner/base strings avoid copies during uploads.
    ///
    /// # Errors
    /// Rejects a metadata serialization failure without returning a partial checkpoint.
    pub fn part(&self, part: Part) -> Result<Arc<str>, Error> {
        match part {
            Part::Metadata => serde_json::to_string(&self.metadata)
                .map(Arc::from)
                .map_err(|_| Error::InvalidRecord),
            Part::Owner => Ok(Arc::clone(&self.texts[0])),
            Part::Base => Ok(Arc::clone(&self.texts[1])),
            Part::Draft => Ok(Arc::clone(&self.texts[2])),
        }
    }

    /// Keep ordinary checkpoints in v1; only an unfinished intent needs the separate v2 envelope.
    pub(super) fn encode(&self) -> Result<Vec<u8>, Error> {
        let mut value = self.metadata.clone();
        let fields = value.as_object_mut().ok_or(Error::InvalidRecord)?;
        fields.insert(
            "storageVersion".into(),
            Value::from(if self.prepared.is_some() { 2 } else { 1 }),
        );
        if let Some(intent) = &self.prepared {
            fields.insert("prepared".into(), intent.encode(&self.texts));
        }
        for (name, text) in ["ownerText", "base", "draft"].into_iter().zip(&self.texts) {
            fields.insert(name.into(), Value::String(STANDARD.encode(text.as_bytes())));
        }
        let bytes = serde_json::to_vec(&value).map_err(|_| Error::InvalidRecord)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(Error::ResourceLimit);
        }
        Ok(bytes)
    }

    /// Untrusted files cannot inject a source path through a filename or bypass logical byte quotas.
    pub(super) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(Error::ResourceLimit);
        }
        let value: Value = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRecord)?;
        let Value::Object(mut fields) = value else {
            return Err(Error::InvalidRecord);
        };
        let prepared = match fields.remove("storageVersion") {
            Some(Value::Number(version)) if version.as_u64() == Some(1) => None,
            Some(Value::Number(version)) if version.as_u64() == Some(2) => {
                Some(fields.remove("prepared").ok_or(Error::InvalidRecord)?)
            }
            _ => return Err(Error::InvalidRecord),
        };
        let texts = ["ownerText", "base", "draft"].map(|name| decode_text(fields.remove(name)));
        let [owner, base, draft] = texts;
        let record = Self::new(
            Value::Object(fields),
            [owner?.into(), base?.into(), draft?.into()],
        )?;
        let prepared = prepared
            .map(|value| Prepared::decode(value, &record.texts))
            .transpose()?;
        record.with_prepared(prepared)
    }
}

/// Canonical v4 IDs cannot escape the private directory or alias another spelling of a record.
#[must_use]
pub fn is_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    bytes.len() == 36
        && bytes[14] == b'4'
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        && bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)
            }
        })
}

/// Canonical base64 plus UTF-8 validation preserves BOM, NUL and unfinished source text exactly.
pub(super) fn decode_text(value: Option<Value>) -> Result<String, Error> {
    let Some(Value::String(text)) = value else {
        return Err(Error::InvalidRecord);
    };
    if text.len() > MAX_SNAPSHOT_BYTES.div_ceil(3) * 4 {
        return Err(Error::ResourceLimit);
    }
    let bytes = STANDARD.decode(&text).map_err(|_| Error::InvalidRecord)?;
    if bytes.len() > MAX_SNAPSHOT_BYTES || STANDARD.encode(&bytes) != text {
        return Err(Error::InvalidRecord);
    }
    String::from_utf8(bytes).map_err(|_| Error::InvalidRecord)
}

/// Binding shape is independent of an IDE; actual project ownership is checked again on resume.
fn validate_metadata(value: &Value, owner: &str) -> Result<(), Error> {
    let fields = value.as_object().ok_or(Error::InvalidRecord)?;
    if fields.len() != METADATA_FIELDS.len()
        || METADATA_FIELDS.iter().any(|key| !fields.contains_key(*key))
        || value["schemaVersion"] != 1
        || !text(value, "id").is_some_and(is_id)
        || text(value, "projectKey").is_none_or(str::is_empty)
        || !valid_node(&value["node"])
        || !valid_path(&value["source"])
        || !valid_path(&value["descriptor"])
        || ![
            "descriptorSnapshot",
            "diskSnapshot",
            "ownerSnapshot",
            "contextFingerprint",
        ]
        .iter()
        .all(|key| text(value, key).is_some_and(is_snapshot))
        || text(value, "ownerSnapshot") != Some(snapshot(owner).as_str())
        || !text(value, "ownerVersion").is_some_and(|text| {
            text.parse::<u64>()
                .is_ok_and(|n| n > 0 && n.to_string() == text)
        })
        || !value["writable"].is_boolean()
        || !(value["application"].is_null()
            || text(value, "application").is_some_and(|text| !text.is_empty()))
        || !valid_identity(&value["identity"], owner.len())
    {
        return Err(Error::InvalidRecord);
    }
    if serde_json::to_vec(fields)
        .map_err(|_| Error::InvalidRecord)?
        .len()
        > MAX_METADATA_BYTES
    {
        return Err(Error::ResourceLimit);
    }
    Ok(())
}

/// JSON metadata never relies on lossy native string conversion.
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// Remembered hashes are canonical lowercase SHA-256 values.
fn is_snapshot(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Source paths remain data; the store never follows them while reading or writing checkpoints.
fn valid_path(value: &Value) -> bool {
    value.as_object().is_some_and(|fields| fields.len() == 2)
        && text(value, "value").is_some_and(|text| !text.contains('\0'))
        && matches!(
            text(value, "encoding"),
            Some("utf-8" | "percent" | "utf-16-percent")
        )
}

/// A node binding contains an opaque metadata identity rather than a tree index or translated label.
fn valid_node(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    let nonempty = |key| text(value, key).is_some_and(|text| !text.is_empty());
    match text(value, "kind") {
        Some("object") => fields.len() == 2 && nonempty("objectId"),
        Some("module") => {
            fields.len() == 3
                && nonempty("owner")
                && matches!(
                    text(value, "role"),
                    Some(
                        "module"
                            | "object"
                            | "manager"
                            | "record-set"
                            | "value-manager"
                            | "managed-application"
                            | "ordinary-application"
                            | "session"
                            | "external-connection"
                            | "command"
                    )
                )
        }
        Some("collection") => {
            fields.len() == 3 && nonempty("owner") && valid_collection(&value["collection"])
        }
        _ => false,
    }
}

/// Collection records can be retained for review but do not make a collection an editable source.
fn valid_collection(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    match text(value, "kind") {
        Some("metadata") => {
            fields.len() == 2 && text(value, "metadataKind").is_some_and(|text| !text.is_empty())
        }
        Some("common" | "modules" | "unsupported") => fields.len() == 1,
        _ => false,
    }
}

/// Literal ranges are meaningful only inside the remembered complete owner snapshot.
fn valid_identity(value: &Value, bytes: usize) -> bool {
    match text(value, "kind") {
        Some("dataSet") => value["path"].as_array().is_some_and(|path| {
            !path.is_empty()
                && path.len() <= 64
                && path
                    .iter()
                    .all(|part| part.as_str().is_some_and(|text| !text.is_empty()))
        }),
        Some("dynamicList") => ["name", "id"]
            .iter()
            .all(|key| text(value, key).is_some_and(|text| !text.is_empty())),
        Some("bslLiteral") => value["range"]["start"]
            .as_u64()
            .zip(value["range"]["end"].as_u64())
            .is_some_and(|(start, end)| {
                start < end && usize::try_from(end).is_ok_and(|end| end <= bytes)
            }),
        _ => false,
    }
}
