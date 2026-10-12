//! Accepted identities select local YAML edits; working UUIDs cannot grant permission.

mod cst;

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::project_core::publish::{self, PublishError};

use super::{
    AcceptedBase, AcceptedRef, Child, Error, MetadataNode, NativeUuid, ObjectSource, Operation,
    PermissionReason, Property, PropertyName, PropertyValue, Section, SourcePath, document_paths,
    files,
};

/// A prepared single-file edit retains exact bytes for preview and conflict detection.
#[derive(Debug)]
pub struct SourceEditPlan {
    root: PathBuf,
    path: SourcePath,
    uuid: NativeUuid,
    property: PropertyName,
    baseline: AcceptedRef,
    original: String,
    updated: String,
}

/// A lossless preview over caller-owned current text; it has no filesystem publication authority.
#[derive(Debug)]
pub struct SourcePropertyEdit {
    path: SourcePath,
    baseline: AcceptedRef,
    updated: String,
    unchanged: bool,
}

impl SourcePropertyEdit {
    /// Return the accepted owner's path, never a path selected by editable YAML.
    #[must_use]
    pub const fn path(&self) -> &SourcePath {
        &self.path
    }

    /// Bind the preview's permission decision to the caller's independently verified base.
    #[must_use]
    pub const fn baseline(&self) -> &AcceptedRef {
        &self.baseline
    }

    /// Return the validated current text with only the selected existing property changed.
    #[must_use]
    pub fn updated(&self) -> &str {
        &self.updated
    }

    /// An equal logical value preserves every original byte, including scalar spelling.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.unchanged
    }
}

impl SourceEditPlan {
    /// Return the accepted owner's relative source path.
    #[must_use]
    pub const fn path(&self) -> &SourcePath {
        &self.path
    }

    /// Preview the validated source without publishing it.
    #[must_use]
    pub fn updated(&self) -> &str {
        &self.updated
    }

    /// An unchanged value retains the original spelling and produces no write.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.original == self.updated
    }

    /// Recheck the independent base and exact source bytes before atomic replacement.
    ///
    /// The final comparison detects prior writes, but is not an OS-level compare-and-swap
    /// against a noncooperating writer racing the subsequent rename.
    ///
    /// # Errors
    /// Returns a policy/base change, conflict, unsafe path, read-only file or I/O failure.
    pub fn publish(&self, base: &AcceptedBase) -> Result<(), Error> {
        if base.reference() != &self.baseline {
            return Err(Error::root("source_edit_baseline_changed"));
        }
        permission(base, &self.uuid, self.property)?;
        base.verify()?;
        publish::publish_snapshot(
            &self.root,
            self.path.as_path(),
            &self.original,
            &self.updated,
        )
        .map_err(edit_error)
        .map_err(|error| error.in_field(self.path.as_str()))
    }
}

/// Prepare one existing typed property without reserializing the owner's YAML document.
///
/// Identity, native kind and Name must still match the accepted ancestry. Structural editing
/// and changes to localized language composition have separate, unsupported operation scopes.
///
/// # Errors
/// Returns restricted/unknown identity, unsupported operation, invalid YAML/value or I/O errors.
pub fn prepare_property_edit(
    root: &Path,
    base: &AcceptedBase,
    uuid: &NativeUuid,
    property: PropertyName,
    value: PropertyValue,
) -> Result<SourceEditPlan, Error> {
    permission(base, uuid, property)?;
    let target = target(base, uuid)?;
    let root = safe_root(root, &target.owner)?;
    let original = files::read_text(&root.join(target.owner.as_path()))?;
    let updated = property_text(base, &target, property, value, &original)?;
    // Reuse the publication preflight without writing; do not preview a stale or unsafe source.
    publish::publish_snapshot(&root, target.owner.as_path(), &original, &original)
        .map_err(edit_error)?;
    Ok(SourceEditPlan {
        root,
        path: target.owner,
        uuid: uuid.clone(),
        property,
        baseline: base.reference().clone(),
        original,
        updated,
    })
}

/// Prepare a property preview over exact editor bytes without reading or writing a source file.
///
/// The caller validates its document version/hash and supplies an independently verified base.
/// This does not recheck mutable baseline storage or authorize publication; saved-file editing
/// must still use `prepare_property_edit` and its conflict-checked `publish` method.
///
/// # Errors
/// Returns the same policy, identity, structure and value failures as saved-file preparation.
pub fn prepare_property_edit_text(
    base: &AcceptedBase,
    uuid: &NativeUuid,
    property: PropertyName,
    value: PropertyValue,
    original: &str,
) -> Result<SourcePropertyEdit, Error> {
    permission(base, uuid, property)?;
    let target = target(base, uuid)?;
    let updated = property_text(base, &target, property, value, original)?;
    Ok(SourcePropertyEdit {
        path: target.owner,
        baseline: base.reference().clone(),
        unchanged: original == updated,
        updated,
    })
}

