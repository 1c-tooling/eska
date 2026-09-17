//! Tree values do not own XML, scan directories, or decide presentation locale.

use std::collections::BTreeMap;

use super::ConfiguratorSchema;
use crate::project::{
    designer_source::{DesignerSource, SourceError},
    metadata_model::{MetadataKind, ModuleRole, NodeId, ObjectId},
    metadata_parser::{Diagnostic, ParsedDescriptor},
};

mod builder;

/// Only proven emptiness can hide a root section; unloaded/error states are distinct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildrenState {
    Empty,
    NonEmpty,
    Unloaded,
    Error,
}

/// Text from metadata is not localized; schema labels are resolved outside core.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TreeLabel {
    Name(String),
    Key(String),
}

/// One stable tree node; virtual parents are separate from metadata's logical ancestry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub label: TreeLabel,
    pub children: Vec<NodeId>,
    pub state: ChildrenState,
    pub expanded_by_default: bool,
    pub root_section: bool,
    pub diagnostics: Vec<Diagnostic>,
}

/// File availability is supplied separately so projection itself never touches the filesystem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModuleAvailability {
    Unloaded,
    /// Roles of existing BSL files, not hypothetical or binary-only modules.
    Loaded(Vec<ModuleRole>),
    /// The caller retains the structured source error; the tree must not claim emptiness.
    Failed,
}

impl ModuleAvailability {
    /// Probe only applicable BSL paths of this owner; never read module bodies or child XML.
    ///
    /// # Errors
    /// Returns source containment/layout failures, to be preserved by the workspace boundary.
    pub fn resolve(
        source: &DesignerSource,
        schema: &ConfiguratorSchema,
        owner: &ObjectId,
        kind: MetadataKind,
    ) -> Result<Self, SourceError> {
        let mut roles = Vec::new();
        for &role in schema.modules(owner, kind) {
            if source.module(owner, role)?.is_some() {
                roles.push(role);
            }
        }
        Ok(Self::Loaded(roles))
    }
}

/// View filtering leaves identities, nested collections and underlying nodes unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TreeOptions {
    pub hide_empty_root_sections: bool,
}

impl Default for TreeOptions {
    /// The user requested root-only empty-section filtering enabled initially.
    fn default() -> Self {
        Self {
            hide_empty_root_sections: true,
        }
    }
}

/// Projection of one descriptor, including unopened references and already parsed inline data.
#[derive(Clone, Debug)]
pub struct ConfiguratorTree {
    root: NodeId,
    nodes: BTreeMap<NodeId, TreeNode>,
}

/// Reject inconsistent caller-supplied metadata rather than constructing a partial false tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TreeError {
    EmptyDescriptor,
    DuplicateObject(ObjectId),
    MissingChild(ObjectId),
}

impl ConfiguratorTree {
    /// Project a descriptor and module availability without opening descendants.
    ///
    /// # Errors
    /// Returns an empty descriptor, duplicate identity or unresolved logical child reference.
    pub fn build(
        schema: &ConfiguratorSchema,
        descriptor: &ParsedDescriptor,
        modules: &BTreeMap<ObjectId, ModuleAvailability>,
    ) -> Result<Self, TreeError> {
        builder::build(schema, descriptor, modules)
    }

    /// Return the projection root; separate nested descriptors retain their original identity.
    #[must_use]
    pub const fn root(&self) -> &NodeId {
        &self.root
    }

    /// Locate a node by identity without parsing XML or performing filesystem operations.
    #[must_use]
    pub fn node(&self, id: &NodeId) -> Option<&TreeNode> {
        self.nodes.get(id)
    }

    /// Return immediate children with root-only filtering applied, preserving declaration order.
    #[must_use]
    pub fn children(&self, id: &NodeId, options: TreeOptions) -> Option<Vec<&TreeNode>> {
        self.node(id).map(|node| {
            node.children
                .iter()
                .filter_map(|id| self.node(id))
                .filter(|node| {
                    !(options.hide_empty_root_sections
                        && node.root_section
                        && node.state == ChildrenState::Empty)
                })
                .collect()
        })
    }

    /// Iterate all projected nodes for inspection, including sections hidden by view options.
    pub fn nodes(&self) -> impl Iterator<Item = &TreeNode> {
        self.nodes.values()
    }
}
