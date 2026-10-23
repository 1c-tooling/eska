//! A durable native-application intent is inert recovery data, never an instruction to write a source.

use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Error, record::decode_text};
use crate::project::query::{MAX_SNAPSHOT_BYTES, QueryOwnerFormat};

/// Retain the exact output and decoded applied query even if a newer editable draft replaces it.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub token: u64,
    pub format: QueryOwnerFormat,
    pub output: Arc<str>,
    pub applied_draft: Arc<str>,
    pub save_owner: bool,
    pub no_op: bool,
}

impl Prepared {
    /// Check bounded inert data before it enters a checkpoint or restored source session.
    ///
    /// # Errors
    /// Rejects zero tokens, contradictory no-op/save flags and oversized retained texts.
    pub fn validate(&self) -> Result<(), Error> {
        if self.token == 0 || (self.no_op && self.save_owner) {
            return Err(Error::InvalidRecord);
        }
        if self.output.len() > MAX_SNAPSHOT_BYTES || self.applied_draft.len() > MAX_SNAPSHOT_BYTES {
            return Err(Error::ResourceLimit);
        }
        Ok(())
    }

    /// Storage version 2 adds this intent without changing ordinary v1 binding metadata or text parts.
    pub(super) fn encode(&self, texts: &[Arc<str>; 3]) -> Value {
        json!({"schemaVersion":1,"applyToken":self.token.to_string(),"format":self.format,
            "saveOwner":self.save_owner,"noOp":self.no_op,
            "output":encode_part(&self.output,texts),
            "appliedDraft":encode_part(&self.applied_draft,texts)})
    }

    /// Decode only the exact private shape; future or malformed intent data is preserved as damaged.
    pub(super) fn decode(value: Value, texts: &[Arc<str>; 3]) -> Result<Self, Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Saved {
            schema_version: u8,
            apply_token: String,
            format: QueryOwnerFormat,
            save_owner: bool,
            no_op: bool,
            output: Value,
            applied_draft: Value,
        }
        let saved: Saved = serde_json::from_value(value).map_err(|_| Error::InvalidRecord)?;
        let token = saved
            .apply_token
            .parse::<u64>()
            .map_err(|_| Error::InvalidRecord)?;
        if saved.schema_version != 1 || saved.apply_token != token.to_string() {
            return Err(Error::InvalidRecord);
        }
        let intent = Self {
            token,
            format: saved.format,
            output: decode_part(saved.output, texts)?,
            applied_draft: decode_part(saved.applied_draft, texts)?,
            save_owner: saved.save_owner,
            no_op: saved.no_op,
        };
        intent.validate()?;
        Ok(intent)
    }
}

/// Shared copies must not make a logically bounded store exceed its existing 64 MiB encoded scan budget.
fn encode_part(text: &Arc<str>, texts: &[Arc<str>; 3]) -> Value {
    texts
        .iter()
        .position(|saved| Arc::ptr_eq(saved, text))
        .map_or_else(
            || Value::String(STANDARD.encode(text.as_bytes())),
            |index| {
                let part = ["ownerText", "base", "draft"][index];
                json!({"part":part})
            },
        )
}

/// References address only ordinary parts of this same record, never paths, another record or another reference.
fn decode_part(value: Value, texts: &[Arc<str>; 3]) -> Result<Arc<str>, Error> {
    if let Value::Object(fields) = &value {
        if fields.len() != 1 {
            return Err(Error::InvalidRecord);
        }
        let index = match fields.get("part").and_then(Value::as_str) {
            Some("ownerText") => 0,
            Some("base") => 1,
            Some("draft") => 2,
            _ => return Err(Error::InvalidRecord),
        };
        Ok(Arc::clone(&texts[index]))
    } else {
        decode_text(Some(value)).map(Arc::from)
    }
}
