//! Three-way source selection keeps new authority independent of local working bytes.

mod publication;

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{
    AcceptedBase, AcceptedRef, Error, FileEntry, Inventory, NativeUuid, Operation, Permission,
    SourcePath, files,
};
use crate::project_core::source_format::{
    Exclusion, export::Package, file_identity as identity, protected,
};

/// A conflict always leaves the original working tree and both accepted stores available.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseUpdateConflictKind {
    BothModified,
    DeletedModified,
    ModifiedDeleted,
    AuthorityEdited,
    PermissionDenied,
    FreezeRemoved,
    UnmappedFile,
    UnmappedDirectory,
}

/// All participating paths are explicit; names never replace metadata identity.
#[derive(Clone, Debug, Serialize)]
pub struct BaseUpdateConflict {
    pub kind: BaseUpdateConflictKind,
    pub previous: Option<SourcePath>,
    pub incoming: Option<SourcePath>,
    pub working: Option<SourcePath>,
    pub uuid: Option<NativeUuid>,
    pub exclusion: Option<Exclusion>,
}

/// File-level comparison includes unchanged content carried across a supplier rename.
#[derive(Clone, Debug, Serialize)]
pub struct BaseUpdateChange {
    pub previous: Option<FileEntry>,
    pub incoming: Option<FileEntry>,
    pub working: Option<FileEntry>,
    pub selected: Option<FileEntry>,
    pub uuid: Option<NativeUuid>,
}

/// Native rules and inherited freeze are reviewed independently of ordinary source changes.
#[derive(Clone, Debug, Serialize)]
pub struct SupportChange {
    pub uuid: NativeUuid,
    pub previous: Permission,
    pub incoming: Permission,
}

/// No Git state, source JSON field or implicit reacceptance can change the selected pins.
pub struct BaseUpdatePlan {
    source: PathBuf,
    working: Inventory,
    previous_root: PathBuf,
    incoming_root: PathBuf,
    previous: AcceptedRef,
    incoming: AcceptedRef,
    previous_storage: Inventory,
    incoming_storage: Inventory,
    output: Inventory,
    selected: BTreeMap<SourcePath, SelectedFile>,
    changes: Vec<BaseUpdateChange>,
    conflicts: Vec<BaseUpdateConflict>,
    support: Vec<SupportChange>,
}

/// Concrete source locations allow unchanged large binary payloads to be streamed during write.
#[derive(Clone)]
enum SelectedFile {
    Working(FileEntry),
    Incoming(FileEntry),
}

impl SelectedFile {
    /// Fingerprints belong to original physical paths until publication chooses the new path.
    const fn entry(&self) -> &FileEntry {
        match self {
            Self::Working(entry) | Self::Incoming(entry) => entry,
        }
    }
}

/// Compare old accepted bytes, independently supplied new authority and the current source tree.
///
/// Concurrent changes to one file remain a conflict. UUID-bound modules follow supplier renames;
/// descriptors and queries retain their complete original text rather than losing authored trivia.
/// Removing an explicit freeze is a conflict, even if incoming rights would permit local changes.
///
/// # Errors
/// Refuses incompatible identity/layout, altered working declarations, invalid source or stale input.
pub fn prepare_base_update(
    source: &Path,
    previous: &AcceptedBase,
    incoming: &AcceptedBase,
) -> Result<BaseUpdatePlan, Error> {
    let previous_storage = previous.verified_inventory()?;
    let incoming_storage = incoming.verified_inventory()?;
    let source = source.canonicalize().map_err(|error| files::io(&error))?;
    if previous.overlaps(&source) || incoming.overlaps(&source) {
        return Err(Error::root("source_baseline_overlap"));
    }
    compatible(previous, incoming)?;
    let working = Inventory::read(&source)?;
    let manifest: crate::project_core::source_format::SourceManifest =
        serde_json::from_str(&files::read_text(&source.join(files::MANIFEST))?)
            .map_err(|_| Error::root("source_schema_invalid"))?;
    if manifest != previous.package.manifest {
        return Err(Error::root("source_update_declarations_changed"));
    }
    let Selection {
        selected,
        changes,
        conflicts,
    } = select_sources(&working, previous, incoming)?;
    let output = output_inventory(incoming, &selected)?;
    let mut plan = BaseUpdatePlan {
        source,
        working,
        previous_root: previous.root.clone(),
        incoming_root: incoming.root.clone(),
        previous: previous.reference.clone(),
        incoming: incoming.reference.clone(),
        previous_storage,
        incoming_storage,
        output,
        selected,
        changes,
        conflicts,
        support: support_changes(previous, incoming)?,
    };
    let package = Package::from_snapshot(clone_inventory(&plan.output), |path| {
        plan.read(path, incoming)
    })?;
    for exclusion in
        protected::update_exclusions(&plan.output, incoming, |path| plan.read(path, incoming))?
    {
        plan.conflicts.push(BaseUpdateConflict {
            kind: BaseUpdateConflictKind::PermissionDenied,
            previous: None,
            incoming: Some(exclusion.path.clone()),
            working: Some(exclusion.path.clone()),
            uuid: exclusion.uuid.clone(),
            exclusion: Some(exclusion),
        });
    }
    if plan.conflicts.is_empty() {
        crate::project_core::source_format::structural_references::validate_references(
            incoming,
            &package,
            |path| plan.read(path, incoming),
        )?;
        package.native_xml()?;
    }
    plan.recheck()?;
    Ok(plan)
}

