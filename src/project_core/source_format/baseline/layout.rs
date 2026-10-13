//! Relocate only independently pinned source bytes; current working edits never enter authority.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use super::{
    AcceptedBase, AcceptedRef, BASE_MANIFEST, Error, FileEntry, FileRole, Inventory, Path, PathBuf,
    SourceManifest, SourcePath, blob_path, files,
};
use crate::project_core::source_format::{
    LayoutChange, LayoutPublication,
    layout::migration::{self, Projection, publication},
    layout::resources::Layout,
};
use sha2::{Digest, Sha256};

/// An exact layout plan preserves the old native evidence, supplier rules and explicit freeze.
#[derive(Debug)]
pub struct AcceptedLayoutPlan {
    root: PathBuf,
    input: Inventory,
    output: Inventory,
    text: BTreeMap<SourcePath, String>,
    manifest: SourceManifest,
    source_changes: Vec<LayoutChange>,
    previous: AcceptedRef,
    accepted: AcceptedRef,
}

/// Prepare a new accepted layout from verified blobs, without reading any working source tree.
///
/// The caller must retain the independently supplied old pin and explicitly select the returned
/// new pin afterwards. This operation does not update project settings or CI trust references.
///
/// # Errors
/// Returns corrupt or changed old storage, invalid layout, unsupported trivia or schema failures.
pub fn prepare_accepted_descriptor_layout(
    base: &AcceptedBase,
) -> Result<AcceptedLayoutPlan, Error> {
    prepare(base, Layout::Descriptors)
}

/// Migrate verified accepted blobs to explicit payload paths without accepting working changes.
///
/// Policy, supplier evidence, freeze and original native fingerprints remain independently pinned.
/// The returned new pin must be selected explicitly by the caller after publication.
///
/// # Errors
/// Returns invalid or changed accepted storage, unsupported layouts or path collisions.
pub fn prepare_accepted_resource_layout(base: &AcceptedBase) -> Result<AcceptedLayoutPlan, Error> {
    prepare(base, Layout::Resources)
}

/// Convert pinned source blobs to readable envelopes without accepting any current working edits.
///
/// Native policy/freeze and evidence remain pinned; selecting the new trust reference is explicit.
///
/// # Errors
/// Returns changed/corrupt storage, invalid bindings or unsupported authored migration trivia.
pub fn prepare_accepted_readable_layout(base: &AcceptedBase) -> Result<AcceptedLayoutPlan, Error> {
    prepare(base, Layout::Readable)
}

/// Convert independently pinned dialog bytes into map 4 without accepting any current working edits.
///
/// Supplier/freeze policy and native evidence are retained; the returned new pin is selected explicitly.
///
/// # Errors
/// Returns invalid/corrupt storage, unverified dialog profiles, unsupported trivia or changed inputs.
pub fn prepare_accepted_form_layout(base: &AcceptedBase) -> Result<AcceptedLayoutPlan, Error> {
    prepare(base, Layout::Forms)
}

/// Extract independently pinned query values into map 5 while retaining supplier evidence and freeze.
///
/// Working files are never read; the returned new pin must be selected explicitly after publication.
///
/// # Errors
/// Returns changed/corrupt storage, unsupported query profiles, invalid trivia or path collisions.
pub fn prepare_accepted_query_layout(base: &AcceptedBase) -> Result<AcceptedLayoutPlan, Error> {
    prepare(base, Layout::Queries)
}

/// Source and accepted migrations use the same pure projection for each explicit target.
fn prepare(base: &AcceptedBase, target: Layout) -> Result<AcceptedLayoutPlan, Error> {
    base.verify()?;
    let input = Inventory::read(&base.root)?;
    for path in input.files.keys().chain(&input.directories) {
        if path.as_str().split('/').next().is_some_and(|root| {
            root.eq_ignore_ascii_case(crate::project_core::source_format::layout::PENDING_WRITE)
        }) {
            return Err(Error::root("source_path_collision").in_field(path.as_str()));
        }
    }
    let Projection {
        manifest,
        snapshot,
        text: source_text,
        ..
    } = migration::project_layout(&base.package, target, |path| {
        let entry = base
            .package
            .snapshot
            .files
            .get(path)
            .ok_or_else(|| Error::root("source_baseline_blob_missing"))?;
        super::read_blob_entry_cancellable(&base.root, entry, &|| false)
    })?;
    let source_changes = migration::changes(&base.package.snapshot, &snapshot);
    let (output, text, accepted) = if snapshot == base.package.snapshot {
        (
            Inventory {
                files: input.files.clone(),
                directories: input.directories.clone(),
                revision: input.revision.clone(),
            },
            BTreeMap::new(),
            base.reference.clone(),
        )
    } else {
        storage(base, &snapshot, source_text)?
    };
    if Inventory::read(&base.root)? != input {
        return Err(Error::root("source_input_changed"));
    }
    Ok(AcceptedLayoutPlan {
        root: base.root.clone(),
        input,
        output,
        text,
        manifest,
        source_changes,
        previous: base.reference.clone(),
        accepted,
    })
}

