//! Per-part uploads preserve frame bounds and reuse immutable acknowledged owner/base snapshots.

use std::sync::Arc;

use eska_project_core::project::{
    metadata_edit::snapshot,
    query::recovery::{Part, Record},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{CHUNK_BYTES, failure, storage_error};
use crate::ide::envelope::{domain, error};

const FIELDS: [Field; 4] = [Field::Metadata, Field::OwnerText, Field::Base, Field::Draft];

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Field {
    Metadata,
    OwnerText,
    Base,
    Draft,
}

impl Field {
    /// Wire names never become source paths or IDE URI schemes.
    pub const fn core(self) -> Part {
        match self {
            Self::Metadata => Part::Metadata,
            Self::OwnerText => Part::Owner,
            Self::Base => Part::Base,
            Self::Draft => Part::Draft,
        }
    }

    /// All four positions are explicit rather than inferred from JSON object field order.
    const fn index(self) -> usize {
        match self {
            Self::Metadata => 0,
            Self::OwnerText => 1,
            Self::Base => 2,
            Self::Draft => 3,
        }
    }

    /// Public payload keys are independent of localized UI strings.
    const fn name(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::OwnerText => "ownerText",
            Self::Base => "base",
            Self::Draft => "draft",
        }
    }
}

#[derive(Deserialize)]
pub(super) struct Description {
    bytes: usize,
    snapshot: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Parts {
    metadata: Description,
    owner_text: Description,
    base: Description,
    draft: Description,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Begin {
    pub id: String,
    pub expected_revision: Option<String>,
    parts: Parts,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Append {
    pub update_id: String,
    part: Field,
    offset: usize,
    text: String,
}

struct Buffer {
    description: Description,
    pending: String,
    reused: Option<Arc<str>>,
}

impl Buffer {
    /// Admission precedes allocation, including reservation for uploads abandoned before their first chunk.
    fn new(
        description: Description,
        field: Field,
        previous: Option<&Record>,
    ) -> Result<Self, Value> {
        if description.bytes > field.core().max_bytes() {
            return Err(domain(
                "resource_limit",
                json!({"resource":"query_recovery_part","maxBytes":field.core().max_bytes()}),
            ));
        }
        if description.snapshot.len() != 64
            || !description
                .snapshot
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || (description.bytes == 0 && description.snapshot != snapshot(""))
        {
            return Err(error(-32602));
        }
        let reused = previous
            .map(|record| {
                record
                    .part(field.core())
                    .map_err(|error| storage_error(&error))
            })
            .transpose()?
            .filter(|text| {
                text.len() == description.bytes && snapshot(text) == description.snapshot
            });
        let pending = if reused.is_some() {
            String::new()
        } else {
            String::with_capacity(description.bytes)
        };
        Ok(Self {
            description,
            pending,
            reused,
        })
    }

    /// Reused parts are immutable and already complete; pending offsets always count UTF-8 bytes.
    fn len(&self) -> usize {
        self.reused
            .as_ref()
            .map_or(self.pending.len(), |text| text.len())
    }

    /// A complete part has exactly the reserved byte count, including the empty text case.
    fn ready(&self) -> bool {
        self.len() == self.description.bytes
    }

    /// Moving the completed buffer into the domain record avoids another full owner allocation.
    fn finish(self) -> Arc<str> {
        self.reused.unwrap_or_else(|| self.pending.into())
    }
}

pub(super) struct Upload {
    pub id: String,
    pub record_id: String,
    pub expected_revision: Option<String>,
    parts: [Buffer; 4],
}

impl Upload {
    /// Only one upload can reserve these bounded parts on a connection.
    pub fn new(id: String, input: Begin, previous: Option<&Record>) -> Result<Self, Value> {
        let Parts {
            metadata,
            owner_text,
            base,
            draft,
        } = input.parts;
        let parts = [
            (metadata, Field::Metadata),
            (owner_text, Field::OwnerText),
            (base, Field::Base),
            (draft, Field::Draft),
        ]
        .map(|(description, field)| Buffer::new(description, field, previous));
        let [metadata, owner, base, draft] = parts;
        Ok(Self {
            id,
            record_id: input.id,
            expected_revision: input.expected_revision,
            parts: [metadata?, owner?, base?, draft?],
        })
    }

    /// Acknowledged reuse lets clients send only the changed draft rather than the unchanged owner again.
    pub fn received(&self) -> Value {
        let mut result = json!({});
        for field in FIELDS {
            result[field.name()] = json!(self.parts[field.index()].len());
        }
        result
    }

    /// Reordered, oversized or checksum-mismatching chunks cannot become a committed checkpoint.
    pub fn append(&mut self, input: &Append) -> Result<Value, Value> {
        if input.text.is_empty() || input.text.len() > CHUNK_BYTES {
            return Err(error(-32602));
        }
        let part = &mut self.parts[input.part.index()];
        if part.ready()
            || input.offset != part.len()
            || input.text.len() > part.description.bytes - part.len()
        {
            return Err(failure("upload_offset"));
        }
        part.pending.push_str(&input.text);
        if part.ready() && snapshot(&part.pending) != part.description.snapshot {
            return Err(failure("snapshot_mismatch"));
        }
        Ok(
            json!({"updateId":self.id,"part":input.part,"receivedBytes":part.len(),"ready":part.ready()}),
        )
    }

    /// Partial data cannot be interpreted as metadata or source even when it happens to parse.
    pub fn ready(&self) -> bool {
        self.parts.iter().all(Buffer::ready)
    }

    /// The ID in transferred metadata must agree with the admission request's filename identity.
    pub fn record(self) -> Result<Record, Value> {
        let [metadata, owner, base, draft] = self.parts.map(Buffer::finish);
        let metadata: Value =
            serde_json::from_str(&metadata).map_err(|_| failure("recovery_invalid"))?;
        let record =
            Record::new(metadata, [owner, base, draft]).map_err(|error| storage_error(&error))?;
        if record.id() != self.record_id {
            return Err(failure("recovery_invalid"));
        }
        Ok(record)
    }
}

/// Each header describes the exact UTF-8 data that a later read or upload acknowledgement identifies.
pub(super) fn descriptions(record: &Record) -> Result<Value, Value> {
    let mut result = json!({});
    for field in FIELDS {
        let text = record
            .part(field.core())
            .map_err(|error| storage_error(&error))?;
        result[field.name()] = json!({"bytes":text.len(),"snapshot":snapshot(&text)});
    }
    Ok(result)
}
