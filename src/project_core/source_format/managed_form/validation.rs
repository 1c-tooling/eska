//! Iterative graph validation includes fixed edges without making fixed bindings movable.

use std::collections::{BTreeMap, BTreeSet};

use super::{Error, FormElementKind, FormId, ManagedFormLayout};

/// Native traversal order can change with layout while the actual fixed owner bindings remain identical.
pub(super) fn fixed_bindings(form: &ManagedFormLayout) -> BTreeMap<&FormId, Option<&FormId>> {
    form.fixed
        .iter()
        .map(|slot| (&slot.element, slot.owner.as_ref()))
        .collect()
}

/// All nodes have exactly one incoming edge from the form, ordered layout or their fixed owner.
pub(super) fn validate(form: &ManagedFormLayout) -> Result<(), Error> {
    let parents = parents(form)?;
    validate_names(form)?;
    let mut visited = BTreeSet::new();
    for id in form.elements.keys() {
        let mut path = BTreeSet::new();
        let mut current = Some(id);
        while let Some(id) = current {
            if visited.contains(id) {
                break;
            }
            if !path.insert(id) {
                return Err(failure("source_form_layout_cycle", id));
            }
            current = parents.get(id).and_then(Option::as_ref);
        }
        visited.extend(path);
    }
    for (source, target) in &form.addition_sources {
        let source_kind = form.elements.get(source).map(|node| node.kind);
        if !matches!(
            source_kind,
            Some(
                FormElementKind::SearchStringAddition
                    | FormElementKind::SearchControlAddition
                    | FormElementKind::ViewStatusAddition
            )
        ) || form
            .elements
            .get(target)
            .is_none_or(|node| node.kind != FormElementKind::Table)
        {
            return Err(failure("source_form_reference_missing", source));
        }
    }
    for (source, target) in &form.local_commands {
        if form
            .elements
            .get(source)
            .is_none_or(|node| node.kind != FormElementKind::Button)
            || !form.commands.contains_key(target)
        {
            return Err(failure("source_form_reference_missing", source));
        }
    }
    Ok(())
}

/// IDs and roles validate before traversal, so dangling or duplicate edges cannot conceal an unreachable component.
fn parents(form: &ManagedFormLayout) -> Result<BTreeMap<FormId, Option<FormId>>, Error> {
    let mut parents = BTreeMap::new();
    for id in &form.layout.roots {
        let child = form
            .elements
            .get(id)
            .ok_or_else(|| failure("source_form_element_missing", id))?;
        if !child.kind.visual() {
            return Err(failure("source_form_nesting_invalid", id));
        }
        parent(form, &mut parents, id, None)?;
    }
    for (owner, children) in &form.layout.children {
        let node = form
            .elements
            .get(owner)
            .ok_or_else(|| failure("source_form_element_missing", owner))?;
        if !node.kind.container() {
            return Err(failure("source_form_nesting_invalid", owner));
        }
        for id in children {
            let child = form
                .elements
                .get(id)
                .ok_or_else(|| failure("source_form_element_missing", id))?;
            if !node.kind.accepts(child.kind) {
                return Err(failure("source_form_nesting_invalid", id));
            }
            parent(form, &mut parents, id, Some(owner))?;
        }
    }
    let mut slots = BTreeSet::new();
    let root = FormId::parse("-1")?;
    for slot in &form.fixed {
        let child = form
            .elements
            .get(&slot.element)
            .ok_or_else(|| failure("source_form_element_missing", &slot.element))?;
        let owner = slot
            .owner
            .as_ref()
            .map(|id| {
                form.elements
                    .get(id)
                    .map(|node| node.kind)
                    .ok_or_else(|| failure("source_form_element_missing", id))
            })
            .transpose()?;
        if !child.kind.fixed_owner(owner)
            || (slot.owner.is_none() && slot.element != root)
            || !slots.insert((slot.owner.as_ref(), child.kind))
        {
            return Err(failure("source_form_fixed_slot_invalid", &slot.element));
        }
        parent(form, &mut parents, &slot.element, slot.owner.as_ref())?;
    }
    if !form
        .fixed
        .iter()
        .any(|slot| slot.owner.is_none() && slot.element == root)
    {
        return Err(Error::root("source_form_fixed_slot_invalid"));
    }
    for (id, node) in &form.elements {
        if !parents.contains_key(id) {
            return Err(failure("source_form_element_unreachable", id));
        }
        if id.as_str() == "-1" && node.kind != FormElementKind::AutoCommandBar {
            return Err(failure("source_form_id_invalid", id));
        }
    }
    Ok(parents)
}

/// The actual single parent is checked independently of class nesting and reference namespaces.
fn parent(
    form: &ManagedFormLayout,
    parents: &mut BTreeMap<FormId, Option<FormId>>,
    id: &FormId,
    owner: Option<&FormId>,
) -> Result<(), Error> {
    if !form.elements.contains_key(id) {
        return Err(failure("source_form_element_missing", id));
    }
    if parents.insert(id.clone(), owner.cloned()).is_some() {
        return Err(failure("source_form_parent_duplicate", id));
    }
    Ok(())
}

/// Native fixed names can repeat or be empty; regular element and independent data namespaces stay unambiguous.
pub(super) fn validate_names(form: &ManagedFormLayout) -> Result<(), Error> {
    let fixed: BTreeSet<_> = form.fixed.iter().map(|slot| &slot.element).collect();
    let mut names = BTreeSet::new();
    for (id, element) in &form.elements {
        if !fixed.contains(id)
            && (element.name.is_empty() || !names.insert(element.name.to_lowercase()))
        {
            return Err(failure("source_form_name_ambiguous", id));
        }
    }
    for values in [&form.attributes, &form.commands] {
        let mut names = BTreeSet::new();
        for (id, value) in values {
            if id.as_str() == "-1"
                || value.name.is_empty()
                || !names.insert(value.name.to_lowercase())
            {
                return Err(failure("source_form_name_ambiguous", id));
            }
        }
    }
    Ok(())
}

/// A validated parent graph permits bounded upward walks; memoized contexts avoid quadratic traversal.
pub(super) fn table_contexts(
    form: &ManagedFormLayout,
) -> Result<BTreeMap<FormId, Option<FormId>>, Error> {
    let parents = parents(form)?;
    let mut contexts: BTreeMap<FormId, Option<FormId>> = BTreeMap::new();
    for id in form.elements.keys() {
        let mut path = Vec::new();
        let mut current = parents.get(id).and_then(Option::as_ref);
        let mut context = None;
        while let Some(owner) = current {
            if form
                .elements
                .get(owner)
                .is_some_and(|node| node.kind == FormElementKind::Table)
            {
                context = Some(owner.clone());
                break;
            }
            if let Some(known) = contexts.get(owner) {
                context.clone_from(known);
                break;
            }
            path.push(owner.clone());
            current = parents.get(owner).and_then(Option::as_ref);
        }
        contexts.insert(id.clone(), context.clone());
        for owner in path {
            contexts.insert(owner, context.clone());
        }
    }
    Ok(contexts)
}

/// Machine diagnostics retain the failing native ID without claiming metadata UUID authority.
fn failure(code: &'static str, id: &FormId) -> Error {
    Error::root(code).in_field(id.as_str())
}
