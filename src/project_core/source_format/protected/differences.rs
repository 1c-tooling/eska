//! Diagnostics use accepted identity even when working UUID, path or displayed policy is edited.

use std::collections::BTreeSet;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::super::{
    AcceptedBase, NativeUuid, Operation, PermissionReason, PropertyName, SourcePath, files,
};
use super::{Exclusion, ExclusionAction};
use crate::project_core::support::{Reason, State};

/// Explain each residual difference after supported edits have already been applied.
pub(super) fn collect(
    base: &AcceptedBase,
    path: &SourcePath,
    expected: &Value,
    current: Option<&Value>,
    identity: &NativeUuid,
    diagnostics: &mut Vec<Exclusion>,
) {
    Walker {
        base,
        path,
        diagnostics,
    }
    .walk(
        Some(expected),
        current,
        identity.clone(),
        Operation::Resource,
        None,
        &mut Vec::new(),
    );
}

/// Give unrelated physical paths no permission merely because they look like a metadata file.
pub(super) fn exclusion(
    base: &AcceptedBase,
    path: &SourcePath,
    coordinate: Vec<String>,
    uuid: Option<NativeUuid>,
    operation: Operation,
    action: ExclusionAction,
    code: &str,
) -> Exclusion {
    let (state, native_reason, reason) = uuid.as_ref().map_or(
        (
            State::Unknown,
            Reason::Unavailable,
            PermissionReason::UnknownObject,
        ),
        |uuid| {
            let permission = base.permission(uuid, operation);
            (
                permission.state,
                permission.native_reason,
                if permission.allowed {
                    PermissionReason::UnsupportedOperation
                } else {
                    permission.reason
                },
            )
        },
    );
    let baseline = base.reference().sha256.clone();
    let operation_key = match operation {
        Operation::Property(name) => format!("property:{}", name.as_str()),
        Operation::Module => "module".into(),
        Operation::Resource => "resource".into(),
        Operation::Create => "create".into(),
        Operation::Delete => "delete".into(),
        Operation::Rename => "rename".into(),
        Operation::Move => "move".into(),
        Operation::Reorder => "reorder".into(),
    };
    let action_key = match action {
        ExclusionAction::Restored => "restored",
        ExclusionAction::Excluded => "excluded",
    };
    let mut hash = Sha256::new();
    for field in [
        baseline.as_str(),
        path.as_str(),
        uuid.as_ref().map_or("", NativeUuid::as_str),
        operation_key.as_str(),
        action_key,
        code,
    ]
    .into_iter()
    .chain(coordinate.iter().map(String::as_str))
    {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field);
    }
    let id = files::hex(&hash.finalize());
    Exclusion {
        id,
        code: code.into(),
        path: path.clone(),
        coordinate,
        uuid,
        operation,
        action,
        state,
        native_reason,
        reason,
        baseline,
    }
}

/// Keep the report destination together with the selected policy and canonical source path.
struct Walker<'a> {
    base: &'a AcceptedBase,
    path: &'a SourcePath,
    diagnostics: &'a mut Vec<Exclusion>,
}

