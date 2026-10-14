//! Protected selection precedes platform work; only completed immutable generations are returned.

mod differences;
mod forms;
mod generation;
mod merge;
mod prepared;
mod queries;
mod source_map;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    AcceptedBase, Child, Error, FileRole, MetadataNode, NativeUuid, Operation, PermissionReason,
    PropertyName, PropertyValue, Section, SourcePath,
    composition::reference_folder,
    export::{Package, validate_composition},
    files::{self, Inventory},
};
use crate::project_core::{
    MetadataKind, native,
    support::{Reason, State},
};

pub use prepared::PreparedSource;
pub use source_map::{
    COMPOSITION_SOURCE_MAP_VERSION, EXTERNAL_SOURCE_MAP_VERSION, FileOrigin, InputAuthority,
    InputFile, MANAGED_FORM_SOURCE_MAP_VERSION, MappedFile, OriginTexts, QUERY_SOURCE_MAP_VERSION,
    QueryOrigin, RELOCATED_SOURCE_MAP_VERSION, SOURCE_MAP_VERSION, SourceLocation, SourceMap,
};

/// Every excluded working change states the performed action rather than silently disappearing.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionAction {
    Restored,
    Excluded,
}

/// Stable coordinates and accepted identity explain a protected operation in either locale.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exclusion {
    pub id: String,
    pub code: String,
    pub path: SourcePath,
    pub coordinate: Vec<String>,
    pub uuid: Option<NativeUuid>,
    pub operation: Operation,
    pub action: ExclusionAction,
    pub state: State,
    pub native_reason: Reason,
    pub reason: PermissionReason,
    pub baseline: String,
}

/// Readers consume a completed generation by its immutable path, with no mutable XML fallback.
#[derive(Clone, Debug, Serialize)]
pub struct ProtectedExport {
    pub generation: String,
    pub xml: PathBuf,
    pub input_revision: String,
    pub baseline: String,
    pub diagnostics: Vec<Exclusion>,
}

/// Strict refusal retains the full explanation for callers without publishing any generation.
#[derive(Clone, Debug, Serialize)]
pub struct ProtectedFailure {
    pub error: Error,
    pub diagnostics: Vec<Exclusion>,
}

impl From<Error> for ProtectedFailure {
    /// Propagate bounded parsing and filesystem failures without a localized core message.
    fn from(error: Error) -> Self {
        Self {
            error,
            diagnostics: Vec::new(),
        }
    }
}

impl std::fmt::Display for ProtectedFailure {
    /// Presentation belongs to the caller; the fallback retains the stable machine code.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(formatter)
    }
}
impl std::error::Error for ProtectedFailure {}

/// Merge permitted working values with accepted composition and publish a completed generation.
///
/// # Errors
/// Returns untrusted base, invalid permitted values, changing inputs, conflicting output or
/// strict exclusion. Existing completed generations and working source bytes remain untouched.
pub fn export_protected(
    source: &Path,
    base: &AcceptedBase,
    output: &Path,
    strict: bool,
) -> Result<ProtectedExport, ProtectedFailure> {
    prepare_protected(source, base, output, strict).map(|prepared| prepared.report)
}

/// Prepare a proof-carrying native generation for a consumer that will verify its build snapshot.
///
/// # Errors
/// Returns the same policy, composition, input-race and publication failures as protected export.
pub fn prepare_protected(
    source: &Path,
    base: &AcceptedBase,
    output: &Path,
    strict: bool,
) -> Result<PreparedSource, ProtectedFailure> {
    base.verify()?;
    let source = fs::canonicalize(source).map_err(|error| files::io(&error))?;
    let working = Inventory::read(&source)?;
    let original_base = base;
    let bound;
    let marker = SourcePath::parse(files::MANIFEST)?;
    let manifest = if working.files.contains_key(&marker) {
        serde_json::from_str::<super::SourceManifest>(&read_working(&source, &working, &marker)?)
            .ok()
    } else {
        None
    };
    let base = if manifest
        .as_ref()
        .is_some_and(|manifest| manifest != &base.package.manifest)
    {
        let candidate = Package::from_snapshot(working.clone(), |path| {
            read_working(&source, &working, path)
        })?;
        bound = base.with_composition(&candidate)?;
        &bound
    } else {
        base
    };
    queries::require_values(&working, base)?;
    let mut diagnostics = Vec::new();
    let selected = merge::sources(&source, &working, base, &mut diagnostics)?;
    validate_composition(&selected.documents)?;
    let (snapshot, origins) = select_files(&working, base, &mut diagnostics);
    diagnostics.sort_by(|left, right| {
        (&left.path, &left.coordinate, &left.id).cmp(&(&right.path, &right.coordinate, &right.id))
    });
    if strict && !diagnostics.is_empty() {
        return Err(ProtectedFailure {
            error: Error::root("source_changes_excluded"),
            diagnostics,
        });
    }
    let query_inputs = queries::load(&source, &working, base, &selected, &origins)?;
    let package = Package {
        manifest: base.package.manifest.clone(),
        snapshot,
        documents: selected.documents,
        forms: selected.forms,
        query_inputs,
        owners: base.package.owners.clone(),
        payloads: base.package.payloads.clone(),
        payload_sources: base.package.payload_sources.clone(),
    };
    super::structural_references::validate_references(
        original_base,
        &package,
        |path| match origins.get(path) {
            Some(Origin::Working) => read_working(&source, &working, path),
            Some(Origin::Accepted) => base.read_file(path),
            None => Err(Error::root("source_file_undeclared")),
        },
    )?;
    let xml = package.native_xml()?;
    let native = package.output_inventory(&xml)?;
    let binding = generation::Binding::new(&working.revision, base.reference(), &native.revision);
    let generation = binding.id()?;
    let source_map = source_map::create(
        &working,
        base,
        &package,
        &native,
        &xml,
        &origins,
        generation.clone(),
    )?;
    let plan = generation::Plan {
        source: &source,
        working: &working,
        base,
        binding: &binding,
        xml: &xml,
        native: &native,
        origins: &origins,
    };
    let destination = plan.publish(output)?;
    Ok(PreparedSource {
        report: ProtectedExport {
            generation,
            xml: destination,
            input_revision: working.revision,
            baseline: base.reference().sha256.clone(),
            diagnostics,
        },
        binding,
        native,
        source_map,
    })
}

