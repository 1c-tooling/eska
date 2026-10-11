//! Expected-byte migration publishes a new source tree; it never rewrites the selected input.

mod cst;
pub(super) mod publication;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{DESCRIPTOR_LAYOUT_VERSION, contexts, descriptor_path};
use crate::project_core::source_format::{
    Error, FileEntry, FileRole, SourceManifest, SourcePath,
    export::Package,
    files::{self, Inventory},
};

/// A preview names exact before/after file bytes; an absent side denotes a created/deleted path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutChange {
    pub path: SourcePath,
    pub before: Option<FileEntry>,
    pub after: Option<FileEntry>,
}

/// Publication distinguishes recovery from an exact repeated application without rewriting files.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutPublication {
    Created,
    Resumed,
    Unchanged,
}

/// Migration authority is bound to a complete source inventory and exact output bytes.
#[derive(Debug)]
pub struct DescriptorLayoutPlan {
    root: PathBuf,
    original: Inventory,
    projection: Projection,
    changes: Vec<LayoutChange>,
}

#[derive(Debug)]
pub(super) struct Projection {
    pub manifest: SourceManifest,
    pub snapshot: Inventory,
    pub text: BTreeMap<SourcePath, String>,
}

/// Prepare a separated layout from a completely validated family or separated source tree.
///
/// Native input hashes, payload bytes, UUIDs and ordered composition remain unchanged. The
/// accepted support base is a separate authority and is not replaced or reaccepted by this API.
///
/// # Errors
/// Returns invalid composition, path collisions, unsupported schemas or changed-input failures.
pub fn prepare_descriptor_layout(source: &Path) -> Result<DescriptorLayoutPlan, Error> {
    let package = Package::read(source)?;
    let root = fs::canonicalize(source).map_err(|error| files::io(&error))?;
    let projection = project(&package, |path| {
        files::read_text(&root.join(path.as_path()))
    })?;
    for path in projection
        .snapshot
        .files
        .keys()
        .chain(&projection.snapshot.directories)
    {
        if [publication::RECEIPT, publication::TEMPORARY]
            .iter()
            .any(|reserved| path.as_str().eq_ignore_ascii_case(reserved))
        {
            return Err(Error::root("source_path_collision").in_field(path.as_str()));
        }
    }
    package.recheck(&root)?;
    let changes = changes(&package.snapshot, &projection.snapshot);
    Ok(DescriptorLayoutPlan {
        root,
        original: package.snapshot,
        projection,
        changes,
    })
}

impl DescriptorLayoutPlan {
    /// Return deterministic exact-byte changes; no change means the input already has this layout.
    #[must_use]
    pub fn changes(&self) -> &[LayoutChange] {
        &self.changes
    }

    /// Return the native reverse map which will be the last ready marker in the output.
    #[must_use]
    pub const fn manifest(&self) -> &SourceManifest {
        &self.projection.manifest
    }

    /// Preview newly generated YAML or manifest text; unchanged payloads are streamed verbatim.
    #[must_use]
    pub fn updated_text(&self, path: &SourcePath) -> Option<&str> {
        self.projection.text.get(path).map(String::as_str)
    }

    /// Bind the plan to every input path, directory and exact byte fingerprint.
    #[must_use]
    pub fn input_revision(&self) -> &str {
        &self.original.revision
    }

    /// Bind a completed or resumable output to every planned byte and directory.
    #[must_use]
    pub fn output_revision(&self) -> &str {
        &self.projection.snapshot.revision
    }

    /// An already separated source is preserved byte-for-byte, including hand-written YAML style.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Recheck all input bytes and publish only into a new or exactly owned migration directory.
    ///
    /// An exact completed destination is a no-op. Interrupted output resumes only with this
    /// plan's receipt and an exact subset of expected bytes; edited/foreign entries are refused.
    /// The operation never replaces an existing file or changes the input/accepted base.
    ///
    /// # Errors
    /// Returns changed-input, unsafe/overlapping destination, foreign output or I/O failures.
    pub fn publish(&self, destination: &Path) -> Result<LayoutPublication, Error> {
        publication::publish(self, destination)
    }

    /// No cached parse can authorize copying after any input or directory composition changed.
    fn recheck(&self) -> Result<(), Error> {
        if Inventory::read(&self.root)? != self.original {
            return Err(Error::root("source_input_changed"));
        }
        Ok(())
    }
}

