//! Explicit common-attribute membership contributes a real typed field to its selected objects.

use crate::{AttributeType, MdoType, ObjectBelonging};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A target's explicit setting is distinct from the schema-wide automatic-use setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommonAttributeUse {
    Auto,
    Use,
    DontUse,
}

/// A native readable metadata target retains its family, exact name and explicit use setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommonAttributeTarget {
    pub kind: MdoType,
    pub name: String,
    pub usage: CommonAttributeUse,
}

/// An admitted ordinary common attribute has explicit content and automatic use disabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommonAttribute {
    uuid: Uuid,
    name: String,
    attribute_type: AttributeType,
    targets: Vec<CommonAttributeTarget>,
    object_belonging: ObjectBelonging,
    extends_uuid: Option<Uuid>,
}

impl CommonAttribute {
    /// The producer validates ordinary non-separated membership before supplying the current value.
    pub fn from_parts(
        uuid: Uuid,
        name: String,
        attribute_type: AttributeType,
        targets: Vec<CommonAttributeTarget>,
        object_belonging: ObjectBelonging,
        extends_uuid: Option<Uuid>,
    ) -> Self {
        Self { uuid, name, attribute_type, targets, object_belonging, extends_uuid }
    }

    /// Return the current declaration's exact native identity.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// Return the original field spelling without folding or localization.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Preserve the declared type, including composite alternatives and qualifiers.
    pub fn attribute_type(&self) -> &AttributeType {
        &self.attribute_type
    }

    /// Preserve explicit content order, including targets whose use is disabled.
    pub fn targets(&self) -> &[CommonAttributeTarget] {
        &self.targets
    }

    /// Return the checked extension ownership of this declaration.
    pub fn object_belonging(&self) -> ObjectBelonging {
        self.object_belonging
    }

    /// Return an explicit borrowed identity, independently of the current display name.
    pub fn extends_uuid(&self) -> Option<Uuid> {
        self.extends_uuid
    }

    /// An absent target or Auto does not add a field when automatic use is explicitly disabled.
    pub fn applies_to(&self, kind: MdoType, name: &str) -> bool {
        self.targets.iter().any(|target| {
            target.kind == kind
                && stdx::case::eq_ignore_case(&target.name, name)
                && target.usage == CommonAttributeUse::Use
        })
    }

    /// A borrowed contribution must prove the original UUID as well as the shared member name.
    pub fn adopts(&self, base: &Self) -> bool {
        self.object_belonging == ObjectBelonging::Adopted
            && stdx::case::eq_ignore_case(&self.name, &base.name)
            && self.extends_uuid == Some(base.extends_uuid.unwrap_or(base.uuid))
    }

    /// A verified extension replaces supplied target settings while retaining untouched base targets.
    pub fn apply_extension_overlay(&mut self, overlay: &Self) {
        self.attribute_type = overlay.attribute_type.clone();
        for target in &overlay.targets {
            if let Some(previous) = self.targets.iter_mut().find(|previous| {
                previous.kind == target.kind
                    && stdx::case::eq_ignore_case(&previous.name, &target.name)
            }) {
                *previous = target.clone();
            } else {
                self.targets.push(target.clone());
            }
        }
    }

    /// Canonical source inputs own these strings and types; resolver results borrow their Arcs.
    pub fn estimated_heap_size(&self) -> usize {
        self.name.capacity()
            + self.attribute_type.estimated_heap_size()
            + self.targets.capacity() * std::mem::size_of::<CommonAttributeTarget>()
            + self.targets.iter().map(|target| target.name.capacity()).sum::<usize>()
    }
}
