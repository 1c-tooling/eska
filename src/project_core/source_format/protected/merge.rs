//! Accepted members anchor structure; permitted property values and same-class order may vary.

use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use super::super::{
    AcceptedBase, Child, Error, FileRole, ManagedFormSource, MetadataNode, NativeDocument,
    ObjectSource, Operation, Property, PropertyValue, Section, SourcePath, files::Inventory, yaml,
};
use super::{Exclusion, differences, forms, read_working};

/// Selected descriptors and dialogs share one captured owner input and its accepted identity.
pub(super) struct SelectedSources {
    pub documents: BTreeMap<SourcePath, NativeDocument>,
    pub forms: BTreeMap<SourcePath, ManagedFormSource>,
}

/// Read partial working owners without trusting their declared UUID, file map or supplier status.
pub(super) fn sources(
    root: &Path,
    working: &Inventory,
    base: &AcceptedBase,
    diagnostics: &mut Vec<Exclusion>,
) -> Result<SelectedSources, Error> {
    let mut result = SelectedSources {
        documents: BTreeMap::new(),
        forms: BTreeMap::new(),
    };
    let dialogs = base
        .package
        .manifest
        .files
        .iter()
        .filter_map(|file| {
            if let FileRole::ManagedForm { source, .. } = &file.role {
                Some((source, &file.path))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();
    for (path, parts) in &base.package.owners {
        let mut family = accepted_owner(base, parts, dialogs.get(path).copied())?;
        let current = if working.files.contains_key(path) {
            Some(
                yaml::decode(&read_working(root, working, path)?)
                    .map_err(|error| error.in_field(path.as_str()))?,
            )
        } else {
            None
        };
        merge_document(
            base,
            &mut family.document,
            current.as_ref().and_then(|value| value.get("document")),
        )?;
        if let Some(form) = &mut family.form {
            forms::select(
                base,
                &family.document.descriptor.object.uuid,
                form,
                current.as_ref().and_then(|value| value.get("form")),
            )
            .map_err(|error| error.in_field("form").in_field(path.as_str()))?;
        }
        let nested = current
            .as_ref()
            .and_then(|value| value.get("nested"))
            .and_then(Value::as_array);
        let identities: BTreeSet<_> = family
            .nested
            .iter()
            .map(|document| document.descriptor.object.uuid.key())
            .collect();
        for (index, document) in family.nested.iter_mut().enumerate() {
            let candidate = nested_candidate(nested, index, document, &identities)?;
            merge_document(base, document, candidate)?;
        }
        family
            .validate()
            .map_err(|error| error.in_field(path.as_str()))?;
        let mut expected =
            serde_json::to_value(&family).map_err(|_| Error::root("source_schema_invalid"))?;
        super::super::readable::cleared_reference_bindings(base, &mut expected, current.as_ref())?;
        differences::collect(
            base,
            path,
            &expected,
            current.as_ref(),
            &family.document.descriptor.object.uuid,
            diagnostics,
        );
        if let Some(form) = family.form {
            let dialog = dialogs
                .get(path)
                .ok_or_else(|| Error::root("source_baseline_corrupt"))?;
            result.forms.insert((*dialog).clone(), form);
        }
        for document in std::iter::once(family.document).chain(family.nested) {
            result.documents.insert(document.path.clone(), document);
        }
    }
    Ok(result)
}

/// Assemble the exact accepted descriptor/body pair before reading untrusted working fields.
fn accepted_owner(
    base: &AcceptedBase,
    paths: &[SourcePath],
    dialog: Option<&SourcePath>,
) -> Result<ObjectSource, Error> {
    let mut parts = paths.iter().map(|path| {
        base.package
            .documents
            .get(path)
            .cloned()
            .ok_or_else(|| Error::root("source_baseline_corrupt"))
    });
    Ok(ObjectSource {
        document: parts
            .next()
            .ok_or_else(|| Error::root("source_baseline_corrupt"))??,
        nested: parts.collect::<Result<_, _>>()?,
        form: dialog
            .map(|path| {
                base.package
                    .forms
                    .get(path)
                    .cloned()
                    .ok_or_else(|| Error::root("source_baseline_corrupt"))
            })
            .transpose()?,
    })
}

/// Ambiguous external identities fail instead of selecting the first copy of an allowed value.
fn nested_candidate<'a>(
    parts: Option<&'a Vec<Value>>,
    index: usize,
    document: &NativeDocument,
    known: &BTreeSet<String>,
) -> Result<Option<&'a Value>, Error> {
    let mut candidates = parts.into_iter().flatten().filter(|part| {
        part.get("path").and_then(Value::as_str) == Some(document.path.as_str())
            || part
                .get("descriptor")
                .and_then(|descriptor| descriptor.get("object"))
                .and_then(|node| node.get("uuid"))
                .and_then(Value::as_str)
                .is_some_and(|uuid| {
                    uuid.eq_ignore_ascii_case(document.descriptor.object.uuid.as_str())
                })
    });
    let matched = candidates.next();
    if candidates.next().is_some() {
        return Err(Error::root("source_identity_ambiguous"));
    }
    if matched.is_some() {
        return Ok(matched);
    }
    Ok(parts.and_then(|parts| parts.get(index)).filter(|part| {
        part.get("descriptor")
            .and_then(|descriptor| descriptor.get("object"))
            .and_then(|node| node.get("uuid"))
            .and_then(Value::as_str)
            .is_none_or(|uuid| !known.contains(&uuid.to_ascii_lowercase()))
    }))
}

/// A path or UUID edit restores identity while permitted values remain tied to accepted ownership.
fn merge_document(
    base: &AcceptedBase,
    document: &mut NativeDocument,
    current: Option<&Value>,
) -> Result<(), Error> {
    let node = current
        .and_then(|value| value.get("descriptor"))
        .and_then(|value| value.get("object"));
    merge_node(
        base,
        &mut document.descriptor.object,
        node,
        document.descriptor.schema_version,
        &document.path,
    )
    .map_err(|error| error.in_field(document.path.as_str()))
}

/// Preserve protected structure, and recurse into individually permitted inline parts.
fn merge_node(
    base: &AcceptedBase,
    node: &mut MetadataNode,
    current: Option<&Value>,
    schema: u32,
    native: &SourcePath,
) -> Result<(), Error> {
    let sections = current
        .and_then(|value| value.get("sections"))
        .and_then(Value::as_array);
    for section in &mut node.sections {
        match section {
            Section::Properties(properties) => {
                if schema == super::super::READABLE_SCHEMA_VERSION {
                    merge_named_properties(
                        base,
                        &node.uuid,
                        node.kind,
                        properties,
                        current.and_then(|node| node.get("properties")),
                    )?;
                } else {
                    let values = section_items(sections, "properties")?;
                    merge_properties(base, &node.uuid, properties, values)?;
                }
            }
            Section::Children(children) => {
                let values = if schema == super::super::READABLE_SCHEMA_VERSION {
                    current
                        .and_then(|node| node.get("children"))
                        .and_then(Value::as_array)
                } else {
                    section_items(sections, "children")?
                };
                let known: BTreeSet<_> = children
                    .iter()
                    .filter_map(|child| {
                        if let Child::Inline { object } = child {
                            Some(object.uuid.key())
                        } else {
                            None
                        }
                    })
                    .collect();
                for (index, child) in children.iter_mut().enumerate() {
                    if let Child::Inline { object } = child {
                        let candidate =
                            inline_candidate(values, index, &object.uuid.key(), &known)?;
                        merge_node(base, object, candidate, schema, native)?;
                    }
                }
                let renamed = super::super::child_rename::select(
                    base, native, &node.uuid, children, values, schema,
                )?;
                super::super::child_order::select(base, native, &node.uuid, children, values)?;
                super::super::child_creation::select(
                    base, native, &node.uuid, children, values, schema,
                )?;
                super::super::child_deletion::select(base, native, &node.uuid, children, values)?;
                if renamed {
                    super::super::child_creation::validate_names(children.iter())?;
                }
            }
            Section::Preserved(_) | Section::TabularSectionIdentities(_) => {}
        }
    }
    Ok(())
}

/// Ambiguous sections cannot be interpreted as a valid location for an allowed change.
fn section_items<'a>(
    sections: Option<&'a Vec<Value>>,
    kind: &str,
) -> Result<Option<&'a Vec<Value>>, Error> {
    let mut found = sections
        .into_iter()
        .flatten()
        .filter(|section| section.get("kind").and_then(Value::as_str) == Some(kind));
    let result = found
        .next()
        .and_then(|section| section.get("items"))
        .and_then(Value::as_array);
    if found.next().is_some() {
        return Err(Error::root("source_section_duplicate"));
    }
    Ok(result)
}

