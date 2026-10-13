//! Expected-byte migration publishes a new source tree; it never rewrites the selected input.

mod cst;
mod forms;
pub(in crate::project_core::source_format) mod publication;
mod queries;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    DESCRIPTOR_LAYOUT_VERSION, contexts, descriptor_path,
    resources::{self, Layout},
};
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
pub struct SourceLayoutPlan {
    root: PathBuf,
    original: Inventory,
    projection: Projection,
    changes: Vec<LayoutChange>,
}

#[derive(Debug)]
pub(in crate::project_core::source_format) struct Projection {
    pub manifest: SourceManifest,
    pub snapshot: Inventory,
    pub text: BTreeMap<SourcePath, String>,
    pub renamed: BTreeMap<SourcePath, FileEntry>,
}

/// Prepare a separated layout from a completely validated family or separated source tree.
///
/// Native input hashes, payload bytes, UUIDs and ordered composition remain unchanged. The
/// accepted support base is a separate authority and is not replaced or reaccepted by this API.
///
/// # Errors
/// Returns invalid composition, path collisions, unsupported schemas or changed-input failures.
pub fn prepare_descriptor_layout(source: &Path) -> Result<SourceLayoutPlan, Error> {
    prepare(source, Layout::Descriptors)
}

/// Prepare separate descriptors and mapped payload paths from a validated v1, v2 or v3 source.
///
/// Existing authoring text, opaque bytes and native provenance are preserved; the accepted base
/// requires its own independently pinned plan. Changes publish into a new owned directory.
///
/// # Errors
/// Returns invalid composition, path collisions, unsupported trivia, schemas or changed input.
pub fn prepare_resource_layout(source: &Path) -> Result<SourceLayoutPlan, Error> {
    prepare(source, Layout::Resources)
}

/// Prepare named property envelopes without promoting any preserved value to a typed property.
///
/// Physical paths use layout 3. Existing v2 descriptor text is an exact no-op; legacy annotations
/// follow their concrete fields. Mixed transport or unsafe CST reindentation refuses publication.
///
/// # Errors
/// Returns unsupported authored trivia, invalid inputs, collisions or changed-source failures.
pub fn prepare_readable_layout(source: &Path) -> Result<SourceLayoutPlan, Error> {
    prepare(source, Layout::Readable)
}

/// Combine declared managed dialogs with their readable descriptors in explicitly selected map 4.
///
/// Current payload values are converted independently of accepted storage; modules remain separate.
/// Existing map 4 retains every byte. Original native fingerprints and identities never change.
///
/// # Errors
/// Returns unverified dialog profiles, unsupported trivia, invalid inputs or publication conflicts.
pub fn prepare_form_layout(source: &Path) -> Result<SourceLayoutPlan, Error> {
    prepare(source, Layout::Forms)
}

/// Extract sole plain query values into map 5 without accepting current values into any baseline.
///
/// Already selected map 5 is a byte no-op. Older layouts first receive readable managed forms;
/// original native evidence, authoring annotations and unrelated payloads remain unchanged.
///
/// # Errors
/// Returns unsupported query/form profiles, unsafe trivia, collisions, invalid or changed inputs.
pub fn prepare_query_layout(source: &Path) -> Result<SourceLayoutPlan, Error> {
    prepare(source, Layout::Queries)
}

/// Both explicit targets share validation, exact preview and resumable publication.
fn prepare(source: &Path, target: Layout) -> Result<SourceLayoutPlan, Error> {
    let package = Package::read(source)?;
    let root = fs::canonicalize(source).map_err(|error| files::io(&error))?;
    let projection = project_layout(&package, target, |path| {
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
    Ok(SourceLayoutPlan {
        root,
        original: package.snapshot,
        projection,
        changes,
    })
}

impl SourceLayoutPlan {
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

    /// An already selected layout is preserved byte-for-byte, including hand-written YAML style.
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
        publication::publish(self.publication_view(), destination)
    }

    /// Source and accepted-storage migrations share the same exact-byte publication rules.
    pub(in crate::project_core::source_format) fn publication_view(&self) -> publication::View<'_> {
        publication::View {
            root: &self.root,
            input: &self.original,
            output: &self.projection.snapshot,
            text: &self.projection.text,
            ready: files::MANIFEST,
            renamed: Some(&self.projection.renamed),
        }
    }
}