impl Walker<'_> {
    /// Track the nearest accepted object and typed property while descending closed JSON-shaped YAML.
    fn walk(
        &mut self,
        expected: Option<&Value>,
        current: Option<&Value>,
        mut uuid: NativeUuid,
        mut operation: Operation,
        native: Option<&str>,
        coordinate: &mut Vec<String>,
    ) {
        if expected == current {
            return;
        }
        let native = expected
            .and_then(|value| value.get("path"))
            .and_then(Value::as_str)
            .or(native);
        if let Some(value) = expected {
            if let Some(identity) = value
                .get("uuid")
                .or_else(|| value.get("descriptor")?.get("object")?.get("uuid"))
                .and_then(Value::as_str)
                && let Ok(identity) = NativeUuid::parse(identity)
            {
                uuid = identity;
            }
            if value.get("kind").and_then(Value::as_str) == Some("typed")
                && let Some(name) = value
                    .get("name")
                    .and_then(Value::as_str)
                    .and_then(PropertyName::from_native)
            {
                operation = Operation::Property(name);
            }
        }
        let scalar_property = (coordinate.last().is_some_and(|field| field == "value")
            || coordinate
                .len()
                .checked_sub(2)
                .and_then(|index| coordinate.get(index))
                .is_some_and(|field| field == "properties"))
            && matches!(operation, Operation::Property(_));
        match (expected, current) {
            (Some(Value::Object(left)), Some(Value::Object(right))) if !scalar_property => {
                let keys: BTreeSet<_> = left.keys().chain(right.keys()).collect();
                for key in keys {
                    coordinate.push(key.clone());
                    let operation = if matches!(key.as_str(), "path" | "uuid") {
                        Operation::Move
                    } else if coordinate.first().is_none_or(|field| field != "form")
                        && coordinate
                            .len()
                            .checked_sub(2)
                            .and_then(|index| coordinate.get(index))
                            .is_some_and(|field| field == "properties")
                        && let Some(name) = PropertyName::from_native(key)
                    {
                        Operation::Property(name)
                    } else {
                        operation
                    };
                    self.walk(
                        left.get(key),
                        right.get(key),
                        uuid.clone(),
                        operation,
                        native,
                        coordinate,
                    );
                    coordinate.pop();
                }
            }
            (Some(Value::Array(left)), Some(Value::Array(right))) if !scalar_property => {
                if self.children(left, right, &uuid, native, coordinate) {
                    return;
                }
                for index in 0..left.len().max(right.len()) {
                    coordinate.push(index.to_string());
                    self.walk(
                        left.get(index),
                        right.get(index),
                        uuid.clone(),
                        operation,
                        native,
                        coordinate,
                    );
                    coordinate.pop();
                }
            }
            _ => self.residual(expected, current, uuid, operation, native, coordinate),
        }
    }

    /// A residual field retains its operation, while a supported value's Name uses namespace authority.
    fn residual(
        &mut self,
        expected: Option<&Value>,
        current: Option<&Value>,
        uuid: NativeUuid,
        operation: Operation,
        native: Option<&str>,
        coordinate: &[String],
    ) {
        let action = if expected.is_none() {
            ExclusionAction::Excluded
        } else {
            ExclusionAction::Restored
        };
        let operation = if coordinate.first().is_some_and(|field| field == "form") {
            Operation::Resource
        } else if current.is_none() {
            Operation::Delete
        } else {
            operation
        };
        let (uuid, operation) = if operation == Operation::Property(PropertyName::Name)
            && let Some(blocking) =
                super::super::child_rename::diagnostic_uuid(self.base, native, &uuid)
        {
            (blocking, Operation::Rename)
        } else {
            (uuid, operation)
        };
        self.diagnostics.push(exclusion(
            self.base,
            self.path,
            coordinate.to_vec(),
            Some(uuid),
            operation,
            action,
            "source_change_excluded",
        ));
    }

    /// Compare existing members by identity so a denied order cannot hide independent property edits.
    fn children(
        &mut self,
        expected: &[Value],
        current: &[Value],
        owner: &NativeUuid,
        native: Option<&str>,
        coordinate: &mut Vec<String>,
    ) -> bool {
        use super::super::child_order;

        let (Some(before), Some(after)) = (
            child_order::from_values(expected),
            child_order::from_values(current),
        ) else {
            return false;
        };
        if !child_order::same_members(&before, &after) {
            if super::super::child_deletion::is_reduction(&before, &after) {
                self.removed_children(
                    expected,
                    current,
                    owner,
                    native,
                    coordinate,
                    (&before, &after),
                );
                return true;
            }
            if super::super::child_creation::is_extension(&before, &after) {
                self.added_children(
                    expected,
                    current,
                    owner,
                    native,
                    coordinate,
                    (&before, &after),
                );
                return true;
            }
            return false;
        }
        if before != after {
            let denied = native
                .and_then(|path| SourcePath::parse(path).ok())
                .and_then(|path| {
                    child_order::denied_permission(self.base, &path, owner, &before, &after).ok()
                })
                .flatten()
                .unwrap_or_else(|| owner.clone());
            self.diagnostics.push(exclusion(
                self.base,
                self.path,
                coordinate.clone(),
                Some(denied),
                Operation::Reorder,
                ExclusionAction::Restored,
                "source_change_excluded",
            ));
        }
        let members: std::collections::BTreeMap<_, _> = after.iter().zip(current).collect();
        for (index, (identity, expected)) in before.iter().zip(expected).enumerate() {
            coordinate.push(index.to_string());
            self.walk(
                Some(expected),
                members.get(identity).copied(),
                owner.clone(),
                Operation::Resource,
                native,
                coordinate,
            );
            coordinate.pop();
        }
        true
    }

    /// Denied additions align retained siblings and report creation authority rather than UUID guesses.
    fn added_children(
        &mut self,
        expected: &[Value],
        current: &[Value],
        owner: &NativeUuid,
        native: Option<&str>,
        coordinate: &mut Vec<String>,
        identities: (
            &[super::super::child_order::Identity],
            &[super::super::child_order::Identity],
        ),
    ) {
        let (before, after) = identities;
        let denied = native
            .and_then(|path| SourcePath::parse(path).ok())
            .and_then(|path| {
                super::super::child_creation::denied_permission(
                    self.base, &path, owner, before, after,
                )
                .ok()
            })
            .flatten()
            .unwrap_or_else(|| owner.clone());
        let retained: std::collections::BTreeMap<_, _> = before.iter().zip(expected).collect();
        for (index, (identity, value)) in after.iter().zip(current).enumerate() {
            coordinate.push(index.to_string());
            if let Some(expected) = retained.get(identity) {
                self.walk(
                    Some(expected),
                    Some(value),
                    owner.clone(),
                    Operation::Resource,
                    native,
                    coordinate,
                );
            } else {
                self.diagnostics.push(exclusion(
                    self.base,
                    self.path,
                    coordinate.clone(),
                    Some(denied.clone()),
                    Operation::Create,
                    ExclusionAction::Excluded,
                    "source_change_excluded",
                ));
            }
            coordinate.pop();
        }
    }

    /// Denied removals align surviving identities and retain one deletion explanation per missing member.
    fn removed_children(
        &mut self,
        expected: &[Value],
        current: &[Value],
        owner: &NativeUuid,
        native: Option<&str>,
        coordinate: &mut Vec<String>,
        identities: (
            &[super::super::child_order::Identity],
            &[super::super::child_order::Identity],
        ),
    ) {
        let (before, after) = identities;
        let denied = native
            .and_then(|path| SourcePath::parse(path).ok())
            .and_then(|path| {
                super::super::child_deletion::denied_permission(
                    self.base, &path, owner, before, after,
                )
                .ok()
            })
            .flatten();
        let retained: std::collections::BTreeMap<_, _> = after.iter().zip(current).collect();
        for (index, (identity, value)) in before.iter().zip(expected).enumerate() {
            coordinate.push(index.to_string());
            if let Some(current) = retained.get(identity) {
                self.walk(
                    Some(value),
                    Some(current),
                    owner.clone(),
                    Operation::Resource,
                    native,
                    coordinate,
                );
            } else {
                let uuid = denied.clone().or_else(|| {
                    let path = SourcePath::parse(native?).ok()?;
                    super::super::child_order::member_uuid(self.base, &path, identity).ok()
                });
                self.diagnostics.push(exclusion(
                    self.base,
                    self.path,
                    coordinate.clone(),
                    uuid,
                    Operation::Delete,
                    ExclusionAction::Restored,
                    "source_change_excluded",
                ));
            }
            coordinate.pop();
        }
    }
}
