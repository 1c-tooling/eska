//! Configured values exposed through the platform's SessionParameters container.

use crate::{AttributeType, ObjectBelonging};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A session value is neither a metadata manager nor an ordinary query parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionParameter {
    uuid: Uuid,
    name: String,
    attribute_type: AttributeType,
    object_belonging: ObjectBelonging,
    extends_uuid: Option<Uuid>,
}

impl SessionParameter {
    /// Preserve the producer's exact identity, type and extension ownership without defaults.
    pub fn from_parts(
        uuid: Uuid,
        name: String,
        attribute_type: AttributeType,
        object_belonging: ObjectBelonging,
        extends_uuid: Option<Uuid>,
    ) -> Self {
        Self { uuid, name, attribute_type, object_belonging, extends_uuid }
    }

    /// Return the descriptor's own identity, including an adopted declaration's UUID.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// Return the declared BSL member name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the current value type, preserving composite alternatives and qualifiers.
    pub fn attribute_type(&self) -> &AttributeType {
        &self.attribute_type
    }

    /// Return the declared extension relationship.
    pub fn object_belonging(&self) -> ObjectBelonging {
        self.object_belonging
    }

    /// Return the explicit base identity, never a name-derived substitute.
    pub fn extends_uuid(&self) -> Option<Uuid> {
        self.extends_uuid
    }

    /// An overlay is valid only for the same name and the proven original object identity.
    pub fn adopts(&self, base: &Self) -> bool {
        self.object_belonging == ObjectBelonging::Adopted
            && stdx::case::eq_ignore_case(&self.name, &base.name)
            && self.extends_uuid == Some(base.extends_uuid.unwrap_or(base.uuid))
    }

    /// Count owned strings and type payload once at the source input or parsed memo.
    pub fn estimated_heap_size(&self) -> usize {
        self.name.capacity() + self.attribute_type.estimated_heap_size()
    }
}
