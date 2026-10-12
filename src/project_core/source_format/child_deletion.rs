//! Inline removal retains accepted ownership and refuses to guess dependencies in preserved XML.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{
    AcceptedBase, Child, Error, NativeUuid, Operation, SourcePath,
    child_order::{self, Identity},
};
use crate::project_core::MetadataKind;

#[cfg(test)]
mod tests;

/// Only inline enum values have an accepted removal profile without generated types or files.
pub(super) const fn supported_object(kind: MetadataKind) -> bool {
    matches!(kind, MetadataKind::EnumValue)
}

/// A reduction retains a unique ordered subset; creation, transfer and reordering are separate operations.
pub(super) fn is_reduction(before: &[Identity], after: &[Identity]) -> bool {
    let known: BTreeSet<_> = before.iter().collect();
    let current: BTreeSet<_> = after.iter().collect();
    after.len() < before.len()
        && known.len() == before.len()
        && current.len() == after.len()
        && before
            .iter()
            .filter(|identity| current.contains(identity))
            .eq(after.iter())
}

/// Select a permitted reduction after independently merging the retained members' properties.
pub(super) fn select(
    base: &AcceptedBase,
    native: &SourcePath,
    owner: &NativeUuid,
    children: &mut Vec<Child>,
    working: Option<&Vec<Value>>,
) -> Result<(), Error> {
    if !base.package.documents.get(native).is_some_and(|document| {
        document.descriptor.object.kind == MetadataKind::Enum
            && document.descriptor.object.uuid.key() == owner.key()
    }) {
        return Ok(());
    }
    let (Some(before), Some(after)) = (
        child_order::from_children(children),
        working.and_then(|values| child_order::from_values(values)),
    ) else {
        return Ok(());
    };
    if !is_reduction(&before, &after)
        || denied_permission(base, native, owner, &before, &after)?.is_some()
    {
        return Ok(());
    }
    let retained: BTreeSet<_> = after.iter().collect();
    for (identity, child) in before.iter().zip(children.iter()) {
        if retained.contains(identity) {
            continue;
        }
        let Child::Inline { object } = child else {
            return Ok(());
        };
        if super::child_creation::validate_new_node(object).is_err() {
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

/// The owner, deleted identities and every shifted retained member each keep their native restrictions.
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
    let positions: BTreeMap<_, _> = after
        .iter()
        .enumerate()
        .map(|(index, identity)| (identity, index))
        .collect();
    for (index, identity) in before.iter().enumerate() {
        let uuid = child_order::member_uuid(base, native, identity)?;
        let operation = match positions.get(identity) {
            None => Operation::Delete,
            Some(position) if *position != index => Operation::Reorder,
            Some(_) => continue,
        };
        if !base.permission(&uuid, operation).allowed {
            return Ok(Some(uuid));
        }
    }
    Ok(None)
}
