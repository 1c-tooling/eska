//! New declared subtrees derive authority from an existing permitted parent, never their own YAML.

use std::collections::BTreeSet;

use super::super::{AcceptedBase, AcceptedObject, document_ownership};
use crate::project_core::{
    source_format::{Error, NativeUuid, Operation, export::Package, file_identity},
    support::{Reason, State},
};

/// New inline identities are valid only inside a complete new declared owner subtree.
pub(super) fn validate(base: &AcceptedBase, current: &Package) -> Result<BTreeSet<String>, Error> {
    let objects = document_ownership(&current.documents)?;
    validate_fresh_identities(base, current)?;
    let declared = current
        .documents
        .values()
        .map(|document| document.descriptor.object.uuid.key())
        .collect::<BTreeSet<_>>();
    let mut added = BTreeSet::new();
    for (uuid, object) in &objects {
        if base.support.objects.contains_key(uuid) {
            continue;
        }
        let mut owner = uuid.as_str();
        while !declared.contains(owner) {
            owner = objects
                .get(owner)
                .and_then(|object| object.parent.as_deref())
                .ok_or_else(|| Error::root("source_identity_mismatch"))?;
        }
        if base.support.objects.contains_key(owner) {
            return Err(Error::root("source_import_mapping_unsupported"));
        }
        validate_parent(base, &objects, object)?;
        let (state, reason) = base
            .support
            .native
            .as_ref()
            .map_or((State::Unrestricted, Reason::OwnObject), |policy| {
                policy.object(uuid)
            });
        if state != State::Unrestricted || reason != Reason::OwnObject {
            return Err(Error::root("source_import_structure_denied").in_field(uuid.clone()));
        }
        added.insert(uuid.clone());
    }
    let before = file_identity::files(&base.package)?;
    for identity in file_identity::files(current)?.keys() {
        if !before.contains_key(identity)
            && file_identity::uuid(identity).is_none_or(|uuid| !added.contains(&uuid.key()))
        {
            return Err(Error::root("source_import_mapping_unsupported"));
        }
    }
    Ok(added)
}

/// Fresh owners cannot reuse a removed accepted object's UUID or a modeled service identity.
fn validate_fresh_identities(base: &AcceptedBase, current: &Package) -> Result<(), Error> {
    let mut original = BTreeSet::new();
    for document in base.package.documents.values() {
        super::super::super::composition::identities(&document.descriptor.object, &mut original)?;
    }
    for document in current.documents.values() {
        if base
            .support
            .objects
            .contains_key(&document.descriptor.object.uuid.key())
        {
            continue;
        }
        if base.package.documents.contains_key(&document.path) {
            return Err(Error::root("source_import_mapping_unsupported"));
        }
        let mut identities = BTreeSet::new();
        super::super::super::composition::identities(&document.descriptor.object, &mut identities)?;
        if identities.iter().any(|uuid| original.contains(uuid)) {
            return Err(Error::root("source_import_mapping_unsupported"));
        }
    }
    Ok(())
}

/// The nearest accepted ancestor must permit a structural edit and retain its inherited freeze.
fn validate_parent(
    base: &AcceptedBase,
    objects: &std::collections::BTreeMap<String, AcceptedObject>,
    object: &AcceptedObject,
) -> Result<(), Error> {
    let mut parent = object.parent.as_deref();
    while let Some(uuid) = parent {
        if base.support.objects.contains_key(uuid) {
            let permission = base.permission(&NativeUuid::parse(uuid)?, Operation::Resource);
            if !permission.allowed {
                return Err(Error::root("source_import_structure_denied").in_field(uuid));
            }
            return Ok(());
        }
        parent = objects
            .get(uuid)
            .and_then(|object| object.parent.as_deref());
    }
    Err(Error::root("source_import_mapping_unsupported"))
}
