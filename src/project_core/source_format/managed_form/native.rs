//! Native XML has separate UI and value identities; only real collection/slot edges become layout.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use roxmltree::{Document, Node};

use super::{
    Error, FormElement, FormElementKind, FormFixedSlot, FormId, FormLayout, FormNamedValue,
    ManagedFormLayout, NAMESPACE,
};

/// The closed UI profile preserves independent numeric namespaces rather than treating every ID as one identity.
pub(super) fn decode(input: &str) -> Result<ManagedFormLayout, Error> {
    let document = Document::parse(input).map_err(|_| Error::root("source_form_xml_invalid"))?;
    let root = document.root_element();
    if !root.has_tag_name((NAMESPACE, "Form"))
        || root.attribute("version") != Some("2.20")
        || root.attributes().len() != 1
    {
        return Err(unverified());
    }
    let mut form = ManagedFormLayout {
        elements: BTreeMap::new(),
        attributes: values(root, "Attributes", "Attribute")?,
        commands: values(root, "Commands", "Command")?,
        layout: FormLayout::default(),
        fixed: vec![],
        addition_sources: BTreeMap::new(),
        local_commands: BTreeMap::new(),
    };
    let mut nodes = HashMap::new();
    for node in root.descendants().filter(Node::is_element) {
        if node.tag_name().namespace() != Some(NAMESPACE)
            || node.attribute("id").is_none()
            || matches!(node.tag_name().name(), "Attribute" | "Column" | "Command")
        {
            continue;
        }
        let kind = FormElementKind::native(node.tag_name().name()).ok_or_else(unverified)?;
        let id = identity(node)?;
        if form
            .elements
            .insert(
                id.clone(),
                FormElement {
                    kind,
                    name: name(node)?.into(),
                },
            )
            .is_some()
        {
            return Err(duplicate(&id));
        }
        nodes.insert(node.id(), id);
    }
    attach(root, &nodes, &mut form)?;
    validate_columns(root)?;
    super::validation::validate_names(&form)?;
    references(root, &nodes, &mut form)?;
    form.validate()?;
    Ok(form)
}

/// A declared root value collection has independent IDs and names; nested data properties do not become UI.
fn values(
    root: Node<'_, '_>,
    container: &str,
    tag: &str,
) -> Result<BTreeMap<FormId, FormNamedValue>, Error> {
    let mut result = BTreeMap::new();
    for collection in root
        .children()
        .filter(|node| node.has_tag_name((NAMESPACE, container)))
    {
        for node in collection.children().filter(Node::is_element) {
            // Native attribute collections can retain an opaque appearance block outside their ID declarations.
            if container == "Attributes" && node.has_tag_name((NAMESPACE, "ConditionalAppearance"))
            {
                continue;
            }
            if !node.has_tag_name((NAMESPACE, tag)) {
                return Err(unverified());
            }
            let id = identity(node)?;
            if result
                .insert(
                    id.clone(),
                    FormNamedValue {
                        name: name(node)?.into(),
                    },
                )
                .is_some()
            {
                return Err(duplicate(&id));
            }
        }
    }
    Ok(result)
}

/// Exact native parent/slot ownership is separate from each ordered `ChildItems` sequence.
fn attach(
    root: Node<'_, '_>,
    nodes: &HashMap<roxmltree::NodeId, FormId>,
    form: &mut ManagedFormLayout,
) -> Result<(), Error> {
    for node in root.descendants().filter(Node::is_element) {
        if node.has_tag_name((NAMESPACE, "ChildItems")) {
            let owner = node.parent_element().ok_or_else(unverified)?;
            let ids = node
                .children()
                .filter(Node::is_element)
                .map(|child| nodes.get(&child.id()).cloned().ok_or_else(unverified))
                .collect::<Result<Vec<_>, _>>()?;
            if owner == root {
                if root
                    .children()
                    .filter(|child| child.has_tag_name((NAMESPACE, "ChildItems")))
                    .count()
                    != 1
                {
                    return Err(unverified());
                }
                form.layout.roots = ids;
            } else {
                let id = nodes.get(&owner.id()).ok_or_else(unverified)?;
                if form.layout.children.insert(id.clone(), ids).is_some() {
                    return Err(unverified());
                }
            }
        }
        if let Some(id) = nodes.get(&node.id()) {
            let parent = node.parent_element().ok_or_else(unverified)?;
            if parent.has_tag_name((NAMESPACE, "ChildItems")) {
                continue;
            }
            let owner = if parent == root {
                None
            } else {
                Some(nodes.get(&parent.id()).cloned().ok_or_else(unverified)?)
            };
            form.fixed.push(FormFixedSlot {
                owner,
                element: id.clone(),
            });
        }
    }
    Ok(())
}