/// Validate a proposed update with the same permissions as protected export, without publication.
pub(super) fn update_exclusions(
    working: &Inventory,
    base: &AcceptedBase,
    read: impl FnMut(&SourcePath) -> Result<String, Error>,
) -> Result<Vec<Exclusion>, Error> {
    queries::require_values(working, base)?;
    let mut diagnostics = Vec::new();
    let selected = merge::sources_with_reader(working, base, &mut diagnostics, read)?;
    validate_composition(&selected.documents)?;
    select_files(working, base, &mut diagnostics);
    diagnostics.sort_by(|left, right| {
        (&left.path, &left.coordinate, &left.id).cmp(&(&right.path, &right.coordinate, &right.id))
    });
    Ok(diagnostics)
}

/// Byte origins remain explicit while the full output composition is taken from accepted files.
#[derive(Clone, Copy)]
enum Origin {
    Accepted,
    Working,
}

/// File permissions depend on a declared native owner; policy and service files remain pinned.
fn select_files(
    working: &Inventory,
    base: &AcceptedBase,
    diagnostics: &mut Vec<Exclusion>,
) -> (Inventory, BTreeMap<SourcePath, Origin>) {
    let mut files = base.package.snapshot.files.clone();
    let mut origins = BTreeMap::new();
    let owners: BTreeSet<_> = base
        .package
        .manifest
        .files
        .iter()
        .filter_map(|file| {
            if let FileRole::Descriptor { source, .. } = &file.role {
                Some(source)
            } else {
                None
            }
        })
        .collect();
    for (path, accepted) in &base.package.snapshot.files {
        if owners.contains(path) {
            continue;
        }
        let current = working.files.get(path);
        if current == Some(accepted) {
            origins.insert(path.clone(), Origin::Working);
            continue;
        }
        let native = base.package.payloads.get(path).unwrap_or(path);
        let query_owner = base.query_owner(path);
        let uuid = query_owner
            .cloned()
            .or_else(|| native_file_owner(base, native));
        let operation = if native::is_module_path(native.as_path()) {
            Operation::Module
        } else {
            Operation::Resource
        };
        let native_policy = native.as_str() == "Ext/ParentConfigurations.bin"
            || native.as_str().starts_with("Ext/ParentConfigurations/");
        let binary_module = operation == Operation::Module
            && !path
                .as_path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bsl"));
        let editable = !native_policy
            && !binary_module
            && if operation == Operation::Module {
                base.module_permission(path)
                    .is_ok_and(|permission| permission.allowed)
            } else {
                uuid.as_ref()
                    .is_some_and(|uuid| base.permission(uuid, operation).allowed)
            };
        if let Some(current) = current.filter(|_| editable) {
            files.insert(path.clone(), current.clone());
            origins.insert(path.clone(), Origin::Working);
        } else {
            origins.insert(path.clone(), Origin::Accepted);
            diagnostics.push(differences::exclusion(
                base,
                path,
                Vec::new(),
                uuid,
                if current.is_none() && query_owner.is_none() {
                    Operation::Delete
                } else {
                    operation
                },
                ExclusionAction::Restored,
                "source_file_restored",
            ));
        }
    }
    select_optional_modules(working, base, &mut files, &mut origins, diagnostics);
    record_extras(working, base, &origins, diagnostics);
    let directories = base.package.snapshot.directories.clone();
    let revision = files::revision(&files, &directories);
    (
        Inventory {
            files,
            directories,
            revision,
        },
        origins,
    )
}