/// Saved and editor consumers share accepted ancestry, CST replacement and whole-model validation.
fn property_text(
    base: &AcceptedBase,
    target: &Target,
    property: PropertyName,
    value: PropertyValue,
    original: &str,
) -> Result<String, Error> {
    let mut object = ObjectSource::from_yaml(original)?;
    let paths = std::iter::once(&object.document)
        .chain(&object.nested)
        .map(|part| &part.path);
    let accepted_paths = base
        .package
        .owners
        .get(&target.owner)
        .ok_or_else(|| Error::root("source_descriptor_missing"))?;
    if !paths.eq(accepted_paths.iter()) {
        return Err(Error::root("source_edit_structure_changed"));
    }
    let (document, mut steps) = if object.document.path == target.native {
        (&mut object.document, vec![cst::Step::Key("document")])
    } else {
        let index = object
            .nested
            .iter()
            .position(|part| part.path == target.native)
            .ok_or_else(|| Error::root("source_edit_structure_changed"))?;
        (
            &mut object.nested[index],
            vec![cst::Step::Key("nested"), cst::Step::Index(index)],
        )
    };
    let accepted = base
        .package
        .documents
        .get(&target.native)
        .ok_or_else(|| Error::root("source_descriptor_missing"))?;
    steps.extend([cst::Step::Key("descriptor"), cst::Step::Key("object")]);
    let schema = document.descriptor.schema_version;
    let node = anchored_node(
        &mut document.descriptor.object,
        &accepted.descriptor.object,
        &target.inline,
        &mut steps,
        property,
        schema,
    )?;
    let candidate = value.clone();
    let previous = change_property(node, property, value, &mut steps, schema)?;
    object.validate()?;
    if matches!(
        previous,
        PropertyValue::Reference(_)
            | PropertyValue::ReferenceList(_)
            | PropertyValue::EnumFillValue(_)
            | PropertyValue::TypeDescription(_)
    ) || matches!(
        candidate,
        PropertyValue::Reference(_)
            | PropertyValue::ReferenceList(_)
            | PropertyValue::EnumFillValue(_)
            | PropertyValue::TypeDescription(_)
    ) {
        for document in std::iter::once(&object.document).chain(&object.nested) {
            super::targets::validate_edit(&base.package.documents, &document.descriptor.object)?;
        }
    }
    let updated = if previous == candidate {
        original.to_owned()
    } else {
        let updated = if schema == super::READABLE_SCHEMA_VERSION {
            cst::replace_direct(original, &steps, &previous, &candidate)?
        } else {
            cst::replace(original, &steps, &previous, &candidate)?
        };
        if ObjectSource::from_yaml(&updated)? != object {
            return Err(Error::root("source_edit_value_unsupported"));
        }
        updated
    };
    Ok(updated)
}

/// Every selection is derived from accepted paths and ancestry rather than editable YAML.
struct Target {
    owner: SourcePath,
    native: SourcePath,
    inline: Vec<(usize, usize)>,
}

/// Resolve a native UUID to one accepted owner and its inline coordinates.
fn target(base: &AcceptedBase, uuid: &NativeUuid) -> Result<Target, Error> {
    for (native, document) in &base.package.documents {
        if let Some(inline) = find_node(&document.descriptor.object, uuid, &mut Vec::new()) {
            let owner = base
                .package
                .owners
                .iter()
                .find(|(_, paths)| paths.contains(native))
                .map(|(owner, _)| owner.clone())
                .ok_or_else(|| Error::root("source_descriptor_missing"))?;
            return Ok(Target {
                owner,
                native: native.clone(),
                inline,
            });
        }
    }
    Err(Error::root("source_edit_object_unknown"))
}

/// Traverse only declared inline metadata; opaque XML is never an editable identity source.
fn find_node(
    node: &MetadataNode,
    uuid: &NativeUuid,
    path: &mut Vec<(usize, usize)>,
) -> Option<Vec<(usize, usize)>> {
    if node.uuid.key() == uuid.key() {
        return Some(path.clone());
    }
    for (section, value) in node.sections.iter().enumerate() {
        if let Section::Children(children) = value {
            for (index, child) in children.iter().enumerate() {
                if let Child::Inline { object } = child {
                    path.push((section, index));
                    let found = find_node(object, uuid, path);
                    path.pop();
                    if found.is_some() {
                        return found;
                    }
                }
            }
        }
    }
    None
}

