//! A declared command supplies its current parameter type to its own native command module.

use crate::{AttributeType, ObjectBelonging};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Multiple selection passes an array of the declared value type to the command handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandParameterUseMode {
    Single,
    Multiple,
}

/// Missing borrowed properties remain absent until a UUID-proven base is composed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommonCommand {
    uuid: Uuid,
    name: String,
    parameter_type: Option<AttributeType>,
    parameter_use: Option<CommandParameterUseMode>,
    object_belonging: ObjectBelonging,
    extends_uuid: Option<Uuid>,
}

impl CommonCommand {
    /// Preserve explicit values and extension ownership without inventing schema defaults.
    pub fn from_parts(
        uuid: Uuid,
        name: String,
        parameter_type: Option<AttributeType>,
        parameter_use: Option<CommandParameterUseMode>,
        object_belonging: ObjectBelonging,
        extends_uuid: Option<Uuid>,
    ) -> Self {
        Self { uuid, name, parameter_type, parameter_use, object_belonging, extends_uuid }
    }

    /// Return the source declaration's own UUID.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// Keep the exact spelling used by the registered module backlink.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// An empty explicit TypeDescription stays Unknown; absence can inherit from a proven base.
    pub fn parameter_type(&self) -> Option<&AttributeType> {
        self.parameter_type.as_ref()
    }

    /// The producer retains missing versus explicit single/multiple use.
    pub fn parameter_use(&self) -> Option<CommandParameterUseMode> {
        self.parameter_use
    }

    /// Ownership determines whether a same-name command may inherit earlier properties.
    pub fn object_belonging(&self) -> ObjectBelonging {
        self.object_belonging
    }

    /// Only the exact original UUID can establish an adopted contribution.
    pub fn adopts(&self, base: &Self) -> bool {
        self.object_belonging == ObjectBelonging::Adopted
            && stdx::case::eq_ignore_case(&self.name, &base.name)
            && self.extends_uuid == Some(base.extends_uuid.unwrap_or(base.uuid))
    }

    /// Replace supplied properties after adoption is proven, retaining unsupplied base values.
    pub fn apply_extension_overlay(&mut self, overlay: &Self) {
        self.uuid = overlay.uuid;
        self.object_belonging = overlay.object_belonging;
        self.extends_uuid = overlay.extends_uuid;
        if let Some(value) = &overlay.parameter_type {
            self.parameter_type = Some(value.clone());
        }
        if let Some(value) = overlay.parameter_use {
            self.parameter_use = Some(value);
        }
    }

    /// Count only owned source payloads; module metadata borrows the canonical Arc.
    pub fn estimated_heap_size(&self) -> usize {
        self.name.capacity()
            + self.parameter_type.as_ref().map_or(0, AttributeType::estimated_heap_size)
    }
}
