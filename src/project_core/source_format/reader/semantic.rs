//! YAML and XML share logical metadata values; coordinates always belong to the input format.

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, HashSet},
    ops::Range,
};

use yaml_edit::YamlFile;

use crate::project_core::{
    MetadataKind,
    metadata_model::{
        LocalizedText, MetadataObject, MetadataProperty, MetadataValue, ObjectId, PropertyKey,
    },
    metadata_parser::{
        self, Diagnostic, LocatedProperty, MetadataReference, ParseIssue, ParsedDescriptor,
        ParsedObject, PropertiesMode,
    },
    source_format::{
        Child, Error, MD_NAMESPACE, MetadataNode, Namespace, NativeDocument, NativeUuid,
        ObjectSource, Property, PropertyName, PropertyValue, Section, SourcePath, XmlFragment,
        fragment,
        validation::scope,
        yaml::{Step, range_at},
    },
};

use super::node_name;

const XSI_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// A logical native descriptor with locations in the owning YAML, not generated XML.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeMetadata {
    pub path: SourcePath,
    pub descriptor: ParsedDescriptor,
}

/// Shared semantic values are tied to one exact disk or in-memory YAML revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataSnapshot {
    pub path: SourcePath,
    pub source_sha256: String,
    /// Exact owning YAML size for source quota accounting; this is not the model's heap size.
    pub source_bytes: u64,
    pub manifest_sha256: String,
    pub documents: Vec<NativeMetadata>,
    /// Selected coordinates address object/property spans in the exact text which was parsed.
    pub coordinates: super::super::TextCoordinates,
}

/// Keep a separate logical descriptor per native file while sharing the physical owner text.
pub(super) fn documents(
    source: &ObjectSource,
    tree: &YamlFile,
    parents: &BTreeMap<SourcePath, Option<NativeUuid>>,
    logical_parent: Option<&ObjectId>,
    mode: PropertiesMode,
) -> Result<Vec<NativeMetadata>, Error> {
    let roots = root_objects(source, parents, logical_parent)?;
    let mut result = Vec::new();
    for (index, part) in std::iter::once(&source.document)
        .chain(&source.nested)
        .enumerate()
    {
        let mut steps = super::super::document_paths::root_steps(index);
        let root = roots
            .get(&part.path)
            .ok_or_else(|| Error::root("source_identity_mismatch"))?;
        let mut builder = Projection {
            schema: part.descriptor.schema_version,
            tree,
            mode,
            identities: HashSet::new(),
            result: ParsedDescriptor {
                version: Some(part.descriptor.xml.version.clone()),
                objects: Vec::new(),
                references: Vec::new(),
                diagnostics: Vec::new(),
            },
        };
        builder
            .object(
                &part.descriptor.object,
                root.parent().cloned(),
                &part.descriptor.xml.namespaces,
                &mut steps,
            )
            .map_err(|diagnostic| {
                Error::root("source_semantic_root_invalid")
                    .in_field(diagnostic.range.start.to_string())
            })?;
        result.push(NativeMetadata {
            path: part.path.clone(),
            descriptor: builder.result,
        });
    }
    Ok(result)
}

/// Logical top-level IDs remain unqualified; nested files inherit their actual object's ID.
fn root_objects(
    source: &ObjectSource,
    parents: &BTreeMap<SourcePath, Option<NativeUuid>>,
    logical_parent: Option<&ObjectId>,
) -> Result<BTreeMap<SourcePath, MetadataObject>, Error> {
    let mut children: BTreeMap<String, Vec<&NativeDocument>> = BTreeMap::new();
    for part in &source.nested {
        let parent = parents
            .get(&part.path)
            .and_then(Option::as_ref)
            .ok_or_else(|| Error::root("source_identity_mismatch"))?;
        children.entry(parent.key()).or_default().push(part);
    }
    let mut result = BTreeMap::new();
    let mut pending = vec![(&source.document, logical_parent.cloned())];
    while let Some((part, parent)) = pending.pop() {
        let node = &part.descriptor.object;
        let metadata = metadata_object(node, parent)?;
        for child in children.remove(&node.uuid.key()).unwrap_or_default() {
            pending.push((child, Some(metadata.id().clone())));
        }
        result.insert(part.path.clone(), metadata);
    }
    if !children.is_empty() {
        return Err(Error::root("source_identity_mismatch"));
    }
    Ok(result)
}

