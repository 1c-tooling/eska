//! A lazy, read-only query view composes only the selected application's ordered sources.

use std::collections::BTreeMap;

use quick_xml::events::Event;
use roxmltree::{Document, Node};

use super::{ProjectSession, QuerySessionError, WorkspaceError, workspace_error};
use crate::{
    application::AnalysisContext,
    project::{
        designer_source,
        metadata_model::{MetadataKind, NodeId, ObjectId, ProjectScope},
        metadata_parser::{self, LoadError, PropertiesMode},
        query::QueryError,
    },
};

const MD: &str = metadata_parser::MD_NAMESPACE;
const MAX_BYTES: usize = 64 * 1024 * 1024;

enum Source<'a> {
    Current(&'a ProjectSession),
    Opened(Box<ProjectSession>),
}

impl Source<'_> {
    /// Borrow the already checked session without cloning metadata or retaining source buffers.
    fn project(&self) -> &ProjectSession {
        match self {
            Self::Current(project) => project,
            Self::Opened(project) => project,
        }
    }
}

pub(super) struct Sources<'a> {
    sources: Vec<Source<'a>>,
}

impl<'a> Sources<'a> {
    /// Reuse the caller's session and open other application members in dependency order.
    pub(super) fn open(
        current: &'a ProjectSession,
        context: &AnalysisContext,
    ) -> Result<Self, QuerySessionError> {
        let mut sources = Vec::new();
        for source in context.sources() {
            if source.project().source() == current.project().source() {
                sources.push(Source::Current(current));
                continue;
            }
            let scope = source.member().map_or(ProjectScope::Standalone, |name| {
                ProjectScope::Member(name.clone())
            });
            let source = designer_source::open_project(source.project().clone(), scope, false)
                .map_err(|error| workspace_error(WorkspaceError::Source(error)))?;
            sources.push(Source::Opened(Box::new(
                ProjectSession::open(source).map_err(workspace_error)?,
            )));
        }
        Ok(Self { sources })
    }

    /// Enumerate declarations without opening descriptors unrelated to the requested table.
    pub(super) fn roots(&self) -> BTreeMap<(MetadataKind, String), &str> {
        let mut roots = BTreeMap::new();
        for source in &self.sources {
            for object in source
                .project()
                .objects
                .values()
                .filter(|o| o.parent.is_none())
            {
                roots
                    .entry((object.kind, object.name.to_lowercase()))
                    .or_insert(object.name.as_str());
            }
        }
        roots
    }

    /// Inherit omitted properties, overlay explicit values and combine named inline query fields.
    /// This temporary XML view is never written back or used as a metadata edit plan.
    pub(super) fn descriptor(
        &self,
        kind: MetadataKind,
        reference: &str,
    ) -> Result<Option<String>, QuerySessionError> {
        let name = reference.rsplit('.').next().unwrap_or(reference);
        let mut effective: Option<Object> = None;
        for source in &self.sources {
            let project = source.project();
            let Some(object) = project
                .objects
                .values()
                .find(|o| o.parent.is_none() && o.kind == kind && super::same_name(&o.name, name))
            else {
                continue;
            };
            let input = read(project, &object.id)?;
            let doc = Document::parse(&input).map_err(|_| QueryError::InvalidXml)?;
            let node = doc
                .root_element()
                .children()
                .find(Node::is_element)
                .ok_or(QueryError::InvalidXml)?;
            let overlay = Object::read(node, &input)?;
            if let Some(effective) = &mut effective {
                effective.overlay(overlay);
            } else {
                effective = Some(overlay);
            }
        }
        effective
            .map(|object| {
                let mut output = format!("<MetaDataObject xmlns=\"{MD}\">");
                object.write(&mut output)?;
                output.push_str("</MetaDataObject>");
                if output.len() > MAX_BYTES {
                    return Err(QueryError::TooLarge.into());
                }
                Ok(output)
            })
            .transpose()
    }
}

/// Validate each declared descriptor before composing it; missing declarations stay distinct from broken files.
fn read(project: &ProjectSession, id: &ObjectId) -> Result<String, QuerySessionError> {
    let missing = || workspace_error(WorkspaceError::MissingSource(NodeId::Object(id.clone())));
    let location = project
        .source
        .object_descriptor(id)
        .map_err(|error| workspace_error(WorkspaceError::Source(error)))?
        .ok_or_else(missing)?;
    let input = project
        .source
        .read_xml(&location.path)
        .map_err(|error| workspace_error(WorkspaceError::Source(error)))?
        .ok_or_else(missing)?;
    let parsed =
        metadata_parser::parse(&input, id.parent(), PropertiesMode::Summary).map_err(|source| {
            workspace_error(WorkspaceError::Load(LoadError::Parse {
                path: location.path,
                source,
            }))
        })?;
    if !parsed
        .objects
        .iter()
        .any(|object| object.metadata.id() == id)
    {
        return Err(workspace_error(WorkspaceError::UnknownObject(id.clone())));
    }
    Ok(input)
}