/// File choices and their explanations are prepared together before semantic validation.
#[derive(Default)]
struct Selection {
    selected: BTreeMap<SourcePath, SelectedFile>,
    changes: Vec<BaseUpdateChange>,
    conflicts: Vec<BaseUpdateConflict>,
}

/// Resolve UUID-bound file slots, then expose undeclared data instead of discarding it.
fn select_sources(
    working: &Inventory,
    previous: &AcceptedBase,
    incoming: &AcceptedBase,
) -> Result<Selection, Error> {
    let old_files = identity::files(&previous.package)?;
    let new_files = identity::files(&incoming.package)?;
    let mut result = Selection::default();
    let keys = old_files
        .keys()
        .chain(new_files.keys())
        .collect::<BTreeSet<_>>();
    for key in keys {
        let old_path = old_files.get(key);
        let new_path = new_files.get(key);
        let old = old_path.and_then(|path| previous.package.snapshot.files.get(path));
        let new = new_path.and_then(|path| incoming.package.snapshot.files.get(path));
        let local_path = old_path.or_else(|| {
            new_path.filter(|path| !previous.package.snapshot.files.contains_key(*path))
        });
        let local = local_path.and_then(|path| working.files.get(path));
        let uuid = identity::uuid(key);
        let choice = select(
            old,
            new,
            local,
            key.starts_with("authority:"),
            &mut result.conflicts,
            uuid.clone(),
        );
        let destination = new_path
            .cloned()
            .or_else(|| local.map(|entry| entry.path.clone()));
        let selected_entry = choice
            .as_ref()
            .zip(destination.as_ref())
            .map(|(choice, path)| {
                let mut entry = choice.entry().clone();
                entry.path = path.clone();
                entry
            });
        if old != new || !same(old, local) {
            result.changes.push(BaseUpdateChange {
                previous: old.cloned(),
                incoming: new.cloned(),
                working: local.cloned(),
                selected: selected_entry,
                uuid,
            });
        }
        if let Some((path, choice)) = destination.zip(choice)
            && result.selected.insert(path.clone(), choice).is_some()
        {
            return Err(Error::root("source_path_collision").in_field(path.as_str()));
        }
    }
    let manifest_path = SourcePath::parse(files::MANIFEST)?;
    result.selected.insert(
        manifest_path.clone(),
        SelectedFile::Incoming(
            incoming
                .package
                .snapshot
                .files
                .get(&manifest_path)
                .ok_or_else(|| Error::root("source_descriptor_missing"))?
                .clone(),
        ),
    );
    record_unmapped(
        working,
        previous,
        incoming,
        &old_files,
        &new_files,
        &mut result.conflicts,
    )?;
    for uuid in &previous.manifest.frozen {
        if !incoming.frozen.contains(&uuid.key()) {
            result.conflicts.push(BaseUpdateConflict {
                kind: BaseUpdateConflictKind::FreezeRemoved,
                previous: None,
                incoming: None,
                working: None,
                uuid: Some(uuid.clone()),
                exclusion: None,
            });
        }
    }
    Ok(result)
}