/// Apply the existing logical-name normalization while retaining native identity separately.
fn metadata_object(node: &MetadataNode, parent: Option<ObjectId>) -> Result<MetadataObject, Error> {
    MetadataObject::new(
        node.kind,
        node_name(node)?.trim().into(),
        node.uuid.as_str().into(),
        parent,
    )
    .map_err(|_| Error::root("source_name_missing"))
}

struct Projection<'a> {
    schema: u32,
    tree: &'a YamlFile,
    mode: PropertiesMode,
    identities: HashSet<ObjectId>,
    result: ParsedDescriptor,
}

impl Projection<'_> {
    /// Walk typed objects and references in declaration order, using the common collision rule.
    fn object(
        &mut self,
        node: &MetadataNode,
        parent: Option<ObjectId>,
        inherited: &[Namespace],
        steps: &mut Vec<Step>,
    ) -> Result<ObjectId, Diagnostic> {
        let range = range_at(self.tree, steps).map_err(|_| invalid(0..0))?;
        let metadata = metadata_object(node, parent).map_err(|_| invalid(range.clone()))?;
        let id = metadata.id().clone();
        metadata_parser::reserve_identity(&mut self.identities, &id, range.clone())?;
        let context = scope(inherited, &node.namespaces).map_err(|_| invalid(range.clone()))?;
        let (synonyms, properties) = self
            .properties(node, &context, steps)
            .map_err(|_| invalid(range.clone()))?;
        let index = self.result.objects.len();
        self.result.objects.push(ParsedObject {
            metadata,
            synonyms,
            range,
            children: Vec::new(),
            properties,
        });
        for (section, value) in node.sections.iter().enumerate() {
            if let Section::Children(children) = value {
                for (item, child) in children.iter().enumerate() {
                    let previous = steps.len();
                    steps.extend(super::super::document_paths::child_entry_steps(
                        self.schema,
                        section,
                        item,
                    ));
                    let parent = (node.kind != MetadataKind::Configuration).then(|| id.clone());
                    let child_id = match child {
                        Child::Inline { object } => {
                            steps.push(Step::Key("object"));
                            self.object(object, parent, &context, steps).map(Some)
                        }
                        Child::Reference {
                            metadata_kind,
                            name,
                        } => self
                            .reference(*metadata_kind, name, &id, parent.as_ref(), steps)
                            .map(Some),
                        Child::Preserved { fragment } => {
                            self.unsupported_child(fragment, &context, steps)
                        }
                    };
                    steps.truncate(previous);
                    match child_id {
                        Ok(Some(id)) => self.result.objects[index].children.push(id),
                        Ok(None) => {}
                        Err(diagnostic) => self.result.diagnostics.push(diagnostic),
                    }
                }
            }
        }
        Ok(id)
    }

    /// References keep the same unqualified configuration collections and nested object IDs.
    fn reference(
        &mut self,
        kind: MetadataKind,
        name: &str,
        owner: &ObjectId,
        parent: Option<&ObjectId>,
        steps: &[Step],
    ) -> Result<ObjectId, Diagnostic> {
        let range = range_at(self.tree, steps).map_err(|_| invalid(0..0))?;
        let name = name.trim().to_owned();
        let id = ObjectId::from_parts(parent, kind.as_str(), &name);
        metadata_parser::reserve_identity(&mut self.identities, &id, range.clone())?;
        self.result.references.push(MetadataReference {
            id: id.clone(),
            owner: owner.clone(),
            kind,
            name,
            range,
        });
        Ok(id)
    }

    /// Unknown native children remain visible at their YAML slot; comments are not objects.
    fn unsupported_child(
        &self,
        value: &XmlFragment,
        context: &[Namespace],
        steps: &[Step],
    ) -> Result<Option<ObjectId>, Diagnostic> {
        let range = range_at(self.tree, steps).map_err(|_| invalid(0..0))?;
        let wrapper = fragment::wrap(value, context).map_err(|_| invalid(range.clone()))?;
        let doc = roxmltree::Document::parse(&wrapper).map_err(|_| invalid(range.clone()))?;
        let Some(node) = doc
            .root_element()
            .children()
            .find(roxmltree::Node::is_element)
        else {
            return Ok(None);
        };
        let issue = metadata_parser::kind(node)
            .map_or_else(|diagnostic| diagnostic.issue, |_| ParseIssue::InvalidObject);
        Err(Diagnostic { range, issue })
    }

    /// Typed properties are direct values; only opaque native fields use the shared XML reader.
    fn properties(
        &mut self,
        node: &MetadataNode,
        context: &[Namespace],
        steps: &[Step],
    ) -> Result<(Vec<LocalizedText>, Option<Vec<LocatedProperty>>), Error> {
        let mut synonyms = Vec::new();
        let mut all = (self.mode == PropertiesMode::All).then(Vec::new);
        for (section, value) in node.sections.iter().enumerate() {
            if let Section::Properties(properties) = value {
                for (index, property) in properties.iter().enumerate() {
                    let mut location = steps.to_vec();
                    location.extend(super::super::document_paths::property_steps(
                        self.schema,
                        section,
                        index,
                        property,
                    ));
                    let range = range_at(self.tree, &location)?;
                    let property = match property {
                        Property::Typed { name, value } => {
                            Some(typed_property(*name, value, context)?)
                        }
                        Property::Preserved { fragment } => opaque_property(
                            fragment,
                            context,
                            range.clone(),
                            self.mode,
                            &mut self.result.diagnostics,
                        )?,
                    };
                    if let Some(property) = property {
                        if property.key.namespace.as_deref() == Some(MD_NAMESPACE)
                            && property.key.name == "Synonym"
                            && let MetadataValue::Localized(values) = &property.value
                        {
                            synonyms.extend(values.iter().cloned());
                        }
                        if let Some(all) = &mut all {
                            all.push(LocatedProperty { property, range });
                        }
                    }
                }
            }
        }
        Ok((synonyms, all))
    }
}