struct Object {
    tag: String,
    properties: BTreeMap<String, String>,
    children: BTreeMap<(String, String), Self>,
}

impl Object {
    /// Retain complete property subtrees, including inherited namespace bindings and explicit empty values.
    fn read(node: Node<'_, '_>, input: &str) -> Result<Self, QuerySessionError> {
        let properties = node
            .children()
            .find(|n| n.has_tag_name((MD, "Properties")))
            .ok_or(QueryError::InvalidXml)?;
        let mut result = Self {
            tag: node.tag_name().name().to_owned(),
            properties: BTreeMap::new(),
            children: BTreeMap::new(),
        };
        for property in properties.children().filter(Node::is_element) {
            if property.tag_name().namespace() == Some(MD) {
                result.properties.insert(
                    property.tag_name().name().to_owned(),
                    property_xml(property, input)?,
                );
            }
        }
        if let Some(children) = node
            .children()
            .find(|n| n.has_tag_name((MD, "ChildObjects")))
        {
            for child in children.children().filter(Node::is_element).filter(|n| {
                n.tag_name().namespace() == Some(MD)
                    && matches!(
                        n.tag_name().name(),
                        "Attribute" | "Dimension" | "Resource" | "Requisite" | "TabularSection"
                    )
            }) {
                let name = super::fields::property(child, "Name").ok_or(QueryError::InvalidXml)?;
                result.children.insert(
                    (child.tag_name().name().to_owned(), name.to_lowercase()),
                    Self::read(child, input)?,
                );
            }
        }
        Ok(result)
    }

    /// Later application sources override only values they actually declare.
    fn overlay(&mut self, overlay: Self) {
        self.properties.extend(overlay.properties);
        for (key, child) in overlay.children {
            if let Some(inherited) = self.children.get_mut(&key) {
                inherited.overlay(child);
            } else {
                self.children.insert(key, child);
            }
        }
    }

    /// Generate a bounded inspection descriptor; synthetic UUIDs never escape this query-only view.
    fn write(&self, output: &mut String) -> Result<(), QuerySessionError> {
        output.push('<');
        output.push_str(&self.tag);
        output.push_str(" uuid=\"query-schema\"><Properties>");
        for property in self.properties.values() {
            if output.len().saturating_add(property.len()) > MAX_BYTES {
                return Err(QueryError::TooLarge.into());
            }
            output.push_str(property);
        }
        output.push_str("</Properties><ChildObjects>");
        for child in self.children.values() {
            child.write(output)?;
        }
        output.push_str("</ChildObjects></");
        output.push_str(&self.tag);
        output.push('>');
        Ok(())
    }
}

/// Raw subtree bytes retain nested namespaces; add bindings inherited from the original envelope to its first tag.
fn property_xml(node: Node<'_, '_>, input: &str) -> Result<String, QuerySessionError> {
    let raw = &input[node.range()];
    let mut reader = quick_xml::Reader::from_str(raw);
    let event = reader.read_event().map_err(|_| QueryError::InvalidXml)?;
    let (start, empty) = match event {
        Event::Start(start) => (start, false),
        Event::Empty(start) => (start, true),
        _ => return Err(QueryError::InvalidXml.into()),
    };
    let mut tag = start.into_owned();
    for ns in node.namespaces() {
        let key = ns
            .name()
            .map_or_else(|| "xmlns".to_owned(), |name| format!("xmlns:{name}"));
        if !tag
            .attributes()
            .any(|a| a.is_ok_and(|a| a.key.as_ref() == key.as_str()))
        {
            tag.push_attribute((key.as_str(), ns.uri()));
        }
    }
    let mut writer = quick_xml::Writer::new(Vec::new());
    writer
        .write_event(if empty {
            Event::Empty(tag)
        } else {
            Event::Start(tag)
        })
        .map_err(|_| QueryError::InvalidXml)?;
    let mut output = String::from_utf8(writer.into_inner()).map_err(|_| QueryError::InvalidXml)?;
    output.push_str(
        &raw[usize::try_from(reader.buffer_position()).map_err(|_| QueryError::TooLarge)?..],
    );
    Ok(output)
}
