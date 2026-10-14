//! Three-way import compares confirmed ancestry with working and incoming source bytes.

mod descriptors;
mod project;
mod publication;
mod selection;

pub(in crate::project_core::source_format) use project::stage_project_import_ancestor;
pub(in crate::project_core::source_format) use project::verified_project_import_history;
pub use project::verify_project_import_history;
pub use project::{
    NativeProjectSourceImport, PROJECT_SOURCE_IMPORT_RECEIPT, ProjectSourceImportPlan,
    ProjectSourceImportRef, SourceImportRecovery, prepare_project_native_source_import,
    prepare_project_source_import, project_import_ancestor, recover_project_source_import,
};
pub use project::{
    PROJECT_SOURCE_IMPORT_DIRECTORY, initialize_project_import_ancestor,
    open_initial_project_import_ancestor,
};

#[cfg(test)]
mod tests;

use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use super::{ImportAncestor, ImportAncestorRef, storage};
use crate::project_core::source_format::{
    AcceptedBase, AcceptedRef, Error, Exclusion, FileEntry, NativeUuid, Operation, Permission,
    SourcePath,
    export::Package,
    file_identity,
    files::{self, Inventory},
    protected,
};

/// Import conflicts are source decisions, never a request to approve incoming supplier rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceImportConflictKind {
    BothModified,
    DeletedModified,
    ModifiedDeleted,
    SupportUpdateRequired,
    MappingUnsupported,
    PermissionDenied,
    UnmappedFile,
    UnmappedDirectory,
}

/// Exact participating paths and policy coordinates make every refused decision reviewable.
#[derive(Clone, Debug, Serialize)]
pub struct SourceImportConflict {
    pub kind: SourceImportConflictKind,
    pub path: SourcePath,
    pub uuid: Option<NativeUuid>,
    pub exclusion: Option<Exclusion>,
    /// Descriptor coordinates identify a semantic collision; file-only conflicts omit this field.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub coordinate: Vec<String>,
}

/// Metadata identity binds a compared file independently of its current spelling or directory.
#[derive(Clone, Debug, Serialize)]
pub struct SourceImportChange {
    pub identity: String,
    pub ancestor: Option<FileEntry>,
    pub incoming: Option<FileEntry>,
    pub working: Option<FileEntry>,
    pub selected: Option<FileEntry>,
}

/// Selected bytes retain their original path so publication can verify the precise input entry.
enum SelectedFile {
    Working(FileEntry),
    Incoming(FileEntry),
    Merged { entry: FileEntry, text: String },
}

/// A captured input is rechecked without reparsing a large immutable package on every write.
struct Input {
    root: PathBuf,
    inventory: Inventory,
}

impl Input {
    /// A cancelled inventory cannot be accepted as an unchanged participant.
    fn verify_cancellable(&self, cancelled: &dyn Fn() -> bool) -> Result<(), Error> {
        if Inventory::read_cancellable(&self.root, cancelled)? != self.inventory {
            return Err(Error::root("source_input_changed"));
        }
        Ok(())
    }
}

/// An import never replaces the separately approved baseline or silently selects conflict bytes.
pub struct SourceImportPlan {
    working: Input,
    ancestor: Input,
    incoming: Input,
    approved: Input,
    native: Option<Input>,
    ancestor_ref: ImportAncestorRef,
    incoming_ref: ImportAncestorRef,
    approved_ref: AcceptedRef,
    output: Inventory,
    selected: BTreeMap<SourcePath, SelectedFile>,
    changes: Vec<SourceImportChange>,
    conflicts: Vec<SourceImportConflict>,
    incoming_data_unchanged: bool,
    support_permissions: Vec<Permission>,
}

