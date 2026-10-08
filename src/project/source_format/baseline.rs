//! Pinned complete source bytes and policy; working metadata never grants itself permission.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::project::metadata_model::MetadataKind;
use crate::project::support::{Reason, State, Support};

use super::{
    Child, Error, FileEntry, FileRole, MetadataNode, NativeDocument, NativeUuid, PropertyName,
    Section, SourceManifest, SourcePath,
    composition::{reference_path, references},
    export::Package,
    files::{self, Inventory},
};

const BASE_MANIFEST: &str = "accepted.json";

/// A caller-owned trust reference must be supplied independently of editable source YAML in CI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedRef {
    pub sha256: String,
}

/// Accepted bytes are content addressed; caches and the original checkout are not required.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    rules_version: u32,
    profile: String,
    source_revision: String,
    input_revision: String,
    configuration_uuid: NativeUuid,
    directories: Vec<SourcePath>,
    files: Vec<FileEntry>,
    frozen: Vec<NativeUuid>,
    suppliers: Vec<SupplierPayload>,
}

/// The observed profile binds a supplier's native name to its complete preserved CF payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SupplierPayload {
    id: NativeUuid,
    configuration_uuid: NativeUuid,
    path: SourcePath,
    bytes: u64,
    sha256: String,
}

/// The requested operation is narrower than a generic editable flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "property", rename_all = "snake_case")]
pub enum Operation {
    Property(PropertyName),
    Module,
    Resource,
    Create,
    Delete,
    Rename,
    Move,
}

/// Native policy and additional freeze remain separate machine-facing reasons.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionReason {
    Allowed,
    NativeRestriction,
    EskaFrozen,
    UnknownObject,
    UnsupportedOperation,
}

/// A decision includes the pinned base identity, never an editor presentation switch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Permission {
    pub allowed: bool,
    pub uuid: NativeUuid,
    pub state: State,
    pub native_reason: Reason,
    pub reason: PermissionReason,
    pub baseline: String,
}

/// Verified accepted ownership and native supplier restrictions.
pub struct AcceptedBase {
    root: PathBuf,
    reference: AcceptedRef,
    manifest: Manifest,
    policy: Option<Support>,
    objects: BTreeMap<String, Option<String>>,
    frozen: BTreeSet<String>,
    pub(super) package: Package,
}

/// Pin a validated complete ESKA source tree in a new directory, with an explicit freeze list.
///
/// # Errors
/// Returns invalid source/policy, unknown freeze targets, conflicting destination or I/O errors.
/// This records supplied source evidence; it does not authenticate a supplier by its checksum.
pub fn accept_source(
    source: &Path,
    destination: &Path,
    frozen: &[NativeUuid],
) -> Result<AcceptedRef, Error> {
    let package = Package::read(source)?;
    let source = fs::canonicalize(source).map_err(|error| files::io(&error))?;
    let destination = new_destination(&source, destination)?;
    let objects = document_ownership(&package.documents)?;
    let mut freeze = BTreeMap::new();
    for uuid in frozen {
        if !objects.contains_key(&uuid.key()) {
            return Err(Error::root("source_freeze_object_unknown"));
        }
        freeze.insert(uuid.key(), uuid.clone());
    }
    let policy = read_policy(&package, |path| {
        files::read_text(&source.join(path.as_path()))
    })?;
    let suppliers = supplier_payloads(&package, policy.as_ref())?;
    let configuration_uuid = package
        .documents
        .get(&SourcePath::parse("Configuration.xml")?)
        .ok_or_else(|| Error::root("source_descriptor_missing"))?
        .descriptor
        .object
        .uuid
        .clone();
    let manifest = Manifest {
        schema_version: super::SCHEMA_VERSION,
        rules_version: super::RULES_VERSION,
        profile: super::PROFILE.into(),
        source_revision: package.snapshot.revision.clone(),
        input_revision: package.manifest.input_revision.clone(),
        configuration_uuid,
        directories: package.snapshot.directories.clone(),
        files: package.snapshot.files.values().cloned().collect(),
        frozen: freeze.into_values().collect(),
        suppliers,
    };
    let bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|_| Error::root("source_schema_invalid"))?;
    let reference = AcceptedRef {
        sha256: files::hex(&Sha256::digest(&bytes)),
    };
    fs::create_dir(&destination).map_err(|error| files::io(&error))?;
    let result =
        persist(&source, &destination, &manifest, &bytes).and_then(|()| package.recheck(&source));
    if let Err(error) = result {
        fs::remove_dir_all(&destination).map_err(|_| Error::root("source_cleanup_failed"))?;
        return Err(error);
    }
    Ok(reference)
}

