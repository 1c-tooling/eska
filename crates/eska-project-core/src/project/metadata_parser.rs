//! Bounded, per-descriptor Designer parsing into owned summaries and optional properties.

mod predefined;
mod values;
pub(crate) use predefined::parse as parse_predefined;

use std::{collections::HashSet, ops::Range, path::PathBuf};

use roxmltree::{Document, Node, ParsingOptions};

use super::{
    designer_source::{DesignerSource, SourceError},
    metadata_model::{LocalizedText, MetadataKind, MetadataObject, MetadataProperty, ObjectId},
};

pub(super) const MD_NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

/// Summaries suffice for tree/search; complete property values are opt-in.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropertiesMode {
    Summary,
    All,
}

/// A property retains a byte address in the unchanged UTF-8 descriptor, including any BOM.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct LocatedProperty {
    pub property: MetadataProperty,
    pub range: Range<usize>,
}

/// One inline or standalone object; children contain IDs in declaration order.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ParsedObject {
    pub metadata: MetadataObject,
    pub synonyms: Vec<LocalizedText>,
    pub range: Range<usize>,
    pub children: Vec<ObjectId>,
    /// None means not requested, rather than an object without properties.
    pub properties: Option<Vec<LocatedProperty>>,
}

/// A named child in another descriptor; resolving it never happens during parsing.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MetadataReference {
    pub id: ObjectId,
    pub owner: ObjectId,
    pub kind: MetadataKind,
    pub name: String,
    pub range: Range<usize>,
}

/// Recoverable unsupported fragments remain visible to consumers as diagnostics.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub range: Range<usize>,
    pub issue: ParseIssue,
}

/// Machine-facing parse failures, independent of presentation and locale.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum ParseIssue {
    UnsupportedRoot,
    UnknownKind {
        namespace: Option<String>,
        name: String,
    },
    InvalidObject,
    DuplicateIdentity(ObjectId),
    UnsupportedValue,
}

/// No DOM, source buffer, payload or module content survives in this result.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct ParsedDescriptor {
    pub version: Option<String>,
    pub objects: Vec<ParsedObject>,
    pub references: Vec<MetadataReference>,
    pub diagnostics: Vec<Diagnostic>,
}

/// A failure is local to this descriptor, not a failed project-wide discovery.
#[derive(Debug)]
pub enum ParseError {
    TooLarge,
    TooDeep,
    Envelope(quick_xml::Error),
    Xml(roxmltree::Error),
    Invalid(Diagnostic),
}

/// File loading preserves both source and parse errors without guessing an object.
#[derive(Debug)]
pub enum LoadError {
    Source(SourceError),
    MissingDescriptor(ObjectId),
    Parse { path: PathBuf, source: ParseError },
    ObjectNotFound(ObjectId),
}

/// Read exactly one owning descriptor, including inline ancestors, without opening children.
///
/// # Errors
/// Returns a missing/unsafe source, descriptor parse failure or absent requested identity.
pub fn load(
    source: &DesignerSource,
    id: &ObjectId,
    mode: PropertiesMode,
) -> Result<ParsedDescriptor, LoadError> {
    let location = source
        .object_descriptor(id)
        .map_err(LoadError::Source)?
        .ok_or_else(|| LoadError::MissingDescriptor(id.clone()))?;
    let input = source
        .read_xml(&location.path)
        .map_err(LoadError::Source)?
        .ok_or_else(|| LoadError::MissingDescriptor(id.clone()))?;
    let mut descriptor_id = id.clone();
    for _ in &location.inline {
        descriptor_id = descriptor_id
            .parent()
            .ok_or_else(|| LoadError::ObjectNotFound(id.clone()))?;
    }
    let parsed = (if location
        .inline
        .first()
        .is_some_and(|item| item.kind == MetadataKind::PredefinedItem)
    {
        parse_predefined(&input, &descriptor_id, mode)
    } else {
        parse(&input, descriptor_id.parent(), mode)
    })
    .map_err(|source| LoadError::Parse {
        path: location.path,
        source,
    })?;
    if !parsed
        .objects
        .iter()
        .any(|object| object.metadata.id() == id)
    {
        return Err(LoadError::ObjectNotFound(id.clone()));
    }
    Ok(parsed)
}

