//! Whole-text YAML admission reproduces only independently authorized property splices.

use std::path::{Path, PathBuf};

use crate::project_core::publish;

use super::super::baseline::source_binding::SourceBinding;
use super::{
    AcceptedBase, AcceptedRef, Child, Error, MetadataNode, NativeUuid, ObjectSource, Operation,
    Property, PropertyName, PropertyValue, Section, SourceEditPlan, SourcePath, edit_error,
    permission, prepare_property_edit_text, safe_root,
};

/// A complete candidate is published once after every changed property is authorized.
///
/// This first whole-text subset accepts existing typed property values only. Structural,
/// form-body and independent trivia changes require their own lossless authority proof.
#[derive(Debug)]
pub struct SourceYamlEditPlan {
    publication: Publication,
}

/// A no-op has no property claim and never borrows an unrelated object's write permission.
#[derive(Debug)]
enum Publication {
    Changed {
        plan: SourceEditPlan,
        claims: Vec<(NativeUuid, PropertyName)>,
    },
    Unchanged {
        root: PathBuf,
        path: SourcePath,
        baseline: AcceptedRef,
        original: String,
        source_binding: Option<SourceBinding>,
    },
}

impl SourceYamlEditPlan {
    /// Return the independently declared physical authoring path.
    #[must_use]
    pub const fn path(&self) -> &SourcePath {
        match &self.publication {
            Publication::Changed { plan, .. } => plan.path(),
            Publication::Unchanged { path, .. } => path,
        }
    }

    /// Return the exact admitted candidate, including its preserved lexical spelling.
    #[must_use]
    pub fn updated(&self) -> &str {
        match &self.publication {
            Publication::Changed { plan, .. } => plan.updated(),
            Publication::Unchanged { original, .. } => original,
        }
    }

    /// Exact no-ops retain the source timestamp and create no temporary replacement.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(&self.publication, Publication::Unchanged { .. })
    }

    /// Recheck all claims, the independent baseline and exact source before one publication.
    ///
    /// This shares the existing atomic replacement boundary; it does not provide an OS-level
    /// compare-and-swap against a noncooperating writer racing the final rename.
    ///
    /// # Errors
    /// Returns stale policy/binding, denied permission, exact-byte conflict or unsafe I/O.
    pub fn publish(&self, base: &AcceptedBase) -> Result<(), Error> {
        self.publish_cancellable(base, &|| false)
    }

    /// Cancel before commit while retaining the project lock across every property claim check.
    ///
    /// No cancellation check follows a successful filesystem commit.
    ///
    /// # Errors
    /// Returns publication failures or `source_read_cancelled` before a committed replacement.
    pub fn publish_cancellable(
        &self,
        base: &AcceptedBase,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Error> {
        super::files::check_cancelled(cancelled)?;
        match &self.publication {
            Publication::Changed { plan, claims } => {
                let _write_lock = plan.acquire_write_lock()?;
                if base.reference() != &plan.baseline {
                    return Err(Error::root("source_edit_baseline_changed"));
                }
                for (uuid, property) in claims {
                    permission(base, uuid, Operation::Property(*property))?;
                }
                plan.publish_locked(base, cancelled)
            }
            Publication::Unchanged {
                root,
                path,
                baseline,
                original,
                source_binding,
            } => {
                let _write_lock = source_binding
                    .as_ref()
                    .map_or(Ok(None), SourceBinding::acquire_write_lock)?;
                if base.reference() != baseline {
                    return Err(Error::root("source_edit_baseline_changed"));
                }
                base.verify()?;
                if let Some(binding) = source_binding {
                    binding.verify(root, cancelled)?;
                }
                publish::publish_snapshot_cancellable(
                    root,
                    path.as_path(),
                    original,
                    original,
                    cancelled,
                )
                .map_err(edit_error)
                .map_err(|error| error.in_field(path.as_str()))
            }
        }
    }
}