/// A moved, renamed or substituted ancestor refuses editing instead of silently retargeting it.
fn anchored_node<'a>(
    working: &'a mut MetadataNode,
    accepted: &MetadataNode,
    path: &[(usize, usize)],
    steps: &mut Vec<cst::Step>,
    property: PropertyName,
    schema: u32,
) -> Result<&'a mut MetadataNode, Error> {
    if working.uuid.key() != accepted.uuid.key()
        || working.kind != accepted.kind
        || working.property(PropertyName::Name) != accepted.property(PropertyName::Name)
    {
        return Err(Error::root("source_edit_structure_changed"));
    }
    let Some((&(section, index), tail)) = path.split_first() else {
        if accepted.property(property).is_none() {
            return Err(Error::root("source_edit_property_missing"));
        }
        if let (
            Some(PropertyValue::TypeDescription(current)),
            Some(PropertyValue::TypeDescription(anchor)),
        ) = (working.property(property), accepted.property(property))
            && current.native != anchor.native
        {
            return Err(Error::root("source_edit_structure_changed"));
        }
        if let (
            Some(PropertyValue::ReferenceList(current)),
            Some(PropertyValue::ReferenceList(anchor)),
        ) = (working.property(property), accepted.property(property))
            && current.native != anchor.native
        {
            return Err(Error::root("source_edit_structure_changed"));
        }
        if let (
            Some(PropertyValue::EnumFillValue(current)),
            Some(PropertyValue::EnumFillValue(anchor)),
        ) = (working.property(property), accepted.property(property))
            && current.native != anchor.native
        {
            return Err(Error::root("source_edit_structure_changed"));
        }
        return Ok(working);
    };
    let accepted_child = inline_child(accepted, section, index)
        .ok_or_else(|| Error::root("source_edit_structure_changed"))?;
    let Some(Section::Children(items)) = working.sections.get_mut(section) else {
        return Err(Error::root("source_edit_structure_changed"));
    };
    let Some(Child::Inline { object }) = items.get_mut(index) else {
        return Err(Error::root("source_edit_structure_changed"));
    };
    steps.extend(document_paths::child_steps(schema, section, index));
    anchored_node(object, accepted_child, tail, steps, property, schema)
}

/// Resolve an inline coordinate without interpreting external references or opaque children.
fn inline_child(node: &MetadataNode, section: usize, index: usize) -> Option<&MetadataNode> {
    match node.sections.get(section) {
        Some(Section::Children(items)) => match items.get(index) {
            Some(Child::Inline { object }) => Some(object.as_ref()),
            _ => None,
        },
        _ => None,
    }
}

/// Only an existing verified property can be changed; this never manufactures native defaults.
fn change_property(
    node: &mut MetadataNode,
    property: PropertyName,
    value: PropertyValue,
    steps: &mut Vec<cst::Step>,
    schema: u32,
) -> Result<PropertyValue, Error> {
    for (section, contents) in node.sections.iter_mut().enumerate() {
        if let Section::Properties(items) = contents {
            for (index, entry) in items.iter_mut().enumerate() {
                if let Property::Typed {
                    name,
                    value: previous,
                } = entry
                    && *name == property
                {
                    if schema == super::READABLE_SCHEMA_VERSION {
                        steps.extend([cst::Step::Key("properties"), cst::Step::Key(name.as_str())]);
                    } else {
                        steps.extend([
                            cst::Step::Key("sections"),
                            cst::Step::Index(section),
                            cst::Step::Key("items"),
                            cst::Step::Index(index),
                            cst::Step::Key("value"),
                        ]);
                    }
                    return Ok(std::mem::replace(previous, value));
                }
            }
        }
    }
    Err(Error::root("source_edit_property_missing"))
}

/// Preserve the reason for refusing an edit independently of locale.
fn permission(base: &AcceptedBase, uuid: &NativeUuid, property: PropertyName) -> Result<(), Error> {
    let permission = base.permission(uuid, Operation::Property(property));
    if permission.allowed {
        return Ok(());
    }
    Err(Error::root(match permission.reason {
        PermissionReason::NativeRestriction => "source_edit_native_restricted",
        PermissionReason::EskaFrozen => "source_edit_frozen",
        PermissionReason::UnknownObject => "source_edit_object_unknown",
        PermissionReason::UnsupportedOperation | PermissionReason::Allowed => {
            "source_edit_operation_unsupported"
        }
    })
    .in_field(uuid.as_str()))
}

/// Reject aliases before reading a YAML owner, matching the existing atomic writer's boundary.
fn safe_root(root: &Path, path: &SourcePath) -> Result<PathBuf, Error> {
    if !fs::symlink_metadata(root)
        .map_err(|error| files::io(&error))?
        .file_type()
        .is_dir()
    {
        return Err(Error::root("source_edit_unsafe_path"));
    }
    let root = root.canonicalize().map_err(|error| files::io(&error))?;
    let mut current = root.clone();
    for component in path.as_path().components() {
        current.push(component);
        if fs::symlink_metadata(&current)
            .map_err(|error| files::io(&error))?
            .file_type()
            .is_symlink()
        {
            return Err(Error::root("source_edit_unsafe_path"));
        }
    }
    if !fs::metadata(current)
        .map_err(|error| files::io(&error))?
        .is_file()
    {
        return Err(Error::root("source_edit_unsafe_path"));
    }
    Ok(root)
}

/// Convert the shared writer's domain errors without depending on CLI presentation strings.
fn edit_error(error: PublishError) -> Error {
    match error {
        PublishError::Conflict => Error::root("source_edit_conflict"),
        PublishError::UnsafePath => Error::root("source_edit_unsafe_path"),
        PublishError::ReadOnly => Error::root("source_edit_read_only"),
        PublishError::Io(error) => files::io(&error),
    }
}
