//! Bounded uploads keep the original 1 MiB frame/queue contract intact.

use std::collections::BTreeMap;

use eska_project_core::project::{metadata_edit::snapshot, query::MAX_SNAPSHOT_BYTES};
use serde_json::{Value, json};

use super::{
    INLINE_BYTES, State, failure,
    params::{self, Key},
};
use crate::ide::{envelope::error, params::decode};

pub(super) struct Upload {
    pub key: Key,
    pub bytes: usize,
    pub snapshot: String,
    pub text: String,
}

impl Upload {
    /// Pending content cannot be used as an owner or query value even if it happens to parse.
    pub const fn ready(&self) -> bool {
        self.bytes == self.text.len()
    }
}

impl State {
    /// Reserve aggregate storage before accepting any body chunks.
    pub(super) fn begin_buffer(&mut self, key: &Key, args: &Value) -> Result<Value, Value> {
        let input: params::BufferBegin = decode(args)?;
        if input.bytes > MAX_SNAPSHOT_BYTES {
            return Err(crate::ide::envelope::domain(
                "resource_limit",
                json!({"resource":"query_snapshot","maxBytes":MAX_SNAPSHOT_BYTES}),
            ));
        }
        if input.snapshot.len() != 64
            || !input
                .snapshot
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(error(-32602));
        }
        if input.bytes == 0 && input.snapshot != snapshot("") {
            return Err(failure("snapshot_mismatch"));
        }
        self.admit(input.bytes)?;
        let id = self.next_id("b")?;
        self.buffers.insert(
            id.clone(),
            Upload {
                key: key.clone(),
                bytes: input.bytes,
                snapshot: input.snapshot,
                text: String::with_capacity(input.bytes),
            },
        );
        Ok(json!({"bufferId":id,"receivedBytes":0,"ready":input.bytes==0}))
    }

    /// An append is atomic with respect to offset, size and final content identity.
    pub(super) fn append_buffer(&mut self, key: &Key, args: &Value) -> Result<Value, Value> {
        let input: params::BufferAppend = decode(args)?;
        if input.text.is_empty() {
            return Err(error(-32602));
        }
        if input.text.len() > INLINE_BYTES {
            return Err(inline_limit());
        }
        let upload = self
            .buffers
            .get_mut(&input.buffer_id)
            .filter(|upload| &upload.key == key)
            .ok_or_else(|| failure("unknown_buffer"))?;
        if upload.ready()
            || input.offset != upload.text.len()
            || input.text.len() > upload.bytes - upload.text.len()
        {
            return Err(failure("upload_offset"));
        }
        upload.text.push_str(&input.text);
        let ready = upload.ready();
        let received = upload.text.len();
        if ready && snapshot(&upload.text) != upload.snapshot {
            self.buffers.remove(&input.buffer_id);
            return Err(failure("snapshot_mismatch"));
        }
        Ok(json!({"bufferId":input.buffer_id,"receivedBytes":received,"ready":ready}))
    }

    /// Inline and uploaded variants share one exclusive source value, never an unverified concatenation.
    pub(super) fn input_text<'a>(
        &'a self,
        key: &Key,
        text: Option<&'a str>,
        buffer: Option<&str>,
    ) -> Result<Option<&'a str>, Value> {
        match (text, buffer) {
            (Some(text), None) if text.len() <= INLINE_BYTES => Ok(Some(text)),
            (Some(_), None) => Err(inline_limit()),
            (None, Some(id)) => self
                .buffers
                .get(id)
                .filter(|upload| &upload.key == key && upload.ready())
                .map(|upload| Some(upload.text.as_str()))
                .ok_or_else(|| failure("unknown_buffer")),
            (None, None) => Ok(None),
            (Some(_), Some(_)) => Err(error(-32602)),
        }
    }
}

/// A small frame limit is distinct from the connection's larger retained-byte budget.
fn inline_limit() -> Value {
    crate::ide::envelope::domain(
        "resource_limit",
        json!({"resource":"query_inline","maxBytes":INLINE_BYTES}),
    )
}

/// Cleanup accepts stale generations but never another session/member's retained data.
pub(super) fn close(
    buffers: &mut BTreeMap<String, Upload>,
    key: &Key,
    id: &str,
) -> Result<Value, Value> {
    if buffers
        .get(id)
        .is_some_and(|upload| !upload.key.same_owner(key))
    {
        return Err(failure("unknown_buffer"));
    }
    Ok(json!({"released":buffers.remove(id).is_some()}))
}