impl AcceptedBase {
    /// Open pinned metadata and verify every required blob before deriving any permissions.
    ///
    /// # Errors
    /// Returns missing, corrupt, untrusted or unsupported baseline errors without enabling edits.
    pub fn open(root: &Path, expected: &AcceptedRef) -> Result<Self, Error> {
        let inventory =
            Inventory::read(root).map_err(|_| Error::root("source_baseline_missing"))?;
        let path = SourcePath::parse(BASE_MANIFEST)?;
        let entry = inventory
            .files
            .get(&path)
            .ok_or_else(|| Error::root("source_baseline_missing"))?;
        if entry.sha256 != expected.sha256 {
            return Err(Error::root("source_baseline_untrusted"));
        }
        let text = files::read_text(&root.join(BASE_MANIFEST))?;
        if files::hex(&Sha256::digest(text.as_bytes())) != expected.sha256 {
            return Err(Error::root("source_baseline_untrusted"));
        }
        let manifest: Manifest =
            serde_json::from_str(&text).map_err(|_| Error::root("source_baseline_corrupt"))?;
        if manifest.schema_version != super::SCHEMA_VERSION
            || manifest.rules_version != super::RULES_VERSION
            || manifest.profile != super::PROFILE
        {
            return Err(Error::root("source_baseline_unsupported"));
        }
        let snapshot = verified_snapshot(&manifest, &inventory)?;
        let entries: BTreeMap<_, _> = manifest
            .files
            .iter()
            .map(|file| (&file.path, file))
            .collect();
        let package = Package::from_snapshot(snapshot, |path| {
            read_blob_entry(
                root,
                entries
                    .get(path)
                    .ok_or_else(|| Error::root("source_baseline_blob_missing"))?,
            )
        })?;
        let root_document = package
            .documents
            .get(&SourcePath::parse("Configuration.xml")?)
            .ok_or_else(|| Error::root("source_baseline_corrupt"))?;
        if package.manifest.input_revision != manifest.input_revision
            || root_document.descriptor.object.uuid != manifest.configuration_uuid
        {
            return Err(Error::root("source_baseline_corrupt"));
        }
        let objects = document_ownership(&package.documents)?;
        let policy = read_policy(&package, |path| {
            read_blob_entry(
                root,
                package
                    .snapshot
                    .files
                    .get(path)
                    .ok_or_else(|| Error::root("source_baseline_blob_missing"))?,
            )
        })?;
        if supplier_payloads(&package, policy.as_ref())? != manifest.suppliers {
            return Err(Error::root("source_baseline_corrupt"));
        }
        let mut frozen = BTreeSet::new();
        for uuid in &manifest.frozen {
            if !objects.contains_key(&uuid.key()) || !frozen.insert(uuid.key()) {
                return Err(Error::root("source_baseline_corrupt"));
            }
        }
        if Inventory::read(root)? != inventory {
            return Err(Error::root("source_input_changed"));
        }
        Ok(Self {
            root: fs::canonicalize(root).map_err(|error| files::io(&error))?,
            reference: expected.clone(),
            manifest,
            policy,
            objects,
            frozen,
            package,
        })
    }

    /// Compute permission for an accepted native identity and a concrete operation.
    #[must_use]
    pub fn permission(&self, uuid: &NativeUuid, operation: Operation) -> Permission {
        let (state, native_reason) = if self.objects.contains_key(&uuid.key()) {
            self.policy
                .as_ref()
                .map_or((State::Unrestricted, Reason::OwnObject), |policy| {
                    policy.object(uuid.as_str())
                })
        } else {
            (State::Unknown, Reason::Unavailable)
        };
        let reason = if state == State::Unknown {
            PermissionReason::UnknownObject
        } else if self.frozen(uuid) {
            PermissionReason::EskaFrozen
        } else if state == State::Locked {
            PermissionReason::NativeRestriction
        } else if matches!(
            operation,
            Operation::Create
                | Operation::Delete
                | Operation::Rename
                | Operation::Move
                | Operation::Property(PropertyName::Name)
        ) {
            PermissionReason::UnsupportedOperation
        } else {
            PermissionReason::Allowed
        };
        Permission {
            allowed: reason == PermissionReason::Allowed,
            uuid: uuid.clone(),
            state,
            native_reason,
            reason,
            baseline: self.reference.sha256.clone(),
        }
    }

    /// Restore complete accepted source bytes to a new checkout without any cache or original tree.
    ///
    /// # Errors
    /// Returns conflicting destination, changed baseline, missing blob or filesystem errors.
    pub fn restore(&self, destination: &Path) -> Result<(), Error> {
        let destination = new_destination(&self.root, destination)?;
        Self::open(&self.root, &self.reference)?;
        fs::create_dir(&destination).map_err(|error| files::io(&error))?;
        let result = self.restore_files(&destination);
        if let Err(error) = result {
            fs::remove_dir_all(&destination).map_err(|_| Error::root("source_cleanup_failed"))?;
            return Err(error);
        }
        Ok(())
    }

