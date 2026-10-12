//! Target lookup follows declared identity and ancestry, never a filesystem scan or value heuristic.

use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

use super::metadata_reference::{Domain, ReferenceKind, kind, validate_value};
use crate::project_core::{
    MetadataKind,
    source_format::{
        Child, Error, MetadataNode, Namespace, NativeDocument, Property, PropertyName,
        PropertyValue, Section, SourcePath, composition::reference_path, type_description,
        validation::scope,
    },
};

/// Validate editable property targets after the complete composition has been checked or selected.
pub(in crate::project_core::source_format) fn validate_documents(
    documents: &BTreeMap<SourcePath, NativeDocument>,
) -> Result<(), Error> {
    let index = Index::build(documents)?;
    for (path, entry) in &index.names {
        index.validate_node(path, entry.node, &entry.context)?;
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
    names: BTreeMap<String, Entry<'a>>,
    paths: BTreeMap<String, String>,
    type_names: BTreeMap<String, bool>,
}

/// Effective namespaces come from the selected native composition, including for edit previews.
struct Entry<'a> {
    node: &'a MetadataNode,
    context: Vec<Namespace>,
}

impl<'a> Index<'a> {
    /// Resolve children through the accepted declaration graph and mandatory native coordinates.
    fn build(documents: &'a BTreeMap<SourcePath, NativeDocument>) -> Result<Self, Error> {
        let root = SourcePath::parse("Configuration.xml")?;
        let document = documents
            .get(&root)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        let mut index = Self {
            names: BTreeMap::new(),
            paths: BTreeMap::new(),
            type_names: BTreeMap::new(),
        };
        index.add(
            "",
            &root,
            &document.descriptor.object,
            &document.descriptor.xml.namespaces,
            documents,
        )?;
        Ok(index)
    }

    /// Duplicate names and reused UUIDs are ambiguous targets even if their source files differ.
    fn add(
        &mut self,
        path: &str,
        native: &SourcePath,
        node: &'a MetadataNode,
        inherited: &[Namespace],
        documents: &'a BTreeMap<SourcePath, NativeDocument>,
    ) -> Result<(), Error> {
        if self
            .paths
            .insert(node.uuid.key(), path.to_owned())
            .is_some()
        {
            return Err(Error::root("source_metadata_reference_ambiguous"));
        }
        let context = scope(inherited, &node.namespaces)?;
        for generated in type_description::generated_types(node, &context)? {
            if self
                .type_names
                .insert(generated.name, generated.is_named_set)
                .is_some()
            {
                return Err(Error::root("source_type_target_ambiguous"));
            }
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
                                &context,
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
                            let document = documents
                                .get(&native)
                                .ok_or_else(|| Error::root("source_descriptor_missing"))?;
                            self.add(
                                &child_path(path, *metadata_kind, name),
                                &native,
                                &document.descriptor.object,
                                &document.descriptor.xml.namespaces,
                                documents,
                            )?;
                        }
                        Child::Preserved { .. } => {}
                    }
                }
            }
        }
        if self
            .names
            .insert(path.to_owned(), Entry { node, context })
            .is_some()
        {
            return Err(Error::root("source_metadata_reference_ambiguous"));
        }
        Ok(())
    }

    /// Validate only known typed reference properties; opaque native semantics remain unclaimed.
    fn validate_node(
        &self,
        path: &str,
        node: &MetadataNode,
        context: &[Namespace],
    ) -> Result<(), Error> {
        for section in &node.sections {
            if let Section::Properties(properties) = section {
                for property in properties {
                    let Property::Typed { name, value } = property else {
                        continue;
                    };
                    if let PropertyValue::TypeDescription(description) = value {
                        for (index, alternative) in description.data.alternatives.iter().enumerate()
                        {
                            if let Some(target) =
                                type_description::named_target(alternative, context)?
                                && let Err(error) = self.validate_type_target(target, node.kind)
                            {
                                return Err(error
                                    .in_field(index.to_string())
                                    .in_field("alternatives")
                                    .in_field(name.as_str())
                                    .in_field(node.uuid.as_str()));
                            }
                        }
                    }
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

    /// The native profile forbids named sets inside a defined type with either alternative spelling.
    fn validate_type_target(&self, target: &str, owner: MetadataKind) -> Result<(), Error> {
        let is_named_set = self
            .type_names
            .get(target)
            .ok_or_else(|| Error::root("source_type_target_missing"))?;
        if *is_named_set && owner == MetadataKind::DefinedType {
            return Err(Error::root("source_type_set_owner_invalid"));
        }
        Ok(())
    }

    /// A preview checks every inline candidate against the unchanged accepted target composition.
    fn validate_updated(&self, node: &MetadataNode) -> Result<(), Error> {
        let path = self
            .paths
            .get(&node.uuid.key())
            .ok_or_else(|| Error::root("source_identity_mismatch"))?;
        let entry = self
            .names
            .get(path)
            .ok_or_else(|| Error::root("source_identity_mismatch"))?;
        self.validate_node(path, node, &entry.context)?;
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