/// Prepare a source-only import from an explicitly confirmed ancestor under current pinned rights.
///
/// UUID-preserving declared-object renames may change the reverse map under the original policy.
/// Whole-owner deletion preserves existing rights; new ownership and divergent maps remain conflicts.
///
/// # Errors
/// Returns a wrong root/type/layout, invalid working declarations, stale input or invalid source.
pub fn prepare_source_import(
    source: &Path,
    ancestor: &ImportAncestor,
    incoming: &ImportAncestor,
    approved: &AcceptedBase,
) -> Result<SourceImportPlan, Error> {
    ancestor.verify()?;
    incoming.verify()?;
    let approved_storage = approved.verified_inventory()?;
    compatible(&approved.package, &ancestor.package)?;
    compatible(&approved.package, &incoming.package)?;
    let source = source.canonicalize().map_err(|error| files::io(&error))?;
    let working = Inventory::read(&source)?;
    let manifest = serde_json::from_str::<super::super::SourceManifest>(&files::read_text(
        &source.join(files::MANIFEST),
    )?)
    .map_err(|_| Error::root("source_schema_invalid"))?;
    let working_package;
    let current = if manifest == approved.package.manifest {
        &approved.package
    } else {
        working_package = Package::from_snapshot(working.clone(), |path| {
            files::read_text(&source.join(path.as_path()))
        })?;
        approved.with_composition(&working_package, |path| {
            files::read_text(&source.join(path.as_path()))
        })?;
        &working_package
    };
    if approved.overlaps(&source)
        || source.starts_with(&ancestor.root)
        || ancestor.root.starts_with(&source)
        || source.starts_with(&incoming.root)
        || incoming.root.starts_with(&source)
    {
        return Err(Error::root("source_tree_overlap"));
    }
    let chosen = selection::select(
        &working,
        &ancestor.package,
        &incoming.package,
        &approved.package,
        current,
    )?;
    let mut plan = SourceImportPlan {
        working: Input {
            root: source,
            inventory: working,
        },
        ancestor: Input {
            root: ancestor.root.clone(),
            inventory: storage::clone_inventory(&ancestor.storage),
        },
        incoming: Input {
            root: incoming.root.clone(),
            inventory: storage::clone_inventory(&incoming.storage),
        },
        approved: Input {
            root: approved.storage_root().to_owned(),
            inventory: approved_storage,
        },
        native: None,
        ancestor_ref: ancestor.reference.clone(),
        incoming_ref: incoming.reference.clone(),
        approved_ref: approved.reference().clone(),
        output: chosen.output,
        selected: chosen.selected,
        changes: chosen.changes,
        conflicts: chosen.conflicts,
        incoming_data_unchanged: chosen.incoming_data_unchanged,
        support_permissions: Vec::new(),
    };
    descriptors::resolve(&mut plan)?;
    plan.validate_selected(approved)?;
    plan.verify()?;
    Ok(plan)
}

impl SourceImportPlan {
    /// Validate complete selected declarations and permissions before allowing publication.
    fn validate_selected(&mut self, approved: &AcceptedBase) -> Result<(), Error> {
        self.capture_support(approved);
        if self.conflicts.is_empty() {
            let package = Package::from_snapshot(storage::clone_inventory(&self.output), |path| {
                self.read(path)
            })?;
            let bound;
            let authority = if package.manifest == approved.package.manifest {
                approved
            } else {
                bound = match approved.with_composition(&package, |path| self.read(path)) {
                    Ok(bound) => bound,
                    Err(error)
                        if matches!(
                            error.code,
                            "source_import_mapping_unsupported" | "source_import_structure_denied"
                        ) =>
                    {
                        self.conflicts.push(SourceImportConflict {
                            kind: if error.code == "source_import_structure_denied" {
                                SourceImportConflictKind::PermissionDenied
                            } else {
                                SourceImportConflictKind::MappingUnsupported
                            },
                            path: SourcePath::parse(files::MANIFEST)?,
                            uuid: error
                                .location
                                .first()
                                .and_then(|field| NativeUuid::parse(field).ok()),
                            exclusion: None,
                            coordinate: Vec::new(),
                        });
                        self.verify()?;
                        return Ok(());
                    }
                    Err(error) => return Err(error),
                };
                &bound
            };
            self.capture_support(authority);
            for exclusion in
                protected::update_exclusions(&self.output, authority, |path| self.read(path))?
            {
                self.conflicts.push(SourceImportConflict {
                    kind: SourceImportConflictKind::PermissionDenied,
                    path: exclusion.path.clone(),
                    uuid: exclusion.uuid.clone(),
                    exclusion: Some(exclusion),
                    coordinate: Vec::new(),
                });
            }
            if self.conflicts.is_empty() {
                super::super::structural_references::validate_references(
                    approved,
                    &package,
                    |path| self.read(path),
                )?;
                package.native_xml()?;
            }
        }
        Ok(())
    }