    /// Additional freeze inherits to owned inline descendants; native supplier rules stay per UUID.
    fn frozen(&self, uuid: &NativeUuid) -> bool {
        let mut current = Some(uuid.key());
        while let Some(key) = current {
            if self.frozen.contains(&key) {
                return true;
            }
            current = self.objects.get(&key).and_then(Clone::clone);
        }
        false
    }

    /// Exclusive writes restore only the caller's newly claimed source directory.
    fn restore_files(&self, destination: &Path) -> Result<(), Error> {
        for path in &self.manifest.directories {
            fs::create_dir_all(destination.join(path.as_path()))
                .map_err(|error| files::io(&error))?;
        }
        for file in self.package.snapshot.files.values() {
            let mut blob = file.clone();
            blob.path = blob_path(&file.sha256)?;
            files::copy_to(&self.root, &destination.join(file.path.as_path()), &blob)?;
        }
        let inventory = Inventory::read(destination)?;
        if inventory.revision != self.manifest.source_revision {
            return Err(Error::root("source_baseline_corrupt"));
        }
        verify_manifest(&self.root, &self.reference)?;
        Ok(())
    }
}

/// Persist each distinct payload once; a ready metadata file is created only after complete copies.
fn persist(
    source: &Path,
    destination: &Path,
    manifest: &Manifest,
    bytes: &[u8],
) -> Result<(), Error> {
    let mut copied = BTreeSet::new();
    for file in &manifest.files {
        if !copied.insert(&file.sha256) {
            continue;
        }
        let blob = blob_path(&file.sha256)?;
        files::copy_to(source, &destination.join(blob.as_path()), file)?;
    }
    files::write(&destination.join(BASE_MANIFEST), bytes)
}

/// A malformed native rule file never becomes a no-support baseline.
fn read_policy(
    package: &Package,
    mut read: impl FnMut(&SourcePath) -> Result<String, Error>,
) -> Result<Option<Support>, Error> {
    let path = SourcePath::parse("Ext/ParentConfigurations.bin")?;
    if !package.snapshot.files.contains_key(&path) {
        return Ok(None);
    }
    unchanged_payload(package, &path)?;
    Support::parse(&read(&path)?)
        .map(Some)
        .map_err(|_| Error::root("source_baseline_support_invalid"))
}

/// Pin native rules together with every supplier CF from the unmodified import evidence.
fn supplier_payloads(
    package: &Package,
    policy: Option<&Support>,
) -> Result<Vec<SupplierPayload>, Error> {
    let actual: BTreeSet<_> = package
        .snapshot
        .files
        .keys()
        .filter(|path| {
            path.as_str().starts_with("Ext/ParentConfigurations/")
                && path
                    .as_path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("cf"))
        })
        .cloned()
        .collect();
    let mut result = Vec::new();
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for supplier in policy.into_iter().flat_map(|policy| &policy.suppliers) {
        SourcePath::parse(&supplier.name)?;
        if supplier.name.contains('/') {
            return Err(Error::root("source_baseline_support_invalid"));
        }
        let path = SourcePath::parse(&format!("Ext/ParentConfigurations/{}.cf", supplier.name))?;
        if !ids.insert(supplier.id.to_ascii_lowercase()) || !paths.insert(path.clone()) {
            return Err(Error::root("source_baseline_support_invalid"));
        }
        if !package.snapshot.files.contains_key(&path) {
            return Err(Error::root("source_baseline_supplier_missing").in_field(path.as_str()));
        }
        let file = unchanged_payload(package, &path)?;
        result.push(SupplierPayload {
            id: NativeUuid::parse(&supplier.id)?,
            configuration_uuid: NativeUuid::parse(&supplier.configuration_uuid)?,
            path,
            bytes: file.bytes,
            sha256: file.sha256.clone(),
        });
    }
    if actual != paths {
        return Err(Error::root("source_baseline_supplier_mismatch"));
    }
    Ok(result)
}

/// Working policy or supplier bytes cannot silently replace the recorded native input.
fn unchanged_payload<'a>(package: &'a Package, path: &SourcePath) -> Result<&'a FileEntry, Error> {
    let original = package
        .manifest
        .files
        .iter()
        .find(|file| &file.path == path)
        .ok_or_else(|| Error::root("source_baseline_support_invalid"))?;
    let current = package
        .snapshot
        .files
        .get(path)
        .ok_or_else(|| Error::root("source_baseline_support_invalid"))?;
    if original.role != FileRole::Preserved
        || original.bytes != current.bytes
        || original.sha256 != current.sha256
    {
        return Err(Error::root("source_baseline_policy_changed").in_field(path.as_str()));
    }
    Ok(current)
}