/// Parse a single descriptor; parent is supplied only for a separate nested object's file.
///
/// Byte ranges refer to the exact input. Unknown children produce diagnostics; invalid roots
/// fail. Child references remain references even if their files are missing or malformed.
///
/// # Errors
/// Returns bounded-input failures, malformed XML, or an invalid root object.
pub fn parse(
    input: &str,
    parent: Option<ObjectId>,
    mode: PropertiesMode,
) -> Result<ParsedDescriptor, ParseError> {
    check_envelope(input)?;
    let document = Document::parse_with_options(
        input,
        ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(ParseError::Xml)?;
    let root = document.root_element();
    let mut children = root.children().filter(Node::is_element);
    let object = children
        .next()
        .filter(|_| root.has_tag_name((MD_NAMESPACE, "MetaDataObject")))
        .filter(|_| children.next().is_none())
        .ok_or_else(|| {
            ParseError::Invalid(Diagnostic {
                range: root.range(),
                issue: ParseIssue::UnsupportedRoot,
            })
        })?;
    let mut parser = Parser {
        result: ParsedDescriptor {
            version: root.attribute("version").map(str::to_owned),
            objects: Vec::new(),
            references: Vec::new(),
            diagnostics: Vec::new(),
        },
        identities: HashSet::new(),
        mode,
    };
    parser.object(object, parent).map_err(ParseError::Invalid)?;
    Ok(parser.result)
}

/// Bound nesting before entering roxmltree's recursive tokenizer, including debug builds.
pub(super) fn check_envelope(input: &str) -> Result<(), ParseError> {
    use quick_xml::events::Event;
    if input.len() > MAX_BYTES {
        return Err(ParseError::TooLarge);
    }
    let mut reader = quick_xml::Reader::from_str(input);
    let mut depth = 0_usize;
    loop {
        match reader.read_event().map_err(ParseError::Envelope)? {
            Event::Start(_) => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(ParseError::TooDeep);
                }
            }
            Event::Empty(_) if depth == MAX_DEPTH => return Err(ParseError::TooDeep),
            Event::End(_) => {
                depth = depth.saturating_sub(1);
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

struct Parser {
    result: ParsedDescriptor,
    identities: HashSet<ObjectId>,
    mode: PropertiesMode,
}

impl Parser {
    /// Extract the object's own data before walking only its `ChildObjects` elements.
    fn object(
        &mut self,
        node: Node<'_, '_>,
        parent: Option<ObjectId>,
    ) -> Result<ObjectId, Diagnostic> {
        let kind = kind(node)?;
        let mut property_nodes = node
            .children()
            .filter(|node| node.has_tag_name((MD_NAMESPACE, "Properties")));
        let properties = property_nodes
            .next()
            .filter(|_| property_nodes.next().is_none())
            .ok_or_else(|| invalid(node))?;
        let mut names = properties
            .children()
            .filter(|node| node.has_tag_name((MD_NAMESPACE, "Name")));
        let name = names
            .next()
            .and_then(simple_text)
            .filter(|_| names.next().is_none())
            .ok_or_else(|| invalid(node))?;
        let uuid = node
            .attribute("uuid")
            .filter(|uuid| !uuid.is_empty())
            .ok_or_else(|| invalid(node))?;
        let metadata = MetadataObject::new(kind, name.trim().to_owned(), uuid.to_owned(), parent)
            .map_err(|_| invalid(node))?;
        let id = metadata.id().clone();
        if !self.identities.insert(id.clone()) {
            return Err(Diagnostic {
                range: node.range(),
                issue: ParseIssue::DuplicateIdentity(id),
            });
        }
        let mut synonyms = Vec::new();
        for synonym in properties
            .children()
            .filter(|node| node.has_tag_name((MD_NAMESPACE, "Synonym")))
        {
            match values::localized(synonym) {
                Some(texts) => synonyms.extend(texts),
                None => self.result.diagnostics.push(Diagnostic {
                    range: synonym.range(),
                    issue: ParseIssue::UnsupportedValue,
                }),
            }
        }
        let all = (self.mode == PropertiesMode::All).then(|| {
            properties
                .children()
                .filter(Node::is_element)
                .map(|node| LocatedProperty {
                    property: values::property(node, &mut self.result.diagnostics),
                    range: node.range(),
                })
                .collect()
        });
        let index = self.result.objects.len();
        self.result.objects.push(ParsedObject {
            metadata,
            synonyms,
            range: node.range(),
            children: Vec::new(),
            properties: all,
        });
        for child in node
            .children()
            .filter(|node| node.has_tag_name((MD_NAMESPACE, "ChildObjects")))
            .flat_map(|node| node.children())
            .filter(Node::is_element)
        {
            // Configuration collections have historically unqualified top-level identities.
            let parent = (kind != MetadataKind::Configuration).then(|| id.clone());
            let result = if child.children().any(|node| node.is_element()) {
                self.object(child, parent)
            } else {
                self.reference(child, &id, parent.as_ref())
            };
            match result {
                Ok(child_id) => self.result.objects[index].children.push(child_id),
                Err(diagnostic) => self.result.diagnostics.push(diagnostic),
            }
        }
        Ok(id)
    }

    /// Keep unresolved references and their owner without inventing UUIDs for parsed objects.
    fn reference(
        &mut self,
        node: Node<'_, '_>,
        owner: &ObjectId,
        parent: Option<&ObjectId>,
    ) -> Result<ObjectId, Diagnostic> {
        let kind = kind(node)?;
        let name = simple_text(node)
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| invalid(node))?
            .trim()
            .to_owned();
        let id = ObjectId::from_parts(parent, kind.as_str(), &name);
        if !self.identities.insert(id.clone()) {
            return Err(Diagnostic {
                range: node.range(),
                issue: ParseIssue::DuplicateIdentity(id),
            });
        }
        self.result.references.push(MetadataReference {
            id: id.clone(),
            owner: owner.clone(),
            kind,
            name,
            range: node.range(),
        });
        Ok(id)
    }
}

/// Resolve by namespace URI, never by an XML prefix or foreign lookalike tag.
fn kind(node: Node<'_, '_>) -> Result<MetadataKind, Diagnostic> {
    let tag = node.tag_name();
    (tag.namespace() == Some(MD_NAMESPACE))
        .then(|| MetadataKind::from_xml_tag(tag.name()).ok())
        .flatten()
        .ok_or_else(|| Diagnostic {
            range: node.range(),
            issue: ParseIssue::UnknownKind {
                namespace: tag.namespace().map(str::to_owned),
                name: tag.name().to_owned(),
            },
        })
}

/// Preserve all decoded text segments, including text separated by comments or CDATA.
fn simple_text(node: Node<'_, '_>) -> Option<String> {
    (!node.children().any(|node| node.is_element())).then(|| {
        node.children()
            .filter(Node::is_text)
            .filter_map(|node| node.text())
            .collect()
    })
}

/// Construct a local structural failure at the offending metadata element.
fn invalid(node: Node<'_, '_>) -> Diagnostic {
    Diagnostic {
        range: node.range(),
        issue: ParseIssue::InvalidObject,
    }
}

#[cfg(test)]
mod tests;
