use std::collections::{BTreeMap, BTreeSet};

use super::WorkspaceError;
use crate::project::{
    Project,
    configurator::{
        ChildrenState, ConfiguratorSchema, ConfiguratorTree, ModuleAvailability, TreeNode,
        TreeOptions,
    },
    designer_source::{DesignerSource, SourceLocation, SourceRole},
    metadata_model::{LocalizedText, MetadataKind, NodeId, ObjectId, ProjectScope},
    metadata_parser::{self, LoadError, LocatedProperty, ParsedDescriptor, PropertiesMode},
};

/// Selecting a visible reference does not require its descriptor or invent a UUID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectSummary {
    pub id: ObjectId,
    pub kind: MetadataKind,
    pub name: String,
    pub parent: Option<ObjectId>,
    pub uuid: Option<String>,
    pub synonyms: Vec<LocalizedText>,
}

/// Retained visible nodes are a session snapshot; external invalidation belongs to T67.
#[derive(Debug)]
pub struct ProjectSession {
    source: DesignerSource,
    schema: ConfiguratorSchema,
    root: NodeId,
    nodes: BTreeMap<NodeId, TreeNode>,
    objects: BTreeMap<ObjectId, ObjectSummary>,
    expanded: BTreeSet<ObjectId>,
}

impl ProjectSession {
    /// Open one checked source without loading referenced child descriptors.
    pub(super) fn open(source: DesignerSource) -> Result<Self, WorkspaceError> {
        let schema = ConfiguratorSchema::for_source(&source);
        let root = NodeId::Object(source.root().id().clone());
        let mut session = Self {
            source,
            schema,
            root,
            nodes: BTreeMap::new(),
            objects: BTreeMap::new(),
            expanded: BTreeSet::new(),
        };
        let id = session.source.root().id().clone();
        session.expand(&id)?;
        Ok(session)
    }

    /// Return checked project paths and manifest settings.
    #[must_use]
    pub const fn project(&self) -> &Project {
        self.source.project()
    }

    /// Return the selected member's namespace.
    #[must_use]
    pub const fn scope(&self) -> &ProjectScope {
        self.source.scope()
    }

    /// Return the stable root ID; use node to inspect its presentation data without IO.
    #[must_use]
    pub const fn root(&self) -> &NodeId {
        &self.root
    }

    /// Select an already exposed node without reading or parsing any file.
    ///
    /// # Errors
    /// Returns an unknown ID; arbitrary identities do not trigger source discovery.
    pub fn node(&self, id: &NodeId) -> Result<&TreeNode, WorkspaceError> {
        self.nodes
            .get(id)
            .ok_or_else(|| WorkspaceError::UnknownNode(id.clone()))
    }

    /// Return metadata already known from the owner or its reference, without IO.
    ///
    /// # Errors
    /// Returns an unknown object, including a fabricated descendant not exposed by its owner.
    pub fn object(&self, id: &ObjectId) -> Result<&ObjectSummary, WorkspaceError> {
        self.objects
            .get(id)
            .ok_or_else(|| WorkspaceError::UnknownObject(id.clone()))
    }

    /// Expand only the requested object's descriptor, retaining all virtual parent links.
    ///
    /// # Errors
    /// Distinguishes unknown IDs, missing sources and malformed descriptors.
    /// A failed branch remains selectable and can be retried after the file is corrected.
    pub fn children(
        &mut self,
        id: &NodeId,
        options: TreeOptions,
    ) -> Result<Vec<&TreeNode>, WorkspaceError> {
        self.node(id)?;
        if let NodeId::Object(owner) = id
            && !self.expanded.contains(owner)
            && let Err(error) = self.expand(owner)
        {
            if let Some(node) = self.nodes.get_mut(id) {
                node.state = ChildrenState::Error;
            }
            return Err(error);
        }
        Ok(self
            .node(id)?
            .children
            .iter()
            .filter_map(|child| self.nodes.get(child))
            .filter(|node| {
                !(options.hide_empty_root_sections
                    && node.root_section
                    && node.state == ChildrenState::Empty)
            })
            .collect())
    }

    /// Return the inclusive root-to-node presentation path, including virtual collections.
    ///
    /// # Errors
    /// Returns unknown IDs or a broken/cyclic presentation ancestry.
    pub fn ancestry(&self, id: &NodeId) -> Result<Vec<NodeId>, WorkspaceError> {
        let mut path = Vec::new();
        let mut current = Some(id);
        while let Some(id) = current {
            if path.len() >= self.nodes.len() {
                return Err(WorkspaceError::BrokenAncestry(id.clone()));
            }
            let node = self.node(id)?;
            path.push(id.clone());
            current = node.parent.as_ref();
        }
        path.reverse();
        Ok(path)
    }