/// Local files and empty directories outside either declared map must remain visible in preview.
fn record_unmapped(
    working: &Inventory,
    previous: &AcceptedBase,
    incoming: &AcceptedBase,
    old_files: &BTreeMap<String, SourcePath>,
    new_files: &BTreeMap<String, SourcePath>,
    conflicts: &mut Vec<BaseUpdateConflict>,
) -> Result<(), Error> {
    let manifest_path = SourcePath::parse(files::MANIFEST)?;
    let mapped = old_files
        .values()
        .chain(new_files.values())
        .collect::<BTreeSet<_>>();
    for path in working
        .files
        .keys()
        .filter(|path| **path != manifest_path && !mapped.contains(path))
    {
        conflicts.push(BaseUpdateConflict {
            kind: BaseUpdateConflictKind::UnmappedFile,
            previous: None,
            incoming: None,
            working: Some(path.clone()),
            uuid: None,
            exclusion: None,
        });
    }
    let declared_directories = previous
        .package
        .snapshot
        .directories
        .iter()
        .chain(&incoming.package.snapshot.directories)
        .collect::<BTreeSet<_>>();
    for path in &working.directories {
        if !declared_directories.contains(path) {
            conflicts.push(BaseUpdateConflict {
                kind: BaseUpdateConflictKind::UnmappedDirectory,
                previous: None,
                incoming: None,
                working: Some(path.clone()),
                uuid: None,
                exclusion: None,
            });
        }
    }
    Ok(())
}

impl BaseUpdatePlan {
    /// CI keeps the original trust reference until the new project is explicitly selected.
    #[must_use]
    pub const fn previous_reference(&self) -> &AcceptedRef {
        &self.previous
    }

    /// Incoming bytes and rights were supplied independently of current authoring files.
    #[must_use]
    pub const fn incoming_reference(&self) -> &AcceptedRef {
        &self.incoming
    }

    /// Every compared value uses exact byte fingerprints and explicit physical paths.
    #[must_use]
    pub fn changes(&self) -> &[BaseUpdateChange] {
        &self.changes
    }

    /// A nonempty conflict list blocks both source and accepted-storage publication.
    #[must_use]
    pub fn conflicts(&self) -> &[BaseUpdateConflict] {
        &self.conflicts
    }

    /// Changes include native restrictions, newly introduced identities and inherited freeze.
    #[must_use]
    pub fn support_changes(&self) -> &[SupportChange] {
        &self.support
    }

    /// Bind approval to the full captured working source, including paths and empty directories.
    #[must_use]
    pub fn working_revision(&self) -> &str {
        &self.working.revision
    }

    /// The selected tree's revision is reproducible and independent of output location.
    #[must_use]
    pub fn output_revision(&self) -> &str {
        &self.output.revision
    }

    /// Read only text consumed by composition validation; large opaque payloads stay streamed.
    fn read(&self, path: &SourcePath, incoming: &AcceptedBase) -> Result<String, Error> {
        let selected = self
            .selected
            .get(path)
            .ok_or_else(|| Error::root("source_file_missing"))?;
        let text = match selected {
            SelectedFile::Working(entry) => {
                files::read_text(&self.source.join(entry.path.as_path()))?
            }
            SelectedFile::Incoming(entry) => incoming.read_file(&entry.path)?,
        };
        let expected = selected.entry();
        if text.len() as u64 != expected.bytes
            || files::hex(&Sha256::digest(text.as_bytes())) != expected.sha256
        {
            return Err(Error::root("source_input_changed").in_field(path.as_str()));
        }
        Ok(text)
    }

    /// Neither a completed old authority nor an imported new authority may change after preview.
    pub(in crate::project_core::source_format) fn recheck(&self) -> Result<(), Error> {
        if Inventory::read(&self.source)? != self.working
            || Inventory::read(&self.previous_root)? != self.previous_storage
            || Inventory::read(&self.incoming_root)? != self.incoming_storage
        {
            return Err(Error::root("source_input_changed"));
        }
        Ok(())
    }
}