/// Admit complete YAML text only when existing authorized property edits reproduce it exactly.
///
/// Both texts must be valid current owner documents. Every non-value model field remains
/// unchanged, and bytes outside the existing property CST splices remain identical. A refused
/// candidate is never partially restored or written; the caller keeps its editable draft.
///
/// # Errors
/// Returns undeclared owner, stale bytes, invalid YAML, denied/unsupported change or I/O errors.
pub fn prepare_yaml_edit(
    root: &Path,
    base: &AcceptedBase,
    path: &SourcePath,
    original: &str,
    updated: &str,
) -> Result<SourceYamlEditPlan, Error> {
    let accepted_paths =
        base.package.owners.get(path).ok_or_else(|| {
            Error::root("source_edit_operation_unsupported").in_field(path.as_str())
        })?;
    let root = safe_root(root, path)?;
    publish::publish_snapshot(&root, path.as_path(), original, original)
        .map_err(edit_error)
        .map_err(|error| error.in_field(path.as_str()))?;
    let before = ObjectSource::from_yaml(original)?;
    if !std::iter::once(&before.document)
        .chain(&before.nested)
        .map(|document| &document.path)
        .eq(accepted_paths.iter())
    {
        return Err(Error::root("source_edit_structure_changed"));
    }
    let after = ObjectSource::from_yaml(updated)?;
    let changes = property_changes(&before, &after)?;
    let mut oracle = original.to_owned();
    let mut claims = Vec::with_capacity(changes.len());
    for change in changes {
        let preview =
            prepare_property_edit_text(base, &change.uuid, change.property, change.value, &oracle)?;
        if preview.path() != path {
            return Err(Error::root("source_edit_structure_changed"));
        }
        preview.updated().clone_into(&mut oracle);
        claims.push((change.uuid, change.property));
    }
    if oracle != updated {
        return Err(Error::root("source_edit_value_unsupported"));
    }
    let publication = if let Some((uuid, property)) = claims.first() {
        Publication::Changed {
            plan: SourceEditPlan {
                root,
                path: path.clone(),
                uuid: uuid.clone(),
                operation: Operation::Property(*property),
                baseline: base.reference().clone(),
                original: Some(original.to_owned()),
                updated: updated.to_owned(),
                source_binding: base.source_binding(),
            },
            claims,
        }
    } else {
        Publication::Unchanged {
            root,
            path: path.clone(),
            baseline: base.reference().clone(),
            original: original.to_owned(),
            source_binding: base.source_binding(),
        }
    };
    Ok(SourceYamlEditPlan { publication })
}

/// Deltas carry saved identities; only the existing preview planner can authorize them.
struct PropertyChange {
    uuid: NativeUuid,
    property: PropertyName,
    value: PropertyValue,
}

/// Equality after value substitution rejects all unclassified structural and native changes.
fn property_changes(
    before: &ObjectSource,
    after: &ObjectSource,
) -> Result<Vec<PropertyChange>, Error> {
    let mut projected = before.clone();
    let mut changes = Vec::new();
    for (before, after) in std::iter::once(&mut projected.document)
        .chain(&mut projected.nested)
        .zip(std::iter::once(&after.document).chain(&after.nested))
    {
        collect_node_changes(
            &mut before.descriptor.object,
            &after.descriptor.object,
            &mut changes,
        );
    }
    if projected != *after {
        return Err(Error::root("source_edit_structure_changed"));
    }
    Ok(changes)
}

/// Only paired typed slots and inline coordinates participate; opaque fragments stay opaque.
fn collect_node_changes(
    projected: &mut MetadataNode,
    candidate: &MetadataNode,
    changes: &mut Vec<PropertyChange>,
) {
    for (before, after) in projected.sections.iter_mut().zip(&candidate.sections) {
        match (before, after) {
            (Section::Properties(before), Section::Properties(after)) => {
                for (before, after) in before.iter_mut().zip(after) {
                    if let (
                        Property::Typed { name, value },
                        Property::Typed {
                            name: next_name,
                            value: next_value,
                        },
                    ) = (before, after)
                        && *name == *next_name
                        && *value != *next_value
                    {
                        changes.push(PropertyChange {
                            uuid: projected.uuid.clone(),
                            property: *name,
                            value: next_value.clone(),
                        });
                        value.clone_from(next_value);
                    }
                }
            }
            (Section::Children(before), Section::Children(after)) => {
                for (before, after) in before.iter_mut().zip(after) {
                    if let (Child::Inline { object: before }, Child::Inline { object: after }) =
                        (before, after)
                    {
                        collect_node_changes(before, after, changes);
                    }
                }
            }
            _ => {}
        }
    }
}
