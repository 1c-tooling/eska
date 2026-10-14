//! Native XML is projected into owned source staging using the selected project's existing layout.

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use super::{
    AcceptedBase, AcceptedRef, Error, ImportAncestor, Inventory, Project, ProjectScope,
    ProjectSourceImportPlan, SourceFormat, SourcePath, checked_directory, files, fingerprint,
    prepare_project_source_import, require_project_ready, selected_workspace,
};
use crate::project_core::source_format::{
    Descriptor, READABLE_SCHEMA_VERSION, capture_import_ancestor, export_designer, import,
    layout::resources::Layout, root,
};
use sha2::{Digest, Sha256};

/// A new operation directory belongs to this preparation alone, including failed previews.
struct Projection(PathBuf);

impl Drop for Projection {
    /// Published journals live elsewhere; dropping a preview removes only its owned projection.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Native input remains immutable while its converted incoming snapshot participates in the plan.
pub struct NativeProjectSourceImport {
    plan: ProjectSourceImportPlan,
    _projection: Projection,
}

impl NativeProjectSourceImport {
    /// Expose the same preview/publication protocol without another merge or support implementation.
    #[must_use]
    pub const fn plan(&self) -> &ProjectSourceImportPlan {
        &self.plan
    }
}

/// Convert complete native input into the selected project's layout and prepare guarded import.
///
/// External filenames are reverse locations: the verified incoming root and its directory prefix
/// are rebound to the pinned root before conversion. Payload bytes and metadata identity stay exact.
/// No new map, support approval, project settings or source data are installed during preparation.
///
/// # Errors
/// Returns wrong identity/type/scope, unsafe/foreign staging, incomplete native input or stale data.
pub fn prepare_project_native_source_import(
    project: &Project,
    scope: &ProjectScope,
    expected: &AcceptedRef,
    ancestor: &ImportAncestor,
    native: &Path,
    staging: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<NativeProjectSourceImport, Error> {
    files::check_cancelled(cancelled)?;
    require_project_ready(project.root())?;
    selected_workspace(project, scope)?;
    let SourceFormat::Eska { accepted_base } = project.configuration().source_format() else {
        return Err(Error::root("source_format_mismatch"));
    };
    let approved = AcceptedBase::open_cancellable(
        &project.root().join(accepted_base.as_path()),
        expected,
        cancelled,
    )?;
    let snapshot = Inventory::read_cancellable(native, cancelled)?;
    let native = native.canonicalize().map_err(|error| files::io(&error))?;
    checked_directory(&native)?;
    let (incoming_root, _, _) = import::discover_root(&native, &snapshot)?;
    let descriptor = Descriptor::from_xml(&files::read_text_cancellable(
        &native.join(incoming_root.as_path()),
        files::MAX_TEXT_BYTES,
        cancelled,
    )?)?;
    let accepted_root = approved.package.manifest.root_path()?;
    let accepted = &approved
        .package
        .documents
        .get(&accepted_root)
        .ok_or_else(|| Error::root("source_descriptor_missing"))?
        .descriptor;
    if descriptor.object.uuid.key() != accepted.object.uuid.key() {
        return Err(Error::root("source_identity_mismatch"));
    }
    if root::project_type(&descriptor)? != root::project_type(accepted)?
        || root::project_type(accepted)? != project.configuration().project_type()
    {
        return Err(Error::root("source_import_profile_mismatch"));
    }
    let staging = super::super::super::storage::destination(&native, staging)?;
    for input in [
        project.source(),
        approved.storage_root(),
        ancestor.root.as_path(),
        native.as_path(),
    ] {
        if staging.starts_with(input) || input.starts_with(&staging) {
            return Err(Error::root("source_tree_overlap"));
        }
    }
    if fs::symlink_metadata(&staging).is_ok() {
        return Err(Error::root("source_destination_exists"));
    }
    files::check_cancelled(cancelled)?;
    fs::create_dir(&staging).map_err(|error| files::io(&error))?;
    let projection = Projection(staging);
    let input = projection.0.join("native");
    let normalized = normalize(
        &native,
        &input,
        &snapshot,
        &incoming_root,
        &accepted_root,
        cancelled,
    )?;
    let source = projection.0.join("source");
    import::import(&input, &source, layout(&approved)?)?;
    files::check_cancelled(cancelled)?;
    let store = projection.0.join("incoming");
    let reference = capture_import_ancestor(&source, &store)?;
    let incoming = ImportAncestor::open_cancellable(&store, &reference, cancelled)?;
    verify_projection(&projection.0, &incoming, &normalized, cancelled)?;
    let mut plan = prepare_project_source_import(project, scope, expected, ancestor, &incoming)?;
    plan.receipt.native_revision = Some(snapshot.revision.clone());
    plan.source.native = Some(super::super::Input {
        root: native,
        inventory: snapshot,
    });
    (plan.text, plan.reference) = fingerprint(&plan.receipt)?;
    plan.verify(cancelled)?;
    Ok(NativeProjectSourceImport {
        plan,
        _projection: projection,
    })
}

/// Existing schema and descriptor representation choose conversion; opening never migrates a project.
fn layout(approved: &AcceptedBase) -> Result<Layout, Error> {
    Ok(match approved.package.manifest.schema_version {
        1 => Layout::Family,
        2 => Layout::Descriptors,
        3 if approved
            .package
            .documents
            .values()
            .any(|document| document.descriptor.schema_version == READABLE_SCHEMA_VERSION) =>
        {
            Layout::Readable
        }
        3 => Layout::Resources,
        4 => Layout::Forms,
        5 | 6 => Layout::Queries,
        _ => return Err(Error::root("source_import_profile_mismatch")),
    })
}

/// Only the verified external root prefix changes; contents and unknown resource-set internals do not.
fn normalize(
    input: &Path,
    destination: &Path,
    snapshot: &Inventory,
    incoming: &SourcePath,
    accepted: &SourcePath,
    cancelled: &dyn Fn() -> bool,
) -> Result<Inventory, Error> {
    fs::create_dir(destination).map_err(|error| files::io(&error))?;
    let mut names = BTreeSet::new();
    let mut directories = Vec::new();
    let mut entries = BTreeMap::new();
    for path in &snapshot.directories {
        files::check_cancelled(cancelled)?;
        let target = bound_path(path, incoming, accepted)?;
        if !names.insert(target.as_str().to_lowercase()) {
            return Err(Error::root("source_path_collision"));
        }
        fs::create_dir(destination.join(target.as_path())).map_err(|error| files::io(&error))?;
        directories.push(target);
    }
    for entry in snapshot.files.values() {
        let target = bound_path(&entry.path, incoming, accepted)?;
        if !names.insert(target.as_str().to_lowercase()) {
            return Err(Error::root("source_path_collision"));
        }
        files::copy_to_cancellable(input, &destination.join(target.as_path()), entry, cancelled)?;
        let mut stored = entry.clone();
        stored.path = target.clone();
        entries.insert(target, stored);
    }
    if Inventory::read_cancellable(input, cancelled)? != *snapshot {
        return Err(Error::root("source_input_changed"));
    }
    directories.sort();
    let expected = Inventory {
        revision: files::revision(&entries, &directories),
        files: entries,
        directories,
    };
    if Inventory::read_cancellable(destination, cancelled)? != expected {
        return Err(Error::root("source_input_changed"));
    }
    Ok(expected)
}

/// Captured ancestry must export the same native XML values and exact opaque input bytes.
fn verify_projection(
    staging: &Path,
    incoming: &ImportAncestor,
    expected: &Inventory,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), Error> {
    files::check_cancelled(cancelled)?;
    let restored = staging.join("verified-source");
    incoming.restore(&restored)?;
    let roundtrip = staging.join("verified-native");
    export_designer(&restored, &roundtrip)?;
    let actual = Inventory::read_cancellable(&roundtrip, cancelled)?;
    let regenerated = incoming.package.native_xml()?;
    if actual != incoming.package.output_inventory(&regenerated)? {
        return Err(Error::root("source_input_changed"));
    }
    if actual.directories != expected.directories || actual.files.len() != expected.files.len() {
        return Err(Error::root("source_native_projection_mismatch"));
    }
    for (path, entry) in &expected.files {
        files::check_cancelled(cancelled)?;
        if actual.files.get(path) == Some(entry) {
            continue;
        }
        let Some(exported) = regenerated.get(path) else {
            return Err(Error::root("source_native_projection_mismatch").in_field(path.as_str()));
        };
        let original = files::read_text_cancellable(
            &staging.join("native").join(path.as_path()),
            files::MAX_TEXT_BYTES,
            cancelled,
        )?;
        if original.len() as u64 != entry.bytes
            || files::hex(&Sha256::digest(original.as_bytes())) != entry.sha256
        {
            return Err(Error::root("source_input_changed"));
        }
        if !xml_equal(&original, exported, cancelled)? {
            return Err(Error::root("source_native_projection_mismatch").in_field(path.as_str()));
        }
    }
    Ok(())
}

/// Regenerated descriptors may change presentation; names, values, order and namespace bindings may not.
fn xml_equal(left: &str, right: &str, cancelled: &dyn Fn() -> bool) -> Result<bool, Error> {
    let parse = |text| {
        crate::project_core::native::check_envelope(text)
            .map_err(|_| Error::root("source_xml_invalid"))?;
        roxmltree::Document::parse_with_options(
            text,
            roxmltree::ParsingOptions {
                nodes_limit: 1_000_000,
                ..Default::default()
            },
        )
        .map_err(|_| Error::root("source_xml_invalid"))
    };
    let left = parse(left)?;
    let right = parse(right)?;
    let mut pending = vec![(left.root_element(), right.root_element())];
    while let Some((left, right)) = pending.pop() {
        files::check_cancelled(cancelled)?;
        if left.node_type() != right.node_type() {
            return Ok(false);
        }
        if left.is_element() {
            if left.tag_name() != right.tag_name()
                || attributes(left) != attributes(right)
                || namespaces(left) != namespaces(right)
            {
                return Ok(false);
            }
        } else if left.text() != right.text() || left.pi() != right.pi() {
            return Ok(false);
        }
        let left = children(left);
        let right = children(right);
        if left.len() != right.len() {
            return Ok(false);
        }
        pending.extend(left.into_iter().zip(right));
    }
    Ok(true)
}

/// Expanded attributes retain exact values regardless of lexical order or quote spelling.
fn attributes<'a, 'input: 'a>(
    node: roxmltree::Node<'a, 'input>,
) -> BTreeSet<(Option<&'a str>, &'a str, &'a str)> {
    node.attributes()
        .map(|a| (a.namespace(), a.name(), a.value()))
        .collect()
}

/// Prefix bindings remain significant because attribute and text values may contain qualified names.
fn namespaces<'a, 'input: 'a>(
    node: roxmltree::Node<'a, 'input>,
) -> BTreeSet<(Option<&'a str>, &'a str)> {
    node.namespaces().map(|ns| (ns.name(), ns.uri())).collect()
}

