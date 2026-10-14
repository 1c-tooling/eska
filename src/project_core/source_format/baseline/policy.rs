//! UUID policy is anchored to accepted ownership, independently of file and descriptor bindings.

use super::{
    AcceptedObject, AcceptedRef, MetadataKind, NativeUuid, Operation, Permission, PermissionReason,
    PropertyName, Reason, State, Support,
};
use std::collections::{BTreeMap, BTreeSet};

/// Only verified baseline decoding constructs these native rules, identities and inherited freezes.
#[derive(Clone)]
pub(super) struct AcceptedPolicy {
    pub(super) native: Option<Support>,
    pub(super) objects: BTreeMap<String, AcceptedObject>,
    pub(super) frozen: BTreeSet<String>,
}

impl AcceptedPolicy {
    /// Compute permission for an accepted native identity and a concrete operation.
    #[must_use]
    pub(super) fn permission(
        &self,
        uuid: &NativeUuid,
        operation: Operation,
        reference: &AcceptedRef,
    ) -> Permission {
        let (state, native_reason) = if self.objects.contains_key(&uuid.key()) {
            self.native
                .as_ref()
                .map_or((State::Unrestricted, Reason::OwnObject), |policy| {
                    policy.object(uuid.as_str())
                })
        } else {
            (State::Unknown, Reason::Unavailable)
        };
        let reason = if state == State::Unknown {
            PermissionReason::UnknownObject
        } else if self.frozen(uuid) {
            PermissionReason::EskaFrozen
        } else if state == State::Locked {
            PermissionReason::NativeRestriction
        } else if matches!(
            operation,
            Operation::Move | Operation::Property(PropertyName::Name)
        ) || (operation == Operation::Create && !self.creation_supported(uuid))
            || (operation == Operation::Delete
                && self.objects.get(&uuid.key()).is_none_or(|object| {
                    !crate::project_core::source_format::child_deletion::supported_object(
                        object.kind,
                    )
                }))
            || (operation == Operation::Rename
                && self.objects.get(&uuid.key()).is_none_or(|object| {
                    !crate::project_core::source_format::child_rename::supported_object(object.kind)
                }))
        {
            PermissionReason::UnsupportedOperation
        } else {
            PermissionReason::Allowed
        };
        Permission {
            allowed: reason == PermissionReason::Allowed,
            uuid: uuid.clone(),
            state,
            native_reason,
            reason,
            baseline: reference.sha256.clone(),
        }
    }

    /// Tabular-section capability follows accepted direct parentage, not an editable root class.
    fn creation_supported(&self, uuid: &NativeUuid) -> bool {
        let Some(object) = self.objects.get(&uuid.key()) else {
            return false;
        };
        if object.kind == MetadataKind::TabularSection {
            object
                .parent
                .as_ref()
                .and_then(|parent| self.objects.get(parent))
                .is_some_and(|parent| {
                    matches!(parent.kind, MetadataKind::Catalog | MetadataKind::Document)
                })
        } else {
            crate::project_core::source_format::child_creation::supported_owner(object.kind)
        }
    }

    /// Additional freeze inherits to owned inline descendants; native supplier rules stay per UUID.
    fn frozen(&self, uuid: &NativeUuid) -> bool {
        let mut current = Some(uuid.key());
        while let Some(key) = current {
            if self.frozen.contains(&key) {
                return true;
            }
            current = self
                .objects
                .get(&key)
                .and_then(|object| object.parent.clone());
        }
        false
    }
}
