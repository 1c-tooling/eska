//! IDE-independent private checkpoints share the existing framed connection, without a project generation.

mod transfer;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use eska_project_core::project::query::recovery::{self as core, Store, Stored};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::ide::{
    envelope::{domain, error},
    params::{self, decode},
};
use transfer::{Field, Upload};

const CHUNK_BYTES: usize = 128 * 1024;

/// One selected private store and one bounded upload belong to this connection only.
#[derive(Default)]
pub(in crate::ide) struct State {
    store: Option<(String, PathBuf, Store)>,
    records: BTreeMap<String, Stored>,
    upload: Option<Upload>,
    serial: u64,
}

/// Notifications cannot allocate or publish a private checkpoint.
pub(super) fn is_method(method: &str) -> bool {
    matches!(
        method,
        "query/recoveryOpen"
            | "query/recoveryRead"
            | "query/recoveryBegin"
            | "query/recoveryAppend"
            | "query/recoveryCommit"
            | "query/recoveryCancel"
            | "query/recoveryRemove"
            | "query/recoveryClose"
    )
}

/// Clients retain the original frame bound and can transfer large texts in acknowledged chunks.
pub(super) fn capability() -> Value {
    json!({"schemaVersion":1,"writesOwner":false,"storageVersion":1,
        "maxRecords":core::MAX_RECORDS,"maxRetainedBytes":core::MAX_RETAINED_BYTES,
        "maxMetadataBytes":core::MAX_METADATA_BYTES,"maxTextBytes":eska_project_core::project::query::MAX_SNAPSHOT_BYTES,
        "maxChunkBytes":CHUNK_BYTES,"maxUploads":1,"compareRevision":true})
}

impl State {
    /// Storage stays available after metadata refresh/reopen flags; it is never permission to edit a source.
    pub(super) fn request(
        &mut self,
        method: &str,
        args: &Value,
        forbidden: &[PathBuf],
    ) -> Result<Value, Value> {
        if method == "query/recoveryOpen" {
            return self.open(args, forbidden);
        }
        let input: StoreId = decode(args)?;
        if self
            .store
            .as_ref()
            .is_none_or(|(id, _, _)| id != &input.store_id)
        {
            return Err(failure("unknown_store"));
        }
        if matches!(
            method,
            "query/recoveryBegin" | "query/recoveryCommit" | "query/recoveryRemove"
        ) {
            let directory = &self
                .store
                .as_ref()
                .ok_or_else(|| failure("unknown_store"))?
                .1;
            check_location(directory, forbidden)?;
        }
        let mut result = match method {
            "query/recoveryRead" => self.read(args),
            "query/recoveryBegin" => self.begin(args),
            "query/recoveryAppend" => self.append(args),
            "query/recoveryCommit" => self.commit(args),
            "query/recoveryRemove" => self.remove(args),
            "query/recoveryCancel" => {
                let input: UpdateId = decode(args)?;
                let released = self
                    .upload
                    .as_ref()
                    .is_some_and(|upload| upload.id == input.update_id);
                if released {
                    self.upload = None;
                }
                Ok(json!({"released":released}))
            }
            "query/recoveryClose" => {
                self.store = None;
                self.upload = None;
                self.records.clear();
                Ok(json!({"released":true}))
            }
            _ => Err(error(-32601)),
        }?;
        result["storeId"] = json!(input.store_id);
        Ok(result)
    }