/// The same pure transformation can later migrate pinned bytes without trusting the working tree.
fn project(
    package: &Package,
    mut read: impl FnMut(&SourcePath) -> Result<String, Error>,
) -> Result<Projection, Error> {
    let mut manifest = package.manifest.clone();
    if manifest.schema_version == DESCRIPTOR_LAYOUT_VERSION {
        return Ok(Projection {
            manifest,
            snapshot: Inventory {
                files: package.snapshot.files.clone(),
                directories: package.snapshot.directories.clone(),
                revision: package.snapshot.revision.clone(),
            },
            text: BTreeMap::new(),
        });
    }
    let documents = package
        .documents
        .iter()
        .map(|(path, document)| (path.clone(), document))
        .collect();
    manifest.descriptor_contexts = contexts(&documents)?;
    manifest.schema_version = DESCRIPTOR_LAYOUT_VERSION;
    let mut sources = BTreeMap::new();
    for (owner, parts) in &package.owners {
        let original = read(owner)?;
        let documents = if parts.len() == 1 {
            BTreeMap::from([(parts[0].clone(), original)])
        } else {
            cst::split(&original)?
        };
        for (native, text) in documents {
            let document = package
                .documents
                .get(&native)
                .ok_or_else(|| Error::root("source_descriptor_missing"))?;
            let path = descriptor_path(document.descriptor.object.kind, &native)?;
            if sources.insert(path, text).is_some() {
                return Err(Error::root("source_path_collision"));
            }
        }
    }
    for file in &mut manifest.files {
        if let FileRole::Descriptor { source, .. } = &mut file.role {
            let document = package
                .documents
                .get(&file.path)
                .ok_or_else(|| Error::root("source_descriptor_missing"))?;
            *source = descriptor_path(document.descriptor.object.kind, &file.path)?;
        }
    }
    manifest.validate()?;
    sources.insert(
        SourcePath::parse(files::MANIFEST)?,
        crate::project_core::source_format::package::manifest_text(&manifest)?,
    );
    let mut output = package.snapshot.files.clone();
    for owner in package.owners.keys() {
        output.remove(owner);
    }
    output.remove(&SourcePath::parse(files::MANIFEST)?);
    for (path, text) in &sources {
        if output
            .insert(path.clone(), fingerprint(path, text))
            .is_some()
        {
            return Err(Error::root("source_path_collision").in_field(path.as_str()));
        }
    }
    let mut directories = package
        .snapshot
        .directories
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for path in output.keys() {
        for (end, _) in path.as_str().match_indices('/') {
            directories.insert(SourcePath::parse(&path.as_str()[..end])?);
        }
    }
    validate_paths(&output, &directories)?;
    let directories = directories.into_iter().collect::<Vec<_>>();
    let revision = files::revision(&output, &directories);
    Ok(Projection {
        manifest,
        snapshot: Inventory {
            files: output,
            directories,
            revision,
        },
        text: sources,
    })
}

/// Compute a source fingerprint without retaining binary payloads in the preview.
fn fingerprint(path: &SourcePath, text: &str) -> FileEntry {
    FileEntry {
        path: path.clone(),
        bytes: text.len() as u64,
        sha256: files::hex(&Sha256::digest(text.as_bytes())),
        role: FileRole::Preserved,
    }
}

/// Reject file/directory and case-only aliases before claiming any output path.
fn validate_paths(
    output: &BTreeMap<SourcePath, FileEntry>,
    directories: &BTreeSet<SourcePath>,
) -> Result<(), Error> {
    let mut spellings = BTreeMap::new();
    for path in output.keys().chain(directories) {
        if spellings
            .insert(path.as_str().to_lowercase(), path)
            .is_some()
        {
            return Err(Error::root("source_path_collision").in_field(path.as_str()));
        }
    }
    Ok(())
}

/// Compare sorted path maps; unchanged paths do not appear in a migration preview.
fn changes(before: &Inventory, after: &Inventory) -> Vec<LayoutChange> {
    before
        .files
        .keys()
        .chain(after.files.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|path| {
            let old = before.files.get(path);
            let new = after.files.get(path);
            (old != new).then(|| LayoutChange {
                path: path.clone(),
                before: old.cloned(),
                after: new.cloned(),
            })
        })
        .collect()
}
