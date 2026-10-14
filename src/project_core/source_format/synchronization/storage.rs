//! Source-only content-addressed storage shares no permission state with accepted baselines.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use super::{
    Error, FileEntry, ImportAncestor, Inventory, MARKER, Manifest, SourcePath, files, valid_hash,
};
use crate::project_core::source_format::FileRole;

/// Resolve a new sibling directory without letting a snapshot consume its own input or output.
pub(super) fn destination(source: &Path, destination: &Path) -> Result<PathBuf, Error> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    SourcePath::parse(name)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let destination = parent
        .canonicalize()
        .map_err(|error| files::io(&error))?
        .join(name);
    if destination.starts_with(source) || source.starts_with(&destination) {
        return Err(Error::root("source_tree_overlap"));
    }
    Ok(destination)
}

/// Copy each unique blob once, verifying its bytes while the ancestor marker remains unpublished.
pub(super) fn persist(
    source: &Path,
    destination: &Path,
    snapshot: &Inventory,
    marker: &[u8],
) -> Result<(), Error> {
    fs::create_dir(destination.join("blobs")).map_err(|error| files::io(&error))?;
    let mut copied = BTreeSet::new();
    for entry in snapshot.files.values() {
        if copied.insert(&entry.sha256) {
            files::copy_to(
                source,
                &destination.join(blob_path(entry)?.as_path()),
                entry,
            )?;
        }
    }
    if Inventory::read(source)? != *snapshot {
        return Err(Error::root("source_input_changed"));
    }
    files::write(&destination.join(MARKER), marker)
}

/// Verify source names, hierarchy and every deduplicated storage blob without trusting marker paths.
pub(super) fn verify(manifest: &Manifest, storage: &Inventory) -> Result<Inventory, Error> {
    let corrupt = || Error::root("source_ancestor_corrupt");
    if manifest.schema_version != 1 || !valid_hash(&manifest.source_revision) {
        return Err(corrupt());
    }
    let mut names = BTreeSet::new();
    let mut files = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    for entry in &manifest.files {
        if !matches!(entry.role, FileRole::Preserved)
            || !names.insert(entry.path.as_str().to_lowercase())
        {
            return Err(corrupt());
        }
        files.insert(entry.path.clone(), entry.clone());
        let blob = blob_entry(entry)?;
        if let Some(previous) = blobs.insert(blob.path.clone(), blob.clone())
            && previous != blob
        {
            return Err(corrupt());
        }
    }
    let directories = &manifest.directories;
    if directories.windows(2).any(|pair| pair[0] >= pair[1])
        || directories
            .iter()
            .any(|path| !names.insert(path.as_str().to_lowercase()))
    {
        return Err(corrupt());
    }
    let parents = directories
        .iter()
        .map(SourcePath::as_str)
        .collect::<BTreeSet<_>>();
    for path in files.keys().chain(directories) {
        if let Some((parent, _)) = path.as_str().rsplit_once('/')
            && !parents.contains(parent)
        {
            return Err(corrupt());
        }
    }
    let revision = files::revision(&files, directories);
    if revision != manifest.source_revision
        || !files.contains_key(&SourcePath::parse(files::MANIFEST)?)
    {
        return Err(corrupt());
    }
    let marker = SourcePath::parse(MARKER)?;
    if storage.files.len() != blobs.len() + 1
        || !storage.files.contains_key(&marker)
        || storage.directories != [SourcePath::parse("blobs")?]
        || blobs
            .iter()
            .any(|(path, entry)| storage.files.get(path) != Some(entry))
    {
        return Err(corrupt());
    }
    Ok(Inventory {
        files,
        directories: directories.clone(),
        revision,
    })
}

/// Restore only into absent or byte-identical complete output, leaving support stores untouched.
pub(super) fn restore(ancestor: &ImportAncestor, destination: &Path) -> Result<(), Error> {
    ancestor.verify()?;
    let destination = self::destination(&ancestor.root, destination)?;
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            if Inventory::read(&destination)? != ancestor.package.snapshot {
                return Err(Error::root("source_ancestor_output_mismatch"));
            }
            ancestor.verify()?;
            return Ok(());
        }
        Ok(_) => return Err(Error::root("source_destination_exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(files::io(&error)),
    }
    fs::create_dir(&destination).map_err(|error| files::io(&error))?;
    let result = write_source(ancestor, &destination);
    if let Err(error) = result {
        fs::remove_dir_all(&destination).map_err(|_| Error::root("source_cleanup_failed"))?;
        return Err(error);
    }
    Ok(())
}

/// Publish the reverse map last, after all logical paths and bytes have been checked.
fn write_source(ancestor: &ImportAncestor, destination: &Path) -> Result<(), Error> {
    for path in &ancestor.package.snapshot.directories {
        fs::create_dir(destination.join(path.as_path())).map_err(|error| files::io(&error))?;
    }
    let marker = SourcePath::parse(files::MANIFEST)?;
    let ordered = ancestor
        .package
        .snapshot
        .files
        .values()
        .filter(|entry| entry.path != marker)
        .chain(ancestor.package.snapshot.files.get(&marker));
    for entry in ordered {
        files::copy_to(
            &ancestor.root,
            &destination.join(entry.path.as_path()),
            &blob_entry(entry)?,
        )?;
    }
    ancestor.verify()?;
    if Inventory::read(destination)? != ancestor.package.snapshot {
        return Err(Error::root("source_input_changed"));
    }
    Ok(())
}

/// Derive a storage path solely from a validated content digest, never from arbitrary marker text.
pub(super) fn blob_path(entry: &FileEntry) -> Result<SourcePath, Error> {
    if !valid_hash(&entry.sha256) {
        return Err(Error::root("source_ancestor_corrupt"));
    }
    SourcePath::parse(&format!("blobs/{}", entry.sha256))
}

/// Preserve the original byte count and digest while addressing a blob through its physical path.
fn blob_entry(entry: &FileEntry) -> Result<FileEntry, Error> {
    Ok(FileEntry {
        path: blob_path(entry)?,
        role: FileRole::Preserved,
        ..entry.clone()
    })
}

/// Package reconstruction consumes one complete snapshot, independently of the storage inventory.
pub(super) fn clone_inventory(inventory: &Inventory) -> Inventory {
    Inventory {
        files: inventory.files.clone(),
        directories: inventory.directories.clone(),
        revision: inventory.revision.clone(),
    }
}