/// A known optional root module is a module edit, never arbitrary physical-file creation.
fn select_optional_modules(
    working: &Inventory,
    base: &AcceptedBase,
    files: &mut BTreeMap<SourcePath, super::FileEntry>,
    origins: &mut BTreeMap<SourcePath, Origin>,
    diagnostics: &mut Vec<Exclusion>,
) {
    for file in &base.package.manifest.files {
        let FileRole::OptionalModule { source } = &file.role else {
            continue;
        };
        if base.package.snapshot.files.contains_key(source) {
            continue;
        }
        let Some(current) = working.files.get(source) else {
            continue;
        };
        let decision = base.module_permission(source).ok();
        if decision
            .as_ref()
            .is_some_and(|permission| permission.allowed)
        {
            files.insert(source.clone(), current.clone());
            origins.insert(source.clone(), Origin::Working);
        } else {
            origins.insert(source.clone(), Origin::Accepted);
            diagnostics.push(differences::exclusion(
                base,
                source,
                Vec::new(),
                decision.map(|permission| permission.uuid),
                Operation::Module,
                ExclusionAction::Excluded,
                "source_file_excluded",
            ));
        }
    }
}

/// Reject new physical entries explicitly, with logarithmic directory lookups on large sources.
fn record_extras(
    working: &Inventory,
    base: &AcceptedBase,
    origins: &BTreeMap<SourcePath, Origin>,
    diagnostics: &mut Vec<Exclusion>,
) {
    for path in working.files.keys().filter(|path| {
        !base.package.snapshot.files.contains_key(*path) && !origins.contains_key(*path)
    }) {
        diagnostics.push(differences::exclusion(
            base,
            path,
            Vec::new(),
            None,
            Operation::Create,
            ExclusionAction::Excluded,
            "source_file_excluded",
        ));
    }
    let directories: BTreeSet<_> = base.package.snapshot.directories.iter().collect();
    for path in working
        .directories
        .iter()
        .filter(|path| !directories.contains(path))
    {
        diagnostics.push(differences::exclusion(
            base,
            path,
            Vec::new(),
            None,
            Operation::Create,
            ExclusionAction::Excluded,
            "source_directory_excluded",
        ));
    }
}

/// Editor plans and protected export share the same accepted text-module ownership and permission.
pub(super) fn module_permission(
    base: &AcceptedBase,
    path: &SourcePath,
) -> Result<super::Permission, Error> {
    let native = base
        .package
        .payloads
        .get(path)
        .ok_or_else(|| Error::root("source_edit_module_unsupported"))?;
    if !native::is_module_path(native.as_path())
        || !native
            .as_path()
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bsl"))
        || (!base.package.snapshot.files.contains_key(path)
            && base.package.manifest.optional_module(path).is_none())
    {
        return Err(Error::root("source_edit_module_unsupported"));
    }
    let uuid =
        native_file_owner(base, native).ok_or_else(|| Error::root("source_edit_owner_unknown"))?;
    Ok(base.permission(&uuid, Operation::Module))
}

/// Resolve the longest declared native owner before considering a configuration root payload.
pub(super) fn native_file_owner(base: &AcceptedBase, path: &SourcePath) -> Option<NativeUuid> {
    native_package_file_owner(&base.package, path)
}

/// Native ownership lookup is independent of which authority supplies permission decisions.
pub(super) fn native_package_file_owner(
    package: &Package,
    path: &SourcePath,
) -> Option<NativeUuid> {
    for (end, _) in path.as_str().match_indices('/').rev() {
        let descriptor = SourcePath::parse(&format!("{}.xml", &path.as_str()[..end])).ok()?;
        if let Some(document) = package.documents.get(&descriptor) {
            return inline_owner(&document.descriptor.object, &path.as_str()[end + 1..]);
        }
    }
    if path.as_str().starts_with("Ext/") {
        return package
            .documents
            .get(&package.manifest.root_path().ok()?)
            .map(|document| document.descriptor.object.uuid.clone());
    }
    None
}

/// A nested inline descriptor owns its payload even though its physical YAML belongs to a parent.
fn inline_owner(mut node: &MetadataNode, mut remaining: &str) -> Option<NativeUuid> {
    loop {
        let mut parts = remaining.splitn(3, '/');
        let folder = parts.next()?;
        if !MetadataKind::ALL
            .iter()
            .any(|kind| reference_folder(*kind) == Some(folder))
        {
            return Some(node.uuid.clone());
        }
        let name = parts.next()?;
        let tail = parts.next()?;
        node = node.sections.iter().find_map(|section| {
            let Section::Children(children) = section else { return None; };
            children.iter().find_map(|child| {
                let Child::Inline {object} = child else {return None;};
                (reference_folder(object.kind) == Some(folder)
                    && matches!(object.property(PropertyName::Name),Some(PropertyValue::Text(value)) if value == name))
                    .then_some(object.as_ref())
            })
        })?;
        remaining = tail;
    }
}

/// A text read must match the already captured input bytes, including changes between reads.
fn read_working(root: &Path, inventory: &Inventory, path: &SourcePath) -> Result<String, Error> {
    let entry = inventory
        .files
        .get(path)
        .ok_or_else(|| Error::root("source_file_missing"))?;
    let text = files::read_text(&root.join(path.as_path()))?;
    if text.len() as u64 != entry.bytes
        || files::hex(&Sha256::digest(text.as_bytes())) != entry.sha256
    {
        return Err(Error::root("source_input_changed").in_field(path.as_str()));
    }
    Ok(text)
}