/// Use validated native references for external ancestry, preserving per-UUID supplier rules.
fn document_ownership(
    documents: &BTreeMap<SourcePath, NativeDocument>,
) -> Result<BTreeMap<String, Option<String>>, Error> {
    let mut parents = BTreeMap::new();
    for (path, document) in documents {
        let node = &document.descriptor.object;
        let base = if node.kind == MetadataKind::Configuration {
            ""
        } else {
            path.as_str()
                .strip_suffix(".xml")
                .ok_or_else(|| Error::root("source_path_invalid"))?
        };
        for (kind, name) in references(node)? {
            let child = reference_path(base, kind, &name)?;
            if !documents.contains_key(&child) || parents.insert(child, node.uuid.key()).is_some() {
                return Err(Error::root("source_baseline_corrupt"));
            }
        }
    }
    let mut objects = BTreeMap::new();
    for (path, document) in documents {
        ownership(
            &document.descriptor.object,
            parents.get(path).cloned(),
            &mut objects,
        )?;
    }
    Ok(objects)
}

/// Record accepted inline ancestry so an ESKA freeze cannot be bypassed through a child.
fn ownership(
    node: &MetadataNode,
    parent: Option<String>,
    values: &mut BTreeMap<String, Option<String>>,
) -> Result<(), Error> {
    if values.insert(node.uuid.key(), parent).is_some() {
        return Err(Error::root("native_uuid_duplicate"));
    }
    for section in &node.sections {
        if let Section::Children(children) = section {
            for child in children {
                if let Child::Inline { object } = child {
                    ownership(object, Some(node.uuid.key()), values)?;
                }
            }
        }
    }
    Ok(())
}

/// Text parsing uses the exact pinned bytes, even if a blob changes after the first inventory.
fn read_blob_entry(root: &Path, file: &FileEntry) -> Result<String, Error> {
    let text = files::read_text(&root.join(blob_path(&file.sha256)?.as_path()))?;
    if text.len() as u64 != file.bytes
        || files::hex(&Sha256::digest(text.as_bytes())) != file.sha256
    {
        return Err(Error::root("source_baseline_corrupt"));
    }
    Ok(text)
}

/// Reconstruct the accepted physical inventory only after every required payload is verified.
fn verified_snapshot(manifest: &Manifest, inventory: &Inventory) -> Result<Inventory, Error> {
    SourceManifest {
        schema_version: manifest.schema_version,
        rules_version: manifest.rules_version,
        profile: manifest.profile.clone(),
        input_revision: manifest.input_revision.clone(),
        directories: manifest.directories.clone(),
        files: manifest.files.clone(),
    }
    .validate()
    .map_err(|_| Error::root("source_baseline_corrupt"))?;
    let mut files = BTreeMap::new();
    for file in &manifest.files {
        if file.role != FileRole::Preserved {
            return Err(Error::root("source_baseline_corrupt"));
        }
        let entry = inventory
            .files
            .get(&blob_path(&file.sha256)?)
            .ok_or_else(|| Error::root("source_baseline_blob_missing"))?;
        if file.bytes != entry.bytes || file.sha256 != entry.sha256 {
            return Err(Error::root("source_baseline_corrupt"));
        }
        if files.insert(file.path.clone(), file.clone()).is_some() {
            return Err(Error::root("source_baseline_corrupt"));
        }
    }
    if files::revision(&files, &manifest.directories) != manifest.source_revision {
        return Err(Error::root("source_baseline_corrupt"));
    }
    Ok(Inventory {
        files,
        directories: manifest.directories.clone(),
        revision: manifest.source_revision.clone(),
    })
}

/// The trust marker must still refer to the same base when a restored checkout is published.
fn verify_manifest(root: &Path, reference: &AcceptedRef) -> Result<(), Error> {
    let text = files::read_text(&root.join(BASE_MANIFEST))?;
    if files::hex(&Sha256::digest(text.as_bytes())) != reference.sha256 {
        return Err(Error::root("source_baseline_untrusted"));
    }
    Ok(())
}

/// Blob filenames have a fixed digest shape before they can authorize filesystem access.
fn blob_path(hash: &str) -> Result<SourcePath, Error> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(Error::root("source_baseline_corrupt"));
    }
    SourcePath::parse(&format!("blobs/{}/{}", &hash[..2], hash))
}

/// Acceptance and restoration never replace an existing user tree or nest inside their input.
fn new_destination(source: &Path, destination: &Path) -> Result<PathBuf, Error> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    SourcePath::parse(name)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let target = fs::canonicalize(parent)
        .map_err(|error| files::io(&error))?
        .join(name);
    if target.starts_with(source) || source.starts_with(&target) {
        return Err(Error::root("source_tree_overlap"));
    }
    match fs::symlink_metadata(&target) {
        Ok(_) => Err(Error::root("source_destination_exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(target),
        Err(error) => Err(files::io(&error)),
    }
}
