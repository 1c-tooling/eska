//! Group immediate children by schema, retaining declaration order within each collection.

use std::collections::BTreeMap;

use super::{
    ChildrenState, ConfiguratorSchema, ConfiguratorTree, ModuleAvailability, TreeError, TreeLabel,
    TreeNode,
};
use crate::project::{
    metadata_model::{CollectionKind, MetadataKind, NodeId, ObjectId},
    metadata_parser::{ParsedDescriptor, ParsedObject},
};

/// Build one projection; unloaded references deliberately do not get speculative child nodes.
pub(super) fn build(
    schema: &ConfiguratorSchema,
    descriptor: &ParsedDescriptor,
    modules: &BTreeMap<ObjectId, ModuleAvailability>,
) -> Result<ConfiguratorTree, TreeError> {
    let root = descriptor
        .objects
        .first()
        .ok_or(TreeError::EmptyDescriptor)?
        .metadata
        .id();
    let mut tree = ConfiguratorTree {
        root: NodeId::Object(root.clone()),
        nodes: BTreeMap::new(),
    };
    let mut kinds = BTreeMap::new();
    for object in &descriptor.objects {
        insert_object(
            &mut tree,
            object.metadata.id(),
            object.metadata.name(),
            object.metadata.kind(),
            ChildrenState::Empty,
        )?;
        kinds.insert(object.metadata.id(), object.metadata.kind());
    }
    for reference in &descriptor.references {
        insert_object(
            &mut tree,
            &reference.id,
            &reference.name,
            reference.kind,
            ChildrenState::Unloaded,
        )?;
        kinds.insert(&reference.id, reference.kind);
    }
    for object in &descriptor.objects {
        build_owner(
            &mut tree,
            schema,
            object,
            &kinds,
            modules.get(object.metadata.id()),
        )?;
    }
    if !descriptor.diagnostics.is_empty() {
        let parent = tree.root.clone();
        let id = group(
            &mut tree,
            root,
            CollectionKind::Unsupported,
            &parent,
            schema.has_root_sections(root),
        );
        if let Some(node) = tree.nodes.get_mut(&id) {
            node.state = ChildrenState::Error;
            node.diagnostics.clone_from(&descriptor.diagnostics);
        }
    }
    let root_id = tree.root.clone();
    for node in tree.nodes.values_mut() {
        if matches!(
            node.id,
            NodeId::Collection {
                kind: CollectionKind::Metadata(MetadataKind::PredefinedItem),
                ..
            }
        ) {
            node.state = ChildrenState::Unloaded;
        }
    }
    summarize(&mut tree, &root_id);
    Ok(tree)
}

/// Register each actual object/reference once, independently of its virtual tree parent.
fn insert_object(
    tree: &mut ConfiguratorTree,
    id: &ObjectId,
    name: &str,
    kind: MetadataKind,
    state: ChildrenState,
) -> Result<(), TreeError> {
    let node_id = NodeId::Object(id.clone());
    let node = TreeNode {
        id: node_id.clone(),
        metadata_kind: Some(kind),
        parent: None,
        label: TreeLabel::Name(name.to_owned()),
        children: Vec::new(),
        state,
        expanded_by_default: false,
        root_section: false,
        diagnostics: Vec::new(),
    };
    if tree.nodes.insert(node_id, node).is_some() {
        return Err(TreeError::DuplicateObject(id.clone()));
    }
    Ok(())
}

