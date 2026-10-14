//! Conservative byte comparison never drops unmapped files or approves a changed native policy.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    Error, FileEntry, Inventory, NativeUuid, Package, SelectedFile, SourceImportChange,
    SourceImportConflict, SourceImportConflictKind, SourcePath, file_identity, files,
};

pub(super) struct Selection {
    pub output: Inventory,
    pub selected: BTreeMap<SourcePath, SelectedFile>,
    pub changes: Vec<SourceImportChange>,
    pub conflicts: Vec<SourceImportConflict>,
    pub incoming_data_unchanged: bool,
}

/// Compare stable identities and select a reverse map independently of its native fingerprints.
pub(super) fn select(
    working: &Inventory,
    ancestor: &Package,
    incoming: &Package,
    approved: &Package,
    current_package: &Package,
) -> Result<Selection, Error> {
    let old = file_identity::files(ancestor)?;
    let new = file_identity::files(incoming)?;
    let current = file_identity::files(current_package)?;
    let approved_paths = file_identity::files(approved)?;
    let mut selected = BTreeMap::new();
    let mut changes = Vec::new();
    let mut conflicts = Vec::new();
    let mut incoming_data_unchanged = shape(&ancestor.manifest) == shape(&incoming.manifest);
    let marker = SourcePath::parse(files::MANIFEST)?;
    let map = selected_map(ancestor, incoming, current_package, &mut conflicts)?;
    let output_paths = file_identity::files(map)?;
    let keys = old
        .keys()
        .chain(new.keys())
        .chain(current.keys())
        .collect::<BTreeSet<_>>();
    for key in keys {
        let old_entry = old
            .get(key)
            .and_then(|path| ancestor.snapshot.files.get(path));
        let new_entry = new
            .get(key)
            .and_then(|path| incoming.snapshot.files.get(path));
        let local = current.get(key).and_then(|path| working.files.get(path));
        let path = output_paths
            .get(key)
            .or_else(|| current.get(key))
            .or_else(|| new.get(key))
            .or_else(|| old.get(key))
            .ok_or_else(|| Error::root("source_file_unmapped"))?;
        let uuid = file_identity::uuid(key);
        let bookkeeping =
            key == "path:ConfigDumpInfo.xml" && approved.manifest.external_root.is_none();
        if !bookkeeping && !same(old_entry, new_entry) {
            incoming_data_unchanged = false;
        }
        if key.starts_with("authority:") {
            let approved_entry = approved_paths
                .get(key)
                .and_then(|path| approved.snapshot.files.get(path));
            if !same(new_entry, approved_entry) || !same(local, approved_entry) {
                conflicts.push(conflict(
                    SourceImportConflictKind::SupportUpdateRequired,
                    path,
                    uuid.clone(),
                ));
            }
        }
        // Dump bookkeeping remains pinned export evidence, not editable metadata or supplier rights.
        // Keep raw incoming versions in ancestry while the protected source retains its original file.
        let choice = if bookkeeping {
            local.cloned().map(SelectedFile::Working)
        } else {
            choose(old_entry, new_entry, local, path, uuid, &mut conflicts)
        };
        let output_entry = choice.as_ref().map(|choice| selected_entry(path, choice));
        if old_entry != new_entry || !same(old_entry, local) {
            changes.push(SourceImportChange {
                identity: key.clone(),
                ancestor: old_entry.cloned(),
                incoming: new_entry.cloned(),
                working: local.cloned(),
                selected: output_entry,
            });
        }
        if let Some(choice) = choice
            && selected.insert(path.clone(), choice).is_some()
        {
            return Err(Error::root("source_path_collision"));
        }
    }
    let chosen_manifest = manifest_choice(map, incoming, working, &marker)?;
    selected.insert(marker, chosen_manifest);
    record_extras(working, &current, &current_package.snapshot, &mut conflicts);
    let files = selected
        .iter()
        .map(|(path, choice)| (path.clone(), selected_entry(path, choice)))
        .collect::<BTreeMap<_, _>>();
    let directories = map.snapshot.directories.clone();
    let output = Inventory {
        revision: files::revision(&files, &directories),
        files,
        directories,
    };
    Ok(Selection {
        output,
        selected,
        changes,
        conflicts,
        incoming_data_unchanged,
    })
}

/// Preview and output inventory use the same selected bytes at their authoritative source path.
pub(super) fn selected_entry(path: &SourcePath, choice: &SelectedFile) -> FileEntry {
    let mut entry = match choice {
        SelectedFile::Working(entry)
        | SelectedFile::Incoming(entry)
        | SelectedFile::Merged { entry, .. } => entry.clone(),
    };
    entry.path = path.clone();
    entry
}