    /// Read existing checkpoints before granting a new connection-scoped handle to the private directory.
    fn open(&mut self, args: &Value, forbidden: &[PathBuf]) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Open {
            directory: params::Path,
        }
        let input: Open = decode(args)?;
        if self.upload.is_some() {
            return Err(failure("upload_busy"));
        }
        let directory = input.directory.native()?;
        let store = Store::new(directory.clone()).map_err(|error| storage_error(&error))?;
        check_location(&directory, forbidden)?;
        if self
            .store
            .as_ref()
            .is_some_and(|(_, previous, _)| previous != &directory)
        {
            return Err(failure("store_already_open"));
        }
        let contents = store.load().map_err(|error| storage_error(&error))?;
        let id = self.next_id("r")?;
        let records = contents
            .records
            .iter()
            .map(header)
            .collect::<Result<Vec<_>, _>>()?;
        self.records = contents
            .records
            .into_iter()
            .map(|stored| (stored.record.id().to_owned(), stored))
            .collect();
        self.store = Some((id.clone(), directory, store));
        Ok(
            json!({"storeId":id,"records":records,"damaged":contents.damaged,"limited":contents.limited}),
        )
    }

    /// Every read identifies the exact cached checkpoint revision, never a later record from another window.
    fn read(&self, args: &Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Read {
            id: String,
            revision: String,
            part: Field,
            offset: usize,
        }
        let input: Read = decode(args)?;
        let stored = self
            .records
            .get(&input.id)
            .filter(|stored| stored.revision == input.revision)
            .ok_or_else(|| failure("recovery_conflict"))?;
        let text = stored
            .record
            .part(input.part.core())
            .map_err(|error| storage_error(&error))?;
        if input.offset > text.len() || !text.is_char_boundary(input.offset) {
            return Err(error(-32602));
        }
        let mut end = input.offset.saturating_add(CHUNK_BYTES).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Ok(
            json!({"id":input.id,"revision":input.revision,"part":input.part,"offset":input.offset,
            "text":&text[input.offset..end],"nextOffset":end,"complete":end==text.len()}),
        )
    }

    /// A new upload reuses unchanged acknowledged parts; only changed text crosses the pipe.
    fn begin(&mut self, args: &Value) -> Result<Value, Value> {
        let input: transfer::Begin = decode(args)?;
        if self.upload.is_some() {
            return Err(failure("upload_busy"));
        }
        if !core::is_id(&input.id) {
            return Err(failure("recovery_invalid"));
        }
        let id = self.next_id("u")?;
        let previous = self.records.get(&input.id);
        if previous.map(|stored| stored.revision.as_str()) != input.expected_revision.as_deref() {
            return Err(failure("recovery_conflict"));
        }
        let upload = Upload::new(id.clone(), input, previous.map(|stored| &stored.record))?;
        let received = upload.received();
        self.upload = Some(upload);
        Ok(json!({"updateId":id,"received":received}))
    }

    /// Each offset and completed part hash is checked before a commit can write any file.
    fn append(&mut self, args: &Value) -> Result<Value, Value> {
        let input: transfer::Append = decode(args)?;
        let upload = self
            .upload
            .as_mut()
            .filter(|upload| upload.id == input.update_id)
            .ok_or_else(|| failure("unknown_update"))?;
        let result = upload.append(&input);
        if result
            .as_ref()
            .is_err_and(|error| error["data"]["details"]["reason"] == "snapshot_mismatch")
        {
            self.upload = None;
        }
        result
    }

    /// Core validates the record, checks disk revision/quota and publishes a single atomic checkpoint.
    fn commit(&mut self, args: &Value) -> Result<Value, Value> {
        let input: UpdateId = decode(args)?;
        let upload = self
            .upload
            .as_ref()
            .filter(|upload| upload.id == input.update_id)
            .ok_or_else(|| failure("unknown_update"))?;
        if !upload.ready() {
            return Err(failure("upload_incomplete"));
        }
        let upload = self
            .upload
            .take()
            .ok_or_else(|| failure("unknown_update"))?;
        let expected = upload.expected_revision.clone();
        let record = upload.record()?;
        let store = &self
            .store
            .as_ref()
            .ok_or_else(|| failure("unknown_store"))?
            .2;
        let stored = store
            .save(record, expected.as_deref())
            .map_err(|error| storage_error(&error))?;
        let result = header(&stored)?;
        self.records.insert(stored.record.id().to_owned(), stored);
        Ok(result)
    }

    /// Removal cancels a queued update and cannot remove a checkpoint changed by a second window.
    fn remove(&mut self, args: &Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Remove {
            id: String,
            revision: String,
        }
        let input: Remove = decode(args)?;
        if !core::is_id(&input.id) {
            return Err(error(-32602));
        }
        let Some(previous) = self.records.get(&input.id) else {
            return Ok(json!({"released":false}));
        };
        if previous.revision != input.revision {
            return Err(failure("recovery_conflict"));
        }
        let store = &self
            .store
            .as_ref()
            .ok_or_else(|| failure("unknown_store"))?
            .2;
        let released = store
            .remove(&input.id, &input.revision)
            .map_err(|error| storage_error(&error))?;
        if self
            .upload
            .as_ref()
            .is_some_and(|upload| upload.record_id == input.id)
        {
            self.upload = None;
        }
        self.records.remove(&input.id);
        Ok(json!({"released":released}))
    }

    /// Handles cannot alias earlier store/upload state even if a source workspace is reopened.
    fn next_id(&mut self, prefix: &str) -> Result<String, Value> {
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or_else(|| failure("recovery_limit"))?;
        Ok(format!("{prefix}{}", self.serial))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreId {
    store_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateId {
    update_id: String,
}

/// Headers keep startup responses bounded; complete owner text is read only in small requested chunks.
fn header(stored: &Stored) -> Result<Value, Value> {
    Ok(
        json!({"id":stored.record.id(),"metadata":stored.record.metadata(),"revision":stored.revision,
        "parts":transfer::descriptions(&stored.record)?}),
    )
}

/// Resolve an existing ancestor for containment checks without creating the requested directory.
fn physical_path(path: &Path) -> Result<PathBuf, Value> {
    let mut parent = path;
    loop {
        match parent.canonicalize() {
            Ok(real) => {
                return Ok(real.join(
                    path.strip_prefix(parent)
                        .map_err(|_| failure("recovery_path"))?,
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                parent = parent.parent().ok_or_else(|| failure("recovery_path"))?;
            }
            Err(_) => return Err(failure("recovery_path")),
        }
    }
}

/// Reopening metadata cannot turn an earlier private store into a writable project source directory.
fn check_location(directory: &Path, forbidden: &[PathBuf]) -> Result<(), Value> {
    let physical = physical_path(directory)?;
    for root in forbidden {
        if physical.starts_with(physical_path(root)?) {
            return Err(failure("recovery_in_project"));
        }
    }
    Ok(())
}

/// Recovery diagnostics contain categories, never owner text, local IO messages or arbitrary metadata.
fn storage_error(error: &core::Error) -> Value {
    match error {
        core::Error::ResourceLimit => domain(
            "resource_limit",
            json!({"resource":"query_recovery","maxBytes":core::MAX_RETAINED_BYTES,"maxRecords":core::MAX_RECORDS}),
        ),
        core::Error::InvalidRecord => failure("recovery_invalid"),
        core::Error::UnsafePath => failure("recovery_path"),
        core::Error::Conflict => failure("recovery_conflict"),
        core::Error::Busy => failure("recovery_busy"),
        core::Error::Io(_) => failure("recovery_io"),
    }
}

/// Source and recovery errors share the established query domain while retaining distinct machine reasons.
fn failure(reason: &str) -> Value {
    super::failure(reason)
}