/// Use native identity for reordered children, with a positional fallback only for a replaced UUID.
fn inline_candidate<'a>(
    values: Option<&'a Vec<Value>>,
    index: usize,
    identity: &str,
    known: &BTreeSet<String>,
) -> Result<Option<&'a Value>, Error> {
    let nodes = values
        .into_iter()
        .flatten()
        .filter_map(|child| child.get("object"));
    let mut matching = nodes.filter(|node| {
        node.get("uuid")
            .and_then(Value::as_str)
            .is_some_and(|uuid| uuid.eq_ignore_ascii_case(identity))
    });
    let matched = matching.next();
    if matching.next().is_some() {
        return Err(Error::root("source_identity_ambiguous"));
    }
    if matched.is_some() {
        return Ok(matched);
    }
    let fallback = values
        .and_then(|values| values.get(index))
        .and_then(|child| child.get("object"));
    Ok(fallback.filter(|node| {
        node.get("uuid")
            .and_then(Value::as_str)
            .is_none_or(|uuid| !known.contains(&uuid.to_ascii_lowercase()))
    }))
}

/// Only the value of an existing typed property may change; additions and removals stay explicit.
fn merge_properties(
    base: &AcceptedBase,
    uuid: &super::super::NativeUuid,
    properties: &mut [Property],
    current: Option<&Vec<Value>>,
) -> Result<(), Error> {
    for property in properties {
        if let Property::Typed { name, value } = property {
            if !base.permission(uuid, Operation::Property(*name)).allowed {
                continue;
            }
            let mut matches = current.into_iter().flatten().filter(|property| {
                property.get("kind").and_then(Value::as_str) == Some("typed")
                    && property.get("name").and_then(Value::as_str) == Some(name.as_str())
            });
            let candidate = matches.next().and_then(|property| property.get("value"));
            if matches.next().is_some() {
                return Err(Error::root("source_property_duplicate"));
            }
            if let Some(candidate) = candidate {
                let mut candidate = serde_json::from_value::<PropertyValue>(candidate.clone())
                    .map_err(|_| Error::root("source_property_invalid").in_field(name.as_str()))?;
                match (&*value, &mut candidate) {
                    (PropertyValue::TypeDescription(old), PropertyValue::TypeDescription(new)) => {
                        new.native.clone_from(&old.native);
                    }
                    (PropertyValue::ReferenceList(old), PropertyValue::ReferenceList(new)) => {
                        new.native.clone_from(&old.native);
                    }
                    (PropertyValue::EnumFillValue(old), PropertyValue::EnumFillValue(new)) => {
                        new.native.clone_from(&old.native);
                    }
                    (PropertyValue::ReferenceList(_), PropertyValue::Empty) => {}
                    (
                        PropertyValue::TypeDescription(_)
                        | PropertyValue::ReferenceList(_)
                        | PropertyValue::EnumFillValue(_),
                        _,
                    )
                    | (
                        _,
                        PropertyValue::TypeDescription(_)
                        | PropertyValue::ReferenceList(_)
                        | PropertyValue::EnumFillValue(_),
                    ) => {
                        return Err(
                            Error::root("source_property_type_invalid").in_field(name.as_str())
                        );
                    }
                    _ => {}
                }
                *value = candidate;
            }
        }
    }
    Ok(())
}

/// Named values inherit the accepted native owner/domain; editable schema fields grant no rights.
fn merge_named_properties(
    base: &AcceptedBase,
    uuid: &super::super::NativeUuid,
    kind: crate::project_core::MetadataKind,
    properties: &mut [Property],
    current: Option<&Value>,
) -> Result<(), Error> {
    for property in properties {
        if let Property::Typed { name, value } = property
            && base.permission(uuid, Operation::Property(*name)).allowed
            && let Some(candidate) = current.and_then(|properties| properties.get(name.as_str()))
        {
            let layout = if let PropertyValue::TypeDescription(value) = &*value {
                Some(&value.native)
            } else {
                None
            };
            let references = if let PropertyValue::ReferenceList(value) = &*value {
                Some(&value.native)
            } else {
                None
            };
            let filling = if let PropertyValue::EnumFillValue(value) = &*value {
                Some(&value.native)
            } else {
                None
            };
            *value = super::super::readable::value_from_wire(
                *name,
                kind,
                candidate.clone(),
                layout,
                references,
                filling,
            )?;
        }
    }
    Ok(())
}