/// Source lineage ignores original native byte fingerprints while retaining every declared binding.
fn shape(
    manifest: &crate::project_core::source_format::SourceManifest,
) -> crate::project_core::source_format::SourceManifest {
    let mut shape = manifest.clone();
    shape.input_revision.clear();
    for entry in &mut shape.files {
        entry.bytes = 0;
        entry.sha256.clear();
    }
    shape
}

/// Whole-file comparison preserves independent files and surfaces any overlapping byte changes.
fn choose(
    old: Option<&FileEntry>,
    incoming: Option<&FileEntry>,
    working: Option<&FileEntry>,
    path: &SourcePath,
    uuid: Option<NativeUuid>,
    conflicts: &mut Vec<SourceImportConflict>,
) -> Option<SelectedFile> {
    if same(old, working) || same(incoming, working) {
        return incoming.cloned().map(SelectedFile::Incoming);
    }
    if same(old, incoming) {
        return working.cloned().map(SelectedFile::Working);
    }
    let kind = if incoming.is_none() {
        SourceImportConflictKind::DeletedModified
    } else if working.is_none() {
        SourceImportConflictKind::ModifiedDeleted
    } else {
        SourceImportConflictKind::BothModified
    };
    conflicts.push(conflict(kind, path, uuid));
    working.cloned().map(SelectedFile::Working)
}

/// Path spelling does not hide unchanged bytes or turn a UUID-preserving rename into a new object.
fn same(left: Option<&FileEntry>, right: Option<&FileEntry>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.bytes == right.bytes && left.sha256 == right.sha256,
        _ => false,
    }
}

/// Preview includes all undeclared data instead of silently discarding local files or directories.
fn record_extras(
    working: &Inventory,
    current: &BTreeMap<String, SourcePath>,
    approved: &Inventory,
    conflicts: &mut Vec<SourceImportConflict>,
) {
    let paths = current.values().collect::<BTreeSet<_>>();
    for path in working.files.keys() {
        if path.as_str() != files::MANIFEST && !paths.contains(path) {
            conflicts.push(conflict(SourceImportConflictKind::UnmappedFile, path, None));
        }
    }
    let directories = approved.directories.iter().collect::<BTreeSet<_>>();
    for path in &working.directories {
        if !directories.contains(path) {
            conflicts.push(conflict(
                SourceImportConflictKind::UnmappedDirectory,
                path,
                None,
            ));
        }
    }
}

/// Policy details are attached only by the authoritative protected selector.
fn conflict(
    kind: SourceImportConflictKind,
    path: &SourcePath,
    uuid: Option<NativeUuid>,
) -> SourceImportConflict {
    SourceImportConflict {
        kind,
        path: path.clone(),
        uuid,
        exclusion: None,
        coordinate: Vec::new(),
    }
}

/// Divergent declaration maps are surfaced before any file can be published at an ambiguous path.
fn selected_map<'a>(
    ancestor: &Package,
    incoming: &'a Package,
    current: &'a Package,
    conflicts: &mut Vec<SourceImportConflict>,
) -> Result<&'a Package, Error> {
    let marker = SourcePath::parse(files::MANIFEST)?;
    let map = if shape(&current.manifest) == shape(&incoming.manifest) {
        current
    } else if shape(&current.manifest) == shape(&ancestor.manifest) {
        incoming
    } else if shape(&incoming.manifest) == shape(&ancestor.manifest)
        || shape(&current.manifest) == shape(&incoming.manifest)
    {
        current
    } else {
        conflicts.push(conflict(
            SourceImportConflictKind::MappingUnsupported,
            &marker,
            None,
        ));
        current
    };
    Ok(map)
}

/// A retained map uses the captured working bytes even when JSON whitespace differs from its base.
fn manifest_choice(
    map: &Package,
    incoming: &Package,
    working: &Inventory,
    marker: &SourcePath,
) -> Result<SelectedFile, Error> {
    let selected = if std::ptr::eq(map, incoming) {
        &map.snapshot
    } else {
        working
    };
    let entry = selected
        .files
        .get(marker)
        .ok_or_else(|| Error::root("source_descriptor_missing"))?
        .clone();
    Ok(if std::ptr::eq(map, incoming) {
        SelectedFile::Incoming(entry)
    } else {
        SelectedFile::Working(entry)
    })
}
