//! Target lookup follows declared identity and ancestry, never a filesystem scan or value heuristic.

use std::collections::BTreeMap;

use super::{Domain, ReferenceKind, kind, validate_value};
use crate::project_core::{
    MetadataKind,
    source_format::{
        Child, Error, MetadataNode, NativeDocument, Property, PropertyName, PropertyValue, Section,
        SourcePath, composition::reference_path,
    },
};

/// Validate editable property targets after the complete composition has been checked or selected.
pub(in crate::project_core::source_format) fn validate_documents(
    documents: &BTreeMap<SourcePath, NativeDocument>,
) -> Result<(), Error> {
    let index = Index::build(documents)?;
    for (path, node) in &index.names {
        index.validate_node(path, node)?;
    }
    Ok(())
}

/// Editing uses the independently verified base as target authority without reading other sources.
pub(in crate::project_core::source_format) fn validate_edit(
    documents: &BTreeMap<SourcePath, NativeDocument>,
    updated: &MetadataNode,
) -> Result<(), Error> {
    let index = Index::build(documents)?;
    index.validate_updated(updated)
}

/// One bounded derived lookup exists only for the current operation; values stay in their owners.
struct Index<'a> {
    names: BTreeMap<String, &'a MetadataNode>,
    paths: BTreeMap<String, String>,
}

impl<'a> Index<'a> {
    /// Resolve children through the accepted declaration graph and mandatory native coordinates.
    fn build(documents: &'a BTreeMap<SourcePath, NativeDocument>) -> Result<Self, Error> {
        let root = SourcePath::parse("Configuration.xml")?;
        let node = &documents
            .get(&root)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?
            .descriptor
            .object;
        let mut index = Self {
            names: BTreeMap::new(),
            paths: BTreeMap::new(),
        };
        index.add("", &root, node, documents)?;
        Ok(index)
    }

    /// Duplicate names and reused UUIDs are ambiguous targets even if their source files differ.
    fn add(
        &mut self,
        path: &str,
        native: &SourcePath,
        node: &'a MetadataNode,
        documents: &'a BTreeMap<SourcePath, NativeDocument>,
    ) -> Result<(), Error> {
        if self.names.insert(path.to_owned(), node).is_some()
            || self
                .paths
                .insert(node.uuid.key(), path.to_owned())
                .is_some()
        {
            return Err(Error::root("source_metadata_reference_ambiguous"));
        }
        for section in &node.sections {
            if let Section::Children(children) = section {
                for child in children {
                    match child {
                        Child::Inline { object } => {
                            let name = node_name(object)?;
                            self.add(
                                &child_path(path, object.kind, name),
                                native,
                                object,
                                documents,
                            )?;
                        }
                        Child::Reference {
                            metadata_kind,
                            name,
                        } => {
                            let base = if node.kind == MetadataKind::Configuration {
                                ""
                            } else {
                                native
                                    .as_str()
                                    .strip_suffix(".xml")
                                    .ok_or_else(|| Error::root("source_path_invalid"))?
                            };
                            let native = reference_path(base, *metadata_kind, name)?;
                            let object = &documents
                                .get(&native)
                                .ok_or_else(|| Error::root("source_descriptor_missing"))?
                                .descriptor
                                .object;
                            self.add(
                                &child_path(path, *metadata_kind, name),
                                &native,
                                object,
                                documents,
                            )?;
                        }
                        Child::Preserved { .. } => {}
                    }
                }
            }
        }
        Ok(())
    }

    /// Validate only known typed reference properties; opaque native semantics remain unclaimed.
    fn validate_node(&self, path: &str, node: &MetadataNode) -> Result<(), Error> {
        for section in &node.sections {
            if let Section::Properties(properties) = section {
                for property in properties {
                    let Property::Typed { name, value } = property else {
                        continue;
                    };
                    let Some(shape) = kind(*name, node.kind) else {
                        continue;
                    };
                    let (domain, values) = match (shape, value) {
                        (_, PropertyValue::Empty | PropertyValue::Nil) => continue,
                        (ReferenceKind::One(domain), PropertyValue::Reference(value)) => {
                            (domain, std::slice::from_ref(value))
                        }
                        (ReferenceKind::Many(domain), PropertyValue::ReferenceList(value)) => {
                            (domain, value.items.as_slice())
                        }
                        _ => return Err(Error::root("source_property_type_invalid")),
                    };
                    for value in values {
                        validate_value(domain, node.kind, value)?;
                        if !self.names.contains_key(value.as_str()) {
                            return Err(Error::root("source_metadata_reference_missing")
                                .in_field(node.uuid.as_str())
                                .in_field(name.as_str()));
                        }
                        if matches!(domain, Domain::Form { .. })
                            && value.as_str().contains(".Form.")
                            && value
                                .as_str()
                                .rsplit_once(".Form.")
                                .is_none_or(|(parent, _)| parent != path)
                        {
                            return Err(Error::root("source_metadata_reference_owner_invalid")
                                .in_field(name.as_str()));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// A preview checks every inline candidate against the unchanged accepted target composition.
    fn validate_updated(&self, node: &MetadataNode) -> Result<(), Error> {
        let path = self
            .paths
            .get(&node.uuid.key())
            .ok_or_else(|| Error::root("source_identity_mismatch"))?;
        self.validate_node(path, node)?;
        for section in &node.sections {
            if let Section::Children(children) = section {
                for child in children {
                    if let Child::Inline { object } = child {
                        self.validate_updated(object)?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Root composition does not prefix normal Designer references with the configuration's own name.
fn child_path(parent: &str, kind: MetadataKind, name: &str) -> String {
    let segment = format!("{}.{name}", kind.designer_tag());
    if parent.is_empty() {
        segment
    } else {
        format!("{parent}.{segment}")
    }
}

/// Logical path construction uses the checked Name value, never an authoring filename.
fn node_name(node: &MetadataNode) -> Result<&str, Error> {
    match node.property(PropertyName::Name) {
        Some(PropertyValue::Text(value)) => Ok(value),
        _ => Err(Error::root("source_name_missing")),
    }
}