    /// Read existing properties from just this object's owning XML, including inline objects.
    ///
    /// This explicit request reads the file each time; property caching is deferred to T67.
    /// The visible tree snapshot is not replaced by a property read.
    ///
    /// # Errors
    /// Returns unknown objects, absent descriptors, containment or parser failures.
    pub fn properties(&self, id: &ObjectId) -> Result<Vec<LocatedProperty>, WorkspaceError> {
        self.object(id)?;
        let parsed = self.load(id, PropertiesMode::All)?;
        parsed
            .objects
            .into_iter()
            .find(|object| object.metadata.id() == id)
            .map(|object| object.properties.unwrap_or_default())
            .ok_or_else(|| WorkspaceError::Load(LoadError::ObjectNotFound(id.clone())))
    }

    /// Return all applicable existing sources; virtual collections intentionally have none.
    ///
    /// Paths are relative to `project().source()`. Module nodes select only their BSL role;
    /// selecting an object retains its descriptor, modules and form/template payloads.
    ///
    /// # Errors
    /// Returns unknown nodes, missing physical sources or structured resolver failures.
    pub fn source(&self, id: &NodeId) -> Result<Vec<SourceLocation>, WorkspaceError> {
        self.node(id)?;
        let sources: Vec<_> = match id {
            NodeId::Collection { .. } => return Ok(Vec::new()),
            NodeId::Module { owner, role } => self
                .source
                .module(owner, *role)
                .map_err(WorkspaceError::Source)?
                .into_iter()
                .collect(),
            NodeId::Object(owner) => {
                let object = self.object(owner)?;
                let roles = self.schema.modules(owner, object.kind);
                self.source
                    .sources(owner)
                    .map_err(WorkspaceError::Source)?
                    .into_iter()
                    .filter(|source| match source.role {
                        SourceRole::Module(role) => roles.contains(&role),
                        _ => true,
                    })
                    .collect()
            }
        };
        if sources.is_empty() {
            return Err(WorkspaceError::MissingSource(id.clone()));
        }
        Ok(sources)
    }

    /// Normalize missing XML to the same source category used for missing BSL.
    fn load(
        &self,
        id: &ObjectId,
        mode: PropertiesMode,
    ) -> Result<ParsedDescriptor, WorkspaceError> {
        metadata_parser::load(&self.source, id, mode).map_err(|error| match error {
            LoadError::MissingDescriptor(id) => WorkspaceError::MissingSource(NodeId::Object(id)),
            error => WorkspaceError::Load(error),
        })
    }

    /// Prepare a branch completely before publishing any nodes or object summaries.
    fn expand(&mut self, id: &ObjectId) -> Result<(), WorkspaceError> {
        let parsed = self.load(id, PropertiesMode::Summary)?;
        let modules = parsed
            .objects
            .iter()
            .map(|object| {
                ModuleAvailability::resolve(
                    &self.source,
                    &self.schema,
                    object.metadata.id(),
                    object.metadata.kind(),
                )
                .map(|availability| (object.metadata.id().clone(), availability))
            })
            .collect::<Result<_, _>>()
            .map_err(WorkspaceError::Source)?;
        let tree = ConfiguratorTree::build(&self.schema, &parsed, &modules)
            .map_err(WorkspaceError::Tree)?;
        for projected in tree.nodes() {
            // The branch root has no parent in its local projection; retain the global parent.
            if let NodeId::Object(owner) = &projected.id
                && self.expanded.contains(owner)
            {
                continue;
            }
            let mut node = projected.clone();
            if let Some(previous) = self.nodes.get(&node.id) {
                node.parent.clone_from(&previous.parent);
            }
            self.nodes.insert(node.id.clone(), node);
        }
        self.retain_objects(parsed);
        Ok(())
    }

    /// Keep lightweight summaries only; XML buffers and property values do not survive expansion.
    fn retain_objects(&mut self, parsed: ParsedDescriptor) {
        for reference in parsed.references {
            self.objects
                .entry(reference.id.clone())
                .or_insert_with(|| ObjectSummary {
                    parent: reference.id.parent(),
                    id: reference.id,
                    kind: reference.kind,
                    name: reference.name,
                    uuid: None,
                    synonyms: Vec::new(),
                });
        }
        for object in parsed.objects {
            let metadata = object.metadata;
            self.expanded.insert(metadata.id().clone());
            self.objects.insert(
                metadata.id().clone(),
                ObjectSummary {
                    id: metadata.id().clone(),
                    kind: metadata.kind(),
                    name: metadata.name().to_owned(),
                    parent: metadata.parent().cloned(),
                    uuid: Some(metadata.uuid().to_owned()),
                    synonyms: object.synonyms,
                },
            );
        }
    }
}