/// Incoming data can move names, but cannot select another native identity or source codec.
fn compatible(previous: &AcceptedBase, incoming: &AcceptedBase) -> Result<(), Error> {
    let old = &previous.package.manifest;
    let new = &incoming.package.manifest;
    if old.schema_version != new.schema_version
        || old.profile != new.profile
        || old.root_kind() != new.root_kind()
    {
        return Err(Error::root("source_update_profile_mismatch"));
    }
    let root = |base: &AcceptedBase| {
        base.package
            .documents
            .get(&base.package.manifest.root_path()?)
            .map(|document| document.descriptor.object.uuid.key())
            .ok_or_else(|| Error::root("source_descriptor_missing"))
    };
    if root(previous)? != root(incoming)? {
        return Err(Error::root("source_identity_mismatch"));
    }
    let project_type = |base: &AcceptedBase| {
        let document = base
            .package
            .documents
            .get(&base.package.manifest.root_path()?)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        crate::project_core::source_format::root::project_type(&document.descriptor)
    };
    if project_type(previous)? != project_type(incoming)? {
        return Err(Error::root("source_update_profile_mismatch"));
    }
    Ok(())
}

/// Unchanged hashes allow a local module to follow a renamed incoming owner without guesswork.
fn same(left: Option<&FileEntry>, right: Option<&FileEntry>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.bytes == right.bytes && left.sha256 == right.sha256,
        _ => false,
    }
}

/// Authority payloads and concurrent deletion/edit pairs cannot silently choose a winner.
fn select(
    old: Option<&FileEntry>,
    new: Option<&FileEntry>,
    local: Option<&FileEntry>,
    authority: bool,
    conflicts: &mut Vec<BaseUpdateConflict>,
    uuid: Option<NativeUuid>,
) -> Option<SelectedFile> {
    let conflict = if authority && !same(old, local) {
        Some(BaseUpdateConflictKind::AuthorityEdited)
    } else if same(old, local) || same(new, local) {
        return new.cloned().map(SelectedFile::Incoming);
    } else if old.is_some() && new.is_none() && local.is_some() {
        Some(BaseUpdateConflictKind::DeletedModified)
    } else if old.is_some() && local.is_none() {
        Some(BaseUpdateConflictKind::ModifiedDeleted)
    } else if same(old, new) {
        return local.cloned().map(SelectedFile::Working);
    } else {
        Some(BaseUpdateConflictKind::BothModified)
    };
    if let Some(kind) = conflict {
        conflicts.push(BaseUpdateConflict {
            kind,
            previous: old.map(|file| file.path.clone()),
            incoming: new.map(|file| file.path.clone()),
            working: local.map(|file| file.path.clone()),
            uuid,
            exclusion: None,
        });
    }
    new.cloned().map(SelectedFile::Incoming)
}

/// Incoming declarations own directories; renamed old folders cannot become undeclared output.
fn output_inventory(
    incoming: &AcceptedBase,
    selected: &BTreeMap<SourcePath, SelectedFile>,
) -> Result<Inventory, Error> {
    let files = selected
        .iter()
        .map(|(path, choice)| {
            let mut entry = choice.entry().clone();
            entry.path = path.clone();
            (path.clone(), entry)
        })
        .collect::<BTreeMap<_, _>>();
    let mut directories = incoming
        .package
        .snapshot
        .directories
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for path in files.keys() {
        for (end, _) in path.as_str().match_indices('/') {
            directories.insert(SourcePath::parse(&path.as_str()[..end])?);
        }
    }
    let directories = directories.into_iter().collect::<Vec<_>>();
    Ok(Inventory {
        revision: files::revision(&files, &directories),
        files,
        directories,
    })
}

/// Clone only snapshot metadata; payload bytes are never duplicated in the comparison model.
fn clone_inventory(input: &Inventory) -> Inventory {
    Inventory {
        files: input.files.clone(),
        directories: input.directories.clone(),
        revision: input.revision.clone(),
    }
}

/// A pin change alone is not a permission change; reasons and decisions are compared separately.
fn support_changes(
    previous: &AcceptedBase,
    incoming: &AcceptedBase,
) -> Result<Vec<SupportChange>, Error> {
    let mut changes = Vec::new();
    for key in previous.objects.keys().chain(
        incoming
            .objects
            .keys()
            .filter(|key| !previous.objects.contains_key(*key)),
    ) {
        let uuid = NativeUuid::parse(key)?;
        let before = previous.permission(&uuid, Operation::Module);
        let after = incoming.permission(&uuid, Operation::Module);
        if before.allowed != after.allowed
            || before.state != after.state
            || before.native_reason != after.native_reason
            || before.reason != after.reason
        {
            changes.push(SupportChange {
                uuid,
                previous: before,
                incoming: after,
            });
        }
    }
    Ok(changes)
}
