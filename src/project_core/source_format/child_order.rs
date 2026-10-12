//! Collection order is editable only when every member retains its accepted identity and slot kind.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::project_core::MetadataKind;

use super::{
    AcceptedBase, Child, Error, NativeUuid, Operation, SourcePath, composition::reference_path,
};

#[cfg(test)]
mod tests;

/// Inline identity and external declaration have different authorities, even for the same class.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Identity {
    Inline(String),
    Reference(MetadataKind, String),
}

/// Unknown children keep their native slots; interpreting their position would require a new rule.
pub(super) fn from_children(children: &[Child]) -> Option<Vec<Identity>> {
    children
        .iter()
        .map(|child| match child {
            Child::Inline { object } => Some(Identity::Inline(object.uuid.key())),
            Child::Reference {
                metadata_kind,
                name,
            } => Some(Identity::Reference(*metadata_kind, name.clone())),
            Child::Preserved { .. } => None,
        })
        .collect()
}

/// Read identity only; caller restores untrusted class, namespace and native data independently.
pub(super) fn from_values(children: &[Value]) -> Option<Vec<Identity>> {
    children
        .iter()
        .map(|child| match child.get("kind")?.as_str()? {
            "inline" => Some(Identity::Inline(
                NativeUuid::parse(child.get("object")?.get("uuid")?.as_str()?)
                    .ok()?
                    .key(),
            )),
            "reference" => Some(Identity::Reference(
                serde_json::from_value(child.get("metadata_kind")?.clone()).ok()?,
                child.get("name")?.as_str()?.to_owned(),
            )),
            _ => None,
        })
        .collect()
}

/// Reordering never substitutes a member, duplicates its identity, or infers a new declaration.
pub(super) fn same_members(before: &[Identity], after: &[Identity]) -> bool {
    let expected: BTreeSet<_> = before.iter().collect();
    let current: BTreeSet<_> = after.iter().collect();
    before.len() == after.len()
        && expected.len() == before.len()
        && current.len() == after.len()
        && expected == current
}

/// Select a permitted order after per-member values have been merged against the pinned base.
pub(super) fn select(
    base: &AcceptedBase,
    native: &SourcePath,
    owner: &NativeUuid,
    children: &mut Vec<Child>,
    working: Option<&Vec<Value>>,
) -> Result<(), Error> {
    let Some(before) = from_children(children) else {
        return Ok(());
    };
    let Some(after) = working.and_then(|values| from_values(values)) else {
        return Ok(());
    };
    if before == after
        || !same_members(&before, &after)
        || denied_permission(base, native, owner, &before, &after)?.is_some()
    {
        return Ok(());
    }
    // Native class positions remain fixed: moving a form into an attribute slot is not ordering.
    let slots: BTreeMap<_, _> = before
        .iter()
        .zip(children.iter())
        .map(|(identity, child)| {
            let kind = match child {
                Child::Inline { object } => (object.kind, true),
                Child::Reference { metadata_kind, .. } => (*metadata_kind, false),
                Child::Preserved { .. } => {
                    return Err(Error::root("source_preserved_slot_invalid"));
                }
            };
            Ok((identity.clone(), kind))
        })
        .collect::<Result<_, Error>>()?;
    for (previous, identity) in before.iter().zip(&after) {
        if slots.get(previous) != slots.get(identity) {
            return Ok(());
        }
    }
    let mut members: BTreeMap<_, _> = before.into_iter().zip(std::mem::take(children)).collect();
    for identity in after {
        children.push(
            members
                .remove(&identity)
                .ok_or_else(|| Error::root("source_identity_ambiguous"))?,
        );
    }
    Ok(())
}

/// Owner restrictions and every displaced member are checked independently; stationary freeze stays put.
pub(super) fn denied_permission(
    base: &AcceptedBase,
    native: &SourcePath,
    owner: &NativeUuid,
    before: &[Identity],
    after: &[Identity],
) -> Result<Option<NativeUuid>, Error> {
    if !base.permission(owner, Operation::Reorder).allowed {
        return Ok(Some(owner.clone()));
    }
    for (previous, current) in before.iter().zip(after) {
        if previous != current {
            let uuid = member_uuid(base, native, current)?;
            if !base.permission(&uuid, Operation::Reorder).allowed {
                return Ok(Some(uuid));
            }
        }
    }
    Ok(None)
}

/// An external child's permission comes from its declared descriptor, not the containing folder.
pub(super) fn member_uuid(
    base: &AcceptedBase,
    native: &SourcePath,
    identity: &Identity,
) -> Result<NativeUuid, Error> {
    match identity {
        Identity::Inline(uuid) => NativeUuid::parse(uuid),
        Identity::Reference(kind, name) => {
            let parent = if native.as_str() == "Configuration.xml" {
                ""
            } else {
                native
                    .as_str()
                    .strip_suffix(".xml")
                    .ok_or_else(|| Error::root("source_path_invalid"))?
            };
            let path = reference_path(parent, *kind, name)?;
            base.package
                .documents
                .get(&path)
                .map(|document| document.descriptor.object.uuid.clone())
                .ok_or_else(|| Error::root("source_baseline_corrupt"))
        }
    }
}