impl AcceptedLayoutPlan {
    /// Review physical source paths and exact bytes independently of content-addressed blob names.
    #[must_use]
    pub fn source_changes(&self) -> &[LayoutChange] {
        &self.source_changes
    }

    /// The relocated native reverse map preserves original input hashes and UUIDs.
    #[must_use]
    pub const fn source_manifest(&self) -> &SourceManifest {
        &self.manifest
    }

    /// The original caller-supplied reference remains the only input trust authority.
    #[must_use]
    pub const fn previous_reference(&self) -> &AcceptedRef {
        &self.previous
    }

    /// The caller explicitly supplies this reference when opening the newly published base.
    #[must_use]
    pub const fn accepted_reference(&self) -> &AcceptedRef {
        &self.accepted
    }

    /// An already separated base retains its exact marker, pin and complete storage bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.input == self.output
    }

    /// Publish new accepted storage with the same resumable, exclusive rules as source migration.
    ///
    /// Existing edited/foreign output and changed old authority are refused. No working values
    /// are accepted; the ready accepted marker is published last, then its receipt is removed.
    ///
    /// # Errors
    /// Returns stale input, unsafe destination, foreign output or I/O failures.
    pub fn publish(&self, destination: &Path) -> Result<LayoutPublication, Error> {
        publication::publish(self.publication_view(), destination)
    }

    /// Reuse concrete publication machinery without exposing a plugin or storage abstraction.
    pub(in crate::project_core::source_format) fn publication_view(&self) -> publication::View<'_> {
        publication::View {
            root: &self.root,
            input: &self.input,
            output: &self.output,
            text: &self.text,
            ready: BASE_MANIFEST,
            renamed: None,
        }
    }
}

/// Hash relocated YAML once; unchanged large payloads keep their original content addresses.
fn storage(
    base: &AcceptedBase,
    source: &Inventory,
    source_text: BTreeMap<SourcePath, String>,
) -> Result<(Inventory, BTreeMap<SourcePath, String>, AcceptedRef), Error> {
    let mut manifest = base.manifest.clone();
    manifest.source_revision.clone_from(&source.revision);
    manifest.directories.clone_from(&source.directories);
    manifest.files = source.files.values().cloned().collect();
    let marker = serde_json::to_string_pretty(&manifest)
        .map_err(|_| Error::root("source_schema_invalid"))?;
    if marker.len() as u64 > files::MAX_TEXT_BYTES {
        return Err(Error::root("source_document_too_large"));
    }
    let accepted = AcceptedRef {
        sha256: files::hex(&Sha256::digest(marker.as_bytes())),
    };
    let mut files = BTreeMap::new();
    let mut directories = BTreeSet::new();
    let mut text = BTreeMap::new();
    for entry in source.files.values() {
        let path = blob_path(&entry.sha256)?;
        for (end, _) in path.as_str().match_indices('/') {
            directories.insert(SourcePath::parse(&path.as_str()[..end])?);
        }
        files.insert(
            path.clone(),
            FileEntry {
                path: path.clone(),
                bytes: entry.bytes,
                sha256: entry.sha256.clone(),
                role: FileRole::Preserved,
            },
        );
    }
    for (path, value) in source_text {
        let entry = source
            .files
            .get(&path)
            .ok_or_else(|| Error::root("source_baseline_corrupt"))?;
        text.insert(blob_path(&entry.sha256)?, value);
    }
    let path = SourcePath::parse(BASE_MANIFEST)?;
    files.insert(
        path.clone(),
        FileEntry {
            path: path.clone(),
            bytes: marker.len() as u64,
            sha256: accepted.sha256.clone(),
            role: FileRole::Preserved,
        },
    );
    text.insert(path, marker);
    let directories = directories.into_iter().collect::<Vec<_>>();
    let output = Inventory {
        revision: files::revision(&files, &directories),
        files,
        directories,
    };
    if super::verified_snapshot(&manifest, &output)? != *source {
        return Err(Error::root("source_baseline_corrupt"));
    }
    Ok((output, text, accepted))
}