    /// Changed owners expose the validated support state; unresolved new ownership remains unknown.
    fn capture_support(&mut self, authority: &AcceptedBase) {
        let owners = self
            .changes
            .iter()
            .filter_map(|change| file_identity::uuid(&change.identity))
            .map(|uuid| (uuid.key(), uuid))
            .collect::<BTreeMap<_, _>>();
        self.support_permissions = owners
            .into_values()
            .map(|uuid| authority.permission(&uuid, Operation::Resource))
            .collect();
    }

    /// These Resource decisions describe owner support, not permission for every individual edit.
    #[must_use]
    pub fn support_permissions(&self) -> &[Permission] {
        &self.support_permissions
    }

    /// Return current support approval; import publication never advances this reference.
    #[must_use]
    pub const fn approved_reference(&self) -> &AcceptedRef {
        &self.approved_ref
    }

    /// Return the explicitly confirmed common ancestry used by this comparison.
    #[must_use]
    pub const fn ancestor_reference(&self) -> &ImportAncestorRef {
        &self.ancestor_ref
    }

    /// Return the exact captured incoming data; project publication may retain equivalent ancestry.
    #[must_use]
    pub const fn incoming_reference(&self) -> &ImportAncestorRef {
        &self.incoming_ref
    }

    /// Show byte-exact changes, including local data retained against an unchanged incoming input.
    #[must_use]
    pub fn changes(&self) -> &[SourceImportChange] {
        &self.changes
    }

    /// A nonempty list blocks source publication without writing any project or baseline files.
    #[must_use]
    pub fn conflicts(&self) -> &[SourceImportConflict] {
        &self.conflicts
    }

    /// Bind application to all current source bytes rather than file timestamps.
    #[must_use]
    pub fn working_revision(&self) -> &str {
        &self.working.inventory.revision
    }

    /// The output revision includes every selected path, byte fingerprint and empty directory.
    #[must_use]
    pub fn output_revision(&self) -> &str {
        &self.output.revision
    }

    /// Recheck all four participating authorities, keeping support independent of source ancestry.
    ///
    /// # Errors
    /// Refuses source, ancestry, incoming data or approved-baseline changes after preparation.
    pub fn verify(&self) -> Result<(), Error> {
        self.verify_cancellable(&|| false)
    }

    /// Stream all participating fingerprints with an explicit caller cancellation boundary.
    ///
    /// # Errors
    /// Returns stale input, unsupported storage, I/O or cancellation failures.
    pub fn verify_cancellable(&self, cancelled: &dyn Fn() -> bool) -> Result<(), Error> {
        for input in [
            &self.working,
            &self.ancestor,
            &self.incoming,
            &self.approved,
        ] {
            input.verify_cancellable(cancelled)?;
        }
        if let Some(native) = &self.native {
            native.verify_cancellable(cancelled)?;
        }
        Ok(())
    }

    /// Read only selected text consumed by validation; opaque resources stay streamed.
    fn read(&self, path: &SourcePath) -> Result<String, Error> {
        match self
            .selected
            .get(path)
            .ok_or_else(|| Error::root("source_file_missing"))?
        {
            SelectedFile::Working(entry) => {
                files::read_text(&self.working.root.join(entry.path.as_path()))
            }
            SelectedFile::Incoming(entry) => files::read_text(
                &self
                    .incoming
                    .root
                    .join(storage::blob_path(entry)?.as_path()),
            ),
            SelectedFile::Merged { text, .. } => Ok(text.clone()),
        }
    }
}

/// Full parsed native type distinguishes CF from CFE even when UUID and profile spelling agree.
fn compatible(approved: &Package, other: &Package) -> Result<(), Error> {
    let root = |package: &Package| {
        let descriptor = &package
            .documents
            .get(&package.manifest.root_path()?)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?
            .descriptor;
        Ok::<_, Error>((
            descriptor.object.uuid.key(),
            super::super::root::project_type(descriptor)?,
        ))
    };
    let (approved_uuid, approved_type) = root(approved)?;
    let (other_uuid, other_type) = root(other)?;
    if approved_uuid != other_uuid {
        return Err(Error::root("source_identity_mismatch"));
    }
    if approved_type != other_type
        || approved.manifest.schema_version != other.manifest.schema_version
        || approved.manifest.profile != other.manifest.profile
    {
        return Err(Error::root("source_import_profile_mismatch"));
    }
    Ok(())
}