/// Populate the schema's collections before attaching ordered metadata children.
fn build_owner(
    tree: &mut ConfiguratorTree,
    schema: &ConfiguratorSchema,
    object: &ParsedObject,
    kinds: &BTreeMap<&ObjectId, MetadataKind>,
    modules: Option<&ModuleAvailability>,
) -> Result<(), TreeError> {
    let owner = object.metadata.id();
    let parent = NodeId::Object(owner.clone());
    build_modules(tree, schema, object, modules);
    let mut groups = BTreeMap::new();
    if schema.has_root_sections(owner) {
        let common = group(tree, owner, CollectionKind::Common, &parent, true);
        for kind in ConfiguratorSchema::common_collections() {
            groups.insert(
                kind,
                group(tree, owner, CollectionKind::Metadata(kind), &common, false),
            );
        }
        for kind in ConfiguratorSchema::root_collections() {
            let id = group(tree, owner, CollectionKind::Metadata(kind), &parent, true);
            groups.insert(kind, id.clone());
            if kind == MetadataKind::Document {
                for child in [MetadataKind::DocumentNumerator, MetadataKind::Sequence] {
                    groups.insert(
                        child,
                        group(tree, owner, CollectionKind::Metadata(child), &id, false),
                    );
                }
            }
        }
    } else {
        for &kind in schema.owner_collections(owner, object.metadata.kind()) {
            let destination = if ConfiguratorSchema::has_direct_children(object.metadata.kind()) {
                parent.clone()
            } else {
                group(tree, owner, CollectionKind::Metadata(kind), &parent, false)
            };
            groups.insert(kind, destination);
        }
    }
    for child in &object.children {
        let kind = kinds
            .get(child)
            .ok_or_else(|| TreeError::MissingChild(child.clone()))?;
        let group_id = groups.get(kind).cloned().unwrap_or_else(|| {
            let id = group(
                tree,
                owner,
                CollectionKind::Unsupported,
                &parent,
                schema.has_root_sections(owner),
            );
            if let Some(node) = tree.nodes.get_mut(&id) {
                node.state = ChildrenState::Error;
            }
            id
        });
        attach(tree, &NodeId::Object(child.clone()), &group_id);
    }
    Ok(())
}

/// Only known existing BSL roles become module nodes; an unresolved probe remains visible.
fn build_modules(
    tree: &mut ConfiguratorTree,
    schema: &ConfiguratorSchema,
    object: &ParsedObject,
    availability: Option<&ModuleAvailability>,
) {
    let owner = object.metadata.id();
    let roles = schema.modules(owner, object.metadata.kind());
    if roles.is_empty() {
        return;
    }
    let availability = availability.unwrap_or(&ModuleAvailability::Unloaded);
    if matches!(availability, ModuleAvailability::Loaded(present) if !roles.iter().any(|role| present.contains(role)))
    {
        return;
    }
    let parent = NodeId::Object(owner.clone());
    let id = group(tree, owner, CollectionKind::Modules, &parent, false);
    if let Some(node) = tree.nodes.get_mut(&id) {
        node.expanded_by_default = true;
        node.state = match availability {
            ModuleAvailability::Loaded(_) => ChildrenState::NonEmpty,
            ModuleAvailability::Unloaded => ChildrenState::Unloaded,
            ModuleAvailability::Failed => ChildrenState::Error,
        };
    }
    if let ModuleAvailability::Loaded(present) = availability {
        for &role in roles.iter().filter(|role| present.contains(role)) {
            let module = NodeId::Module {
                owner: owner.clone(),
                role,
            };
            tree.nodes.insert(
                module.clone(),
                TreeNode {
                    id: module.clone(),
                    metadata_kind: None,
                    parent: None,
                    label: TreeLabel::Key(format!("tree-module-{}", role.as_str())),
                    children: Vec::new(),
                    state: ChildrenState::Empty,
                    expanded_by_default: false,
                    root_section: false,
                    diagnostics: Vec::new(),
                },
            );
            attach(tree, &module, &id);
        }
    }
}

/// Ensure a virtual group exists once, even when several unsupported fragments share it.
fn group(
    tree: &mut ConfiguratorTree,
    owner: &ObjectId,
    kind: CollectionKind,
    parent: &NodeId,
    root_section: bool,
) -> NodeId {
    let id = NodeId::Collection {
        owner: owner.clone(),
        kind,
    };
    if !tree.nodes.contains_key(&id) {
        let key = match kind {
            CollectionKind::Metadata(kind) => format!("tree-collection-{}", kind.as_str()),
            CollectionKind::Common => "tree-collection-common".into(),
            CollectionKind::Modules => "tree-collection-modules".into(),
            CollectionKind::Unsupported => "tree-collection-unsupported".into(),
        };
        tree.nodes.insert(
            id.clone(),
            TreeNode {
                id: id.clone(),
                metadata_kind: None,
                parent: None,
                label: TreeLabel::Key(key),
                children: Vec::new(),
                state: ChildrenState::Empty,
                expanded_by_default: false,
                root_section,
                diagnostics: Vec::new(),
            },
        );
        attach(tree, &id, parent);
    }
    id
}

