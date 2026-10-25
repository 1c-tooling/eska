//! IDE-independent private checkpoints share the existing framed connection, without a project generation.

mod transfer;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use eska_project_core::project::query::recovery::{self as core, Record, Store, Stored};
use serde::{Deserialize, Serialize};
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
            | "query/recoveryPreparedRead"
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
        queries: &mut super::State,
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
        if method == "query/recoveryRemove"
            && let Some(id) = args["id"].as_str()
        {
            queries.constructor_record_guard(id)?;
        }
        let mut result = match method {
            "query/recoveryRead" => self.read(args),
            "query/recoveryPreparedRead" => self.read_prepared(args),
            "query/recoveryBegin" => self.begin(args),
            "query/recoveryAppend" => self.append(args),
            "query/recoveryCommit" => self.commit(args, queries),
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
        chunk(stored, &json!(input.part), &text, input.offset)
    }

    /// Prepared output remains inspectable even when a broken manifest prevents source-session restoration.
    fn read_prepared(&self, args: &Value) -> Result<Value, Value> {
        #[derive(Deserialize, Serialize)]
        #[serde(rename_all = "camelCase")]
        enum Part {
            Output,
            AppliedDraft,
        }
        #[derive(Deserialize)]
        struct Read {
            id: String,
            revision: String,
            part: Part,
            offset: usize,
        }
        let input: Read = decode(args)?;
        let stored = self
            .records
            .get(&input.id)
            .filter(|stored| stored.revision == input.revision)
            .ok_or_else(|| failure("recovery_conflict"))?;
        let prepared = stored
            .record
            .prepared()
            .ok_or_else(|| failure("unknown_apply"))?;
        let text = match input.part {
            Part::Output => &prepared.output,
            Part::AppliedDraft => &prepared.applied_draft,
        };
        chunk(stored, &json!(input.part), text, input.offset)
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
    fn commit(&mut self, args: &Value, queries: &mut super::State) -> Result<Value, Value> {
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
        let mut record = upload.record()?;
        if let Some(previous) = self.records.get(record.id())
            && let Some(prepared) = previous.record.prepared()
        {
            if record.metadata() != previous.record.metadata()
                || [core::Part::Owner, core::Part::Base]
                    .iter()
                    .any(|part| record.part(*part).ok() != previous.record.part(*part).ok())
            {
                return Err(failure("recovery_conflict"));
            }
            record = record
                .with_prepared(Some(prepared.clone()))
                .map_err(|error| storage_error(&error))?;
        }
        let staged = queries.stage_checkpoint(&record, expected.as_deref())?;
        let store = &self
            .store
            .as_ref()
            .ok_or_else(|| failure("unknown_store"))?
            .2;
        let stored = store
            .save(record, expected.as_deref())
            .map_err(|error| storage_error(&error))?;
        let result = header(&stored)?;
        queries.accept_checkpoint(staged, &stored);
        self.records.insert(stored.record.id().to_owned(), stored);
        Ok(result)
    }

    /// A forgotten session cannot bypass its own durable intent by opening another handle for the same owner.
    pub(super) fn pending_owner(
        &self,
        project_key: &str,
        source: &Value,
        except: Option<&str>,
    ) -> Option<&str> {
        self.records
            .values()
            .find(|stored| {
                stored.record.prepared().is_some()
                    && Some(stored.record.id()) != except
                    && stored.record.metadata()["projectKey"] == project_key
                    && stored.record.metadata()["source"] == *source
            })
            .map(|stored| stored.record.id())
    }

    /// Source-session mutations reuse the exact selected private store and never choose a fallback path.
    pub(super) fn checked_record(
        &self,
        store_id: &str,
        id: &str,
        expected: Option<&str>,
        fresh: bool,
    ) -> Result<&Stored, Value> {
        let store = self.selected(store_id)?;
        let stored = self
            .records
            .get(id)
            .ok_or_else(|| failure("recovery_conflict"))?;
        if expected.is_some_and(|expected| expected != stored.revision) {
            return Err(failure("recovery_conflict"));
        }
        if fresh {
            store
                .verify(id, &stored.revision)
                .map_err(|error| storage_error(&error))?;
        }
        Ok(stored)
    }

    /// Publish a state already validated by the common session, with the same disk CAS and location guard.
    pub(super) fn publish(
        &mut self,
        store_id: &str,
        record: Record,
        expected: Option<&str>,
        forbidden: &[PathBuf],
    ) -> Result<Stored, Value> {
        let store = self.selected(store_id)?;
        let directory = &self
            .store
            .as_ref()
            .ok_or_else(|| failure("unknown_store"))?
            .1;
        check_location(directory, forbidden)?;
        if self
            .records
            .get(record.id())
            .map(|stored| stored.revision.as_str())
            != expected
        {
            return Err(failure("recovery_conflict"));
        }
        let stored = store
            .save(record, expected)
            .map_err(|error| storage_error(&error))?;
        self.records
            .insert(stored.record.id().to_owned(), stored.clone());
        Ok(stored)
    }

    /// A stale handle cannot address the same directory after an explicit recovery reopen.
    fn selected(&self, store_id: &str) -> Result<&Store, Value> {
        self.store
            .as_ref()
            .filter(|(id, _, _)| id == store_id)
            .map(|(_, _, store)| store)
            .ok_or_else(|| failure("unknown_store"))
    }

    /// Removal cancels a queued update and cannot remove a checkpoint changed by a second window.
    fn remove(&mut self, args: &Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Remove {
            id: String,
            revision: String,
            #[serde(default, rename = "discardPrepared")]
            discard_prepared: bool,
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
        if previous.record.prepared().is_some() && !input.discard_prepared {
            return Err(failure("pending_apply"));
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
pub(super) fn header(stored: &Stored) -> Result<Value, Value> {
    let mut header = json!({"id":stored.record.id(),"metadata":stored.record.metadata(),"revision":stored.revision,
        "parts":transfer::descriptions(&stored.record)?});
    if let Some(prepared) = stored.record.prepared() {
        let description = |text: &str| json!({"bytes":text.len(),"snapshot":eska_project_core::project::metadata_edit::snapshot(text)});
        header["prepared"] = json!({"applyToken":prepared.token.to_string(),"format":prepared.format,
            "saveOwner":prepared.save_owner,"noOp":prepared.no_op,
            "parts":{"output":description(&prepared.output),"appliedDraft":description(&prepared.applied_draft)}});
    }
    Ok(header)
}

/// Both recovery readers preserve UTF-8 boundaries and the original 128 KiB framed transfer limit.
fn chunk(stored: &Stored, part: &Value, text: &str, offset: usize) -> Result<Value, Value> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Err(error(-32602));
    }
    let mut end = offset.saturating_add(CHUNK_BYTES).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(
        json!({"id":stored.record.id(),"revision":stored.revision,"part":part,"offset":offset,
        "text":&text[offset..end],"nextOffset":end,"complete":end==text.len()}),
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
