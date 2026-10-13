//! One accepted form owner anchors property/layout previews and shared conflict-checked publication.

use std::path::Path;

use crate::project_core::publish;
use crate::project_core::source_format::{
    AcceptedBase, AcceptedRef, Error, FileRole, FormId, FormLayout, FormPropertyName,
    FormPropertyValue, ManagedFormSource, NativeUuid, ObjectSource, Operation, SourcePath, files,
};

use super::{SourceEditPlan, cst, edit_error, permission, safe_root, target};

#[cfg(test)]
mod tests;

/// Existing property values and ordered edges have no authority to replace identities or native bindings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormEdit {
    Property {
        element: Option<FormId>,
        property: FormPropertyName,
        value: FormPropertyValue,
    },
    Layout(FormLayout),
}

/// An editor-owned preview carries accepted identity but cannot publish or authorize a filesystem write.
#[derive(Debug)]
pub struct FormEditPreview {
    path: SourcePath,
    baseline: AcceptedRef,
    updated: String,
    unchanged: bool,
}

impl FormEditPreview {
    /// Return the independently accepted owner path rather than an editable YAML location.
    #[must_use]
    pub const fn path(&self) -> &SourcePath {
        &self.path
    }

    /// Bind the preview to the caller's independently verified accepted baseline.
    #[must_use]
    pub const fn baseline(&self) -> &AcceptedRef {
        &self.baseline
    }

    /// Return exact current text with only the validated property or layout collections changed.
    #[must_use]
    pub fn updated(&self) -> &str {
        &self.updated
    }

    /// An equal logical value retains every original byte and needs no editor change.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.unchanged
    }
}

/// Prepare one managed form edit without reserializing its metadata descriptor or native stencils.
///
/// # Errors
/// Returns policy, accepted identity/binding, invalid value/graph, unsafe path or I/O failures.
pub fn prepare_form_edit(
    root: &Path,
    base: &AcceptedBase,
    uuid: &NativeUuid,
    edit: FormEdit,
) -> Result<SourceEditPlan, Error> {
    permission(base, uuid, Operation::Resource)?;
    let target = target(base, uuid)?;
    let root = safe_root(root, &target.owner)?;
    let original = files::read_text(&root.join(target.owner.as_path()))?;
    let preview = prepare_form_edit_text(base, uuid, edit, &original)?;
    publish::publish_snapshot(&root, target.owner.as_path(), &original, &original)
        .map_err(edit_error)?;
    Ok(SourceEditPlan {
        root,
        path: preview.path,
        uuid: uuid.clone(),
        operation: Operation::Resource,
        baseline: preview.baseline,
        original,
        updated: preview.updated,
    })
}

/// Prepare a preview over exact editor bytes; the caller checks its document version and baseline pin.
///
/// This does not read source files, reverify mutable baseline storage or grant publication authority.
///
/// # Errors
/// Returns restricted/unknown identity, unsupported body, substituted bindings or invalid value/layout.
pub fn prepare_form_edit_text(
    base: &AcceptedBase,
    uuid: &NativeUuid,
    edit: FormEdit,
    original: &str,
) -> Result<FormEditPreview, Error> {
    permission(base, uuid, Operation::Resource)?;
    let target = target(base, uuid)?;
    let dialog = base
        .package
        .manifest
        .files
        .iter()
        .find_map(|file| {
            if let FileRole::ManagedForm { source, descriptor } = &file.role
                && source == &target.owner
                && descriptor == &target.native
                && target.inline.is_empty()
            {
                Some(&file.path)
            } else {
                None
            }
        })
        .ok_or_else(|| Error::root("source_edit_operation_unsupported"))?;
    let accepted = base
        .package
        .forms
        .get(dialog)
        .ok_or_else(|| Error::root("source_baseline_corrupt"))?;
    let mut object = ObjectSource::from_yaml(original)?;
    anchor(base, &target, &object)?;
    let body = object
        .form
        .as_mut()
        .ok_or_else(|| Error::root("source_edit_structure_changed"))?;
    if body.schema_version != accepted.schema_version {
        return Err(Error::root("source_edit_structure_changed"));
    }
    body.validate_change(accepted)?;
    let updated = apply(original, body, accepted, edit)?;
    if ObjectSource::from_yaml(&updated)? != object {
        return Err(Error::root("source_edit_value_unsupported"));
    }
    Ok(FormEditPreview {
        path: target.owner,
        baseline: base.reference().clone(),
        unchanged: original == updated,
        updated,
    })
}

/// A full metadata UUID and native descriptor anchor the dialog independently of decimal UI scope.
fn anchor(base: &AcceptedBase, target: &super::Target, object: &ObjectSource) -> Result<(), Error> {
    let paths = std::iter::once(&object.document)
        .chain(&object.nested)
        .map(|part| &part.path);
    let accepted_paths = base
        .package
        .owners
        .get(&target.owner)
        .ok_or_else(|| Error::root("source_descriptor_missing"))?;
    let accepted = base
        .package
        .documents
        .get(&target.native)
        .ok_or_else(|| Error::root("source_descriptor_missing"))?;
    let current = &object.document.descriptor.object;
    let previous = &accepted.descriptor.object;
    if object.document.path != target.native
        || !paths.eq(accepted_paths.iter())
        || current.uuid != previous.uuid
        || current.kind != previous.kind
        || current.property(super::PropertyName::Name)
            != previous.property(super::PropertyName::Name)
    {
        return Err(Error::root("source_edit_structure_changed"));
    }
    Ok(())
}

/// Validate the complete candidate before splicing any current bytes, including equal-value requests.
fn apply(
    original: &str,
    body: &mut ManagedFormSource,
    accepted: &ManagedFormSource,
    edit: FormEdit,
) -> Result<String, Error> {
    match edit {
        FormEdit::Property {
            element,
            property,
            value,
        } => {
            let properties = if let Some(id) = &element {
                &mut body
                    .elements
                    .get_mut(id)
                    .ok_or_else(|| Error::root("source_edit_object_unknown"))?
                    .properties
            } else {
                &mut body.properties
            };
            let previous = properties
                .get_mut(&property)
                .ok_or_else(|| Error::root("source_edit_property_missing"))?;
            let previous = std::mem::replace(previous, value.clone());
            body.validate_change(accepted)?;
            if previous == value {
                return Ok(original.into());
            }
            cst::form_property(original, element.as_ref(), property, &previous, &value)
        }
        FormEdit::Layout(layout) => {
            let previous = std::mem::replace(&mut body.layout, layout);
            body.validate_change(accepted)?;
            if previous == body.layout {
                return Ok(original.into());
            }
            cst::form_layout(original, &previous, &body.layout)
        }
    }
}