/// Data columns use their immediate native collection; repeated IDs in different collections remain valid.
fn validate_columns(root: Node<'_, '_>) -> Result<(), Error> {
    for container in root.descendants().filter(Node::is_element) {
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for node in container
            .children()
            .filter(|node| node.has_tag_name((NAMESPACE, "Column")))
        {
            if !container.has_tag_name((NAMESPACE, "Columns"))
                && !container.has_tag_name((NAMESPACE, "AdditionalColumns"))
            {
                return Err(unverified());
            }
            let id = identity(node)?;
            if id.as_str() == "-1" || !ids.insert(id.clone()) {
                return Err(duplicate(&id));
            }
            let name = name(node)?;
            if name.is_empty() || !names.insert(name.to_lowercase()) {
                return Err(Error::root("source_form_name_ambiguous").in_field(id.as_str()));
            }
        }
    }
    Ok(())
}

/// Proven local command/addition names resolve by their own declaration namespaces; numeric/builtin references stay opaque.
fn references(
    root: Node<'_, '_>,
    nodes: &HashMap<roxmltree::NodeId, FormId>,
    form: &mut ManagedFormLayout,
) -> Result<(), Error> {
    let fixed: BTreeSet<_> = form.fixed.iter().map(|slot| &slot.element).collect();
    let elements: BTreeMap<_, _> = form
        .elements
        .iter()
        .filter(|(id, _)| !fixed.contains(id))
        .map(|(id, node)| (node.name.to_lowercase(), id.clone()))
        .collect();
    let commands: BTreeMap<_, _> = form
        .commands
        .iter()
        .map(|(id, node)| (node.name.to_lowercase(), id.clone()))
        .collect();
    for node in root.descendants().filter(Node::is_element) {
        let Some(id) = nodes.get(&node.id()) else {
            continue;
        };
        for property in node.children().filter(Node::is_element) {
            if property.has_tag_name((NAMESPACE, "AdditionSource")) {
                let item = property
                    .children()
                    .find(|node| node.has_tag_name((NAMESPACE, "Item")))
                    .ok_or_else(unverified)?;
                let target = elements
                    .get(&item.text().unwrap_or_default().to_lowercase())
                    .ok_or_else(|| missing(id))?;
                if form
                    .addition_sources
                    .insert(id.clone(), target.clone())
                    .is_some()
                {
                    return Err(unverified());
                }
            }
            if property.has_tag_name((NAMESPACE, "CommandName"))
                && let Some(target) = property
                    .text()
                    .and_then(|text| text.strip_prefix("Form.Command."))
            {
                let target = commands
                    .get(&target.to_lowercase())
                    .ok_or_else(|| missing(id))?;
                if form
                    .local_commands
                    .insert(id.clone(), target.clone())
                    .is_some()
                {
                    return Err(unverified());
                }
            }
        }
    }
    Ok(())
}

/// XML attributes retain native identity; absence is different from an allowed empty fixed name.
fn identity(node: Node<'_, '_>) -> Result<FormId, Error> {
    FormId::parse(node.attribute("id").ok_or_else(unverified)?)
}

/// Definitions preserve their exact display/BSL name without normalizing fixed native aliases.
fn name<'a>(node: Node<'a, '_>) -> Result<&'a str, Error> {
    node.attribute("name").ok_or_else(unverified)
}

/// An unproven XML shape remains available to the existing raw-resource reader, without a typed layout claim.
fn unverified() -> Error {
    Error::root("source_form_profile_unverified")
}

/// Machine diagnostics distinguish native identity collisions from unverified field semantics.
fn duplicate(id: &FormId) -> Error {
    Error::root("source_form_id_duplicate").in_field(id.as_str())
}

/// Unresolved local targets cannot acquire authority from a coincident number in another namespace.
fn missing(id: &FormId) -> Error {
    Error::root("source_form_reference_missing").in_field(id.as_str())
}