/// Empty and nil retain their previous semantic value and explicit xsi:nil qualifier.
fn typed_property(
    name: PropertyName,
    value: &PropertyValue,
    context: &[Namespace],
) -> Result<MetadataProperty, Error> {
    let qualifiers = if value == &PropertyValue::Nil {
        vec![(
            PropertyKey {
                namespace: Some(XSI_NAMESPACE.into()),
                name: "nil".into(),
            },
            "true".into(),
        )]
    } else {
        Vec::new()
    };
    let value = match value {
        PropertyValue::Localized(values) => MetadataValue::Localized(
            values
                .iter()
                .map(|value| LocalizedText {
                    language: value.language.clone(),
                    content: value.text.clone(),
                })
                .collect(),
        ),
        PropertyValue::Text(value) | PropertyValue::Enumeration(value) => {
            MetadataValue::Text(value.clone())
        }
        PropertyValue::Boolean(value) => MetadataValue::Text(value.to_string()),
        PropertyValue::Integer(value) => MetadataValue::Text(value.as_str().into()),
        PropertyValue::TypeDescription(value) => value.to_metadata(context)?,
        PropertyValue::Empty | PropertyValue::Nil if name == PropertyName::Synonym => {
            MetadataValue::Localized(Vec::new())
        }
        PropertyValue::Empty | PropertyValue::Nil => MetadataValue::Text(String::new()),
    };
    Ok(MetadataProperty {
        key: PropertyKey {
            namespace: Some(MD_NAMESPACE.into()),
            name: name.as_str().into(),
        },
        qualifiers,
        value,
    })
}

/// Reuse `QName`, record and unsupported-value semantics, mapping diagnostics to the YAML slot.
fn opaque_property(
    value: &XmlFragment,
    context: &[Namespace],
    range: Range<usize>,
    mode: PropertiesMode,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<MetadataProperty>, Error> {
    let wrapper = fragment::wrap(value, context)?;
    let doc = roxmltree::Document::parse(&wrapper)
        .map_err(|_| Error::root("source_preserved_xml_invalid"))?;
    let Some(node) = doc
        .root_element()
        .children()
        .find(roxmltree::Node::is_element)
    else {
        return Ok(None);
    };
    if mode == PropertiesMode::Summary && !node.has_tag_name((MD_NAMESPACE, "Synonym")) {
        return Ok(None);
    }
    let mut issues = Vec::new();
    let property = metadata_parser::values::property(node, &mut issues);
    for issue in issues {
        diagnostics.push(Diagnostic {
            range: range.clone(),
            issue: issue.issue,
        });
    }
    Ok(Some(property))
}

/// Keep invalid logical objects addressable without substituting source-dependent identities.
const fn invalid(range: Range<usize>) -> Diagnostic {
    Diagnostic {
        range,
        issue: ParseIssue::InvalidObject,
    }
}