/// Assign presentation ancestry without changing metadata IDs or logical parents.
fn attach(tree: &mut ConfiguratorTree, child: &NodeId, parent: &NodeId) {
    if let Some(node) = tree.nodes.get_mut(child) {
        node.parent = Some(parent.clone());
    }
    if let Some(node) = tree.nodes.get_mut(parent) {
        node.children.push(child.clone());
    }
}

/// Aggregate emptiness of virtual sections; a real object proves its containing group nonempty.
fn summarize(tree: &mut ConfiguratorTree, id: &NodeId) -> ChildrenState {
    let Some(node) = tree.nodes.get(id) else {
        return ChildrenState::Error;
    };
    let initial = node.state;
    let children = node.children.clone();
    let mut state = initial;
    for child in &children {
        let child_state = summarize(tree, child);
        let evidence = if matches!(child, NodeId::Collection { .. }) {
            child_state
        } else {
            ChildrenState::NonEmpty
        };
        state = match (state, evidence) {
            (ChildrenState::Error, _) | (_, ChildrenState::Error) => ChildrenState::Error,
            (ChildrenState::NonEmpty, _) | (_, ChildrenState::NonEmpty) => ChildrenState::NonEmpty,
            (ChildrenState::Unloaded, _) | (_, ChildrenState::Unloaded) => ChildrenState::Unloaded,
            _ => ChildrenState::Empty,
        };
    }
    if matches!(id, NodeId::Object(_)) && !children.is_empty() && state == ChildrenState::Empty {
        state = ChildrenState::NonEmpty;
    }
    if let Some(node) = tree.nodes.get_mut(id) {
        node.state = state;
    }
    state
}

/// Project predefined items beneath a virtual collection without inventing metadata owners.
pub(super) fn predefined(
    owner: &ObjectId,
    descriptor: &ParsedDescriptor,
) -> Result<ConfiguratorTree, TreeError> {
    let root = NodeId::Collection {
        owner: owner.clone(),
        kind: CollectionKind::Metadata(MetadataKind::PredefinedItem),
    };
    let mut tree = ConfiguratorTree {
        root: root.clone(),
        nodes: BTreeMap::new(),
    };
    group(
        &mut tree,
        owner,
        CollectionKind::Metadata(MetadataKind::PredefinedItem),
        &NodeId::Object(owner.clone()),
        false,
    );
    // The local projection stops at its collection; the session restores the visible owner.
    tree.nodes
        .get_mut(&root)
        .ok_or(TreeError::EmptyDescriptor)?
        .parent = None;
    for object in &descriptor.objects {
        insert_object(
            &mut tree,
            object.metadata.id(),
            object.metadata.name(),
            MetadataKind::PredefinedItem,
            ChildrenState::Empty,
        )?;
    }
    for object in &descriptor.objects {
        let parent = object
            .metadata
            .parent()
            .filter(|id| *id != owner)
            .map_or_else(|| root.clone(), |id| NodeId::Object(id.clone()));
        attach(
            &mut tree,
            &NodeId::Object(object.metadata.id().clone()),
            &parent,
        );
    }
    if !descriptor.diagnostics.is_empty() {
        let node = tree
            .nodes
            .get_mut(&root)
            .ok_or(TreeError::EmptyDescriptor)?;
        node.state = ChildrenState::Error;
        node.diagnostics.clone_from(&descriptor.diagnostics);
    }
    summarize(&mut tree, &root);
    Ok(tree)
}