/// Upgrade physical paths after descriptor splitting; never downgrade an already newer layout.
pub(in crate::project_core::source_format) fn project_layout(
    package: &Package,
    target: Layout,
    mut read: impl FnMut(&SourcePath) -> Result<String, Error>,
) -> Result<Projection, Error> {
    let mut read = |path: &SourcePath| {
        let text = read(path)?;
        if matches!(target, Layout::Readable | Layout::Forms | Layout::Queries)
            && package.owners.contains_key(path)
        {
            crate::project_core::source_format::readable::check_transport(&text)?;
        }
        Ok(text)
    };
    let mut projection = project_descriptors(package, &mut read)?;
    if projection.manifest.schema_version >= super::super::MANAGED_FORM_LAYOUT_VERSION
        && target.version() <= projection.manifest.schema_version
    {
        return Ok(projection);
    }
    if target.version() <= projection.manifest.schema_version {
        if matches!(target, Layout::Readable) {
            project_readable(&mut projection, &mut read)?;
        }
        return Ok(projection);
    }
    if projection.manifest.schema_version < super::super::RESOURCE_LAYOUT_VERSION {
        project_resources(&mut projection)?;
    }
    if matches!(target, Layout::Readable | Layout::Forms | Layout::Queries)
        && projection.manifest.schema_version < super::super::MANAGED_FORM_LAYOUT_VERSION
    {
        project_readable(&mut projection, &mut read)?;
    }
    if matches!(target, Layout::Forms | Layout::Queries)
        && projection.manifest.schema_version < super::super::MANAGED_FORM_LAYOUT_VERSION
    {
        forms::project(package, &mut projection, &mut read)?;
    }
    if matches!(target, Layout::Queries) {
        queries::project(package, &mut projection, &mut read)?;
    }
    Ok(projection)
}

/// Payload relocation is independent of subsequent readable envelopes and managed dialog attachment.
fn project_resources(projection: &mut Projection) -> Result<(), Error> {
    let before = projection.manifest.clone();
    resources::relocate(&mut projection.manifest)?;
    projection.manifest.validate()?;
    for (old, new) in before
        .files
        .iter()
        .zip(&projection.manifest.files)
        .filter(|(_, new)| new.is_payload())
    {
        if old.source_path() != new.source_path() {
            let mut entry = projection
                .snapshot
                .files
                .remove(old.source_path())
                .ok_or_else(|| Error::root("source_file_missing"))?;
            projection
                .renamed
                .insert(new.source_path().clone(), entry.clone());
            entry.path = new.source_path().clone();
            if projection
                .snapshot
                .files
                .insert(entry.path.clone(), entry)
                .is_some()
            {
                return Err(Error::root("source_path_collision"));
            }
        }
    }
    let marker = SourcePath::parse(files::MANIFEST)?;
    let text = crate::project_core::source_format::package::manifest_text(&projection.manifest)?;
    projection
        .snapshot
        .files
        .insert(marker.clone(), fingerprint(&marker, &text));
    projection.text.insert(marker, text);
    projection.snapshot.directories = resources::source_directories(&projection.manifest)?;
    projection.snapshot.revision =
        files::revision(&projection.snapshot.files, &projection.snapshot.directories);
    Ok(())
}

/// Envelope version and physical-map version are independent; only explicitly selected YAML moves.
fn project_readable(
    projection: &mut Projection,
    mut read: impl FnMut(&SourcePath) -> Result<String, Error>,
) -> Result<(), Error> {
    let owners: BTreeSet<_> = projection
        .manifest
        .files
        .iter()
        .filter_map(|file| match &file.role {
            FileRole::Descriptor { source, .. } => Some(source.clone()),
            _ => None,
        })
        .collect();
    for owner in owners {
        let original = match projection.text.get(&owner) {
            Some(text) => text.clone(),
            None => read(&owner)?,
        };
        let updated = crate::project_core::source_format::readable::upgrade_text(&original)?;
        if original != updated {
            projection
                .snapshot
                .files
                .insert(owner.clone(), fingerprint(&owner, &updated));
            projection.text.insert(owner, updated);
        }
    }
    projection.snapshot.revision =
        files::revision(&projection.snapshot.files, &projection.snapshot.directories);
    Ok(())
}

/// Preserve current separated YAML exactly; family sources use the lossless CST splitter.
fn project_descriptors(
    package: &Package,
    mut read: impl FnMut(&SourcePath) -> Result<String, Error>,
) -> Result<Projection, Error> {
    let mut manifest = package.manifest.clone();
    if manifest.schema_version >= DESCRIPTOR_LAYOUT_VERSION {
        return Ok(Projection {
            manifest,
            snapshot: Inventory {
                files: package.snapshot.files.clone(),
                directories: package.snapshot.directories.clone(),
                revision: package.snapshot.revision.clone(),
            },
            text: BTreeMap::new(),
            renamed: BTreeMap::new(),
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
        renamed: BTreeMap::new(),
    })
}

/// Compute a source fingerprint without retaining binary payloads in the preview.
pub(super) fn fingerprint(path: &SourcePath, text: &str) -> FileEntry {
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
pub(in crate::project_core::source_format) fn changes(
    before: &Inventory,
    after: &Inventory,
) -> Vec<LayoutChange> {
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
