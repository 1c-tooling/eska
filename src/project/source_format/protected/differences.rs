//! Diagnostics use accepted identity even when working UUID, path or displayed policy is edited.

use std::collections::BTreeSet;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::super::{
    AcceptedBase, NativeUuid, Operation, PermissionReason, PropertyName, SourcePath, files,
};
use super::{Exclusion, ExclusionAction};
use crate::project::support::{Reason, State};

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
        Operation::Property(PropertyName::Name) => "property:Name",
        Operation::Property(PropertyName::Comment) => "property:Comment",
        Operation::Property(PropertyName::Version) => "property:Version",
        Operation::Property(PropertyName::Synonym) => "property:Synonym",
        Operation::Module => "module",
        Operation::Resource => "resource",
        Operation::Create => "create",
        Operation::Delete => "delete",
        Operation::Rename => "rename",
        Operation::Move => "move",
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
        operation_key,
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
        coordinate: &mut Vec<String>,
    ) {
        if expected == current {
            return;
        }
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
                    .and_then(property_name)
            {
                operation = Operation::Property(name);
            }
        }
        let scalar_property = coordinate.last().is_some_and(|field| field == "value")
            && matches!(operation, Operation::Property(_));
        match (expected, current) {
            (Some(Value::Object(left)), Some(Value::Object(right))) if !scalar_property => {
                let keys: BTreeSet<_> = left.keys().chain(right.keys()).collect();
                for key in keys {
                    coordinate.push(key.clone());
                    let operation = if matches!(key.as_str(), "path" | "uuid") {
                        Operation::Move
                    } else {
                        operation
                    };
                    self.walk(
                        left.get(key),
                        right.get(key),
                        uuid.clone(),
                        operation,
                        coordinate,
                    );
                    coordinate.pop();
                }
            }
            (Some(Value::Array(left)), Some(Value::Array(right))) if !scalar_property => {
                for index in 0..left.len().max(right.len()) {
                    coordinate.push(index.to_string());
                    self.walk(
                        left.get(index),
                        right.get(index),
                        uuid.clone(),
                        operation,
                        coordinate,
                    );
                    coordinate.pop();
                }
            }
            _ => {
                let action = if expected.is_none() {
                    ExclusionAction::Excluded
                } else {
                    ExclusionAction::Restored
                };
                let operation = if current.is_none() {
                    Operation::Delete
                } else {
                    operation
                };
                self.diagnostics.push(exclusion(
                    self.base,
                    self.path,
                    coordinate.clone(),
                    Some(uuid),
                    operation,
                    action,
                    "source_change_excluded",
                ));
            }
        }
    }
}

/// Match exact schema names; locale never changes a property coordinate or permission.
fn property_name(name: &str) -> Option<PropertyName> {
    match name {
        "Name" => Some(PropertyName::Name),
        "Comment" => Some(PropertyName::Comment),
        "Version" => Some(PropertyName::Version),
        "Synonym" => Some(PropertyName::Synonym),
        _ => None,
    }
}