/// Empty metadata child lists also have indentation; whitespace-only scalar leaves remain data.
fn children<'a, 'input: 'a>(node: roxmltree::Node<'a, 'input>) -> Vec<roxmltree::Node<'a, 'input>> {
    let container = node.children().any(|child| child.is_element())
        || node.has_tag_name((
            crate::project_core::source_format::MD_NAMESPACE,
            "ChildObjects",
        ));
    node.children()
        .filter(|child| {
            !(container
                && child.is_text()
                && child.text().is_some_and(|text| text.trim().is_empty()))
        })
        .collect()
}

/// File stems are matched as complete components, never as a string prefix of another resource.
fn bound_path(
    path: &SourcePath,
    incoming: &SourcePath,
    accepted: &SourcePath,
) -> Result<SourcePath, Error> {
    if path == incoming {
        return Ok(accepted.clone());
    }
    let old = incoming
        .as_str()
        .strip_suffix(".xml")
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    let new = accepted
        .as_str()
        .strip_suffix(".xml")
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    if path.as_str() == old {
        return SourcePath::parse(new);
    }
    if let Some(tail) = path
        .as_str()
        .strip_prefix(old)
        .and_then(|tail| tail.strip_prefix('/'))
    {
        return SourcePath::parse(&format!("{new}/{tail}"));
    }
    Ok(path.clone())
}
