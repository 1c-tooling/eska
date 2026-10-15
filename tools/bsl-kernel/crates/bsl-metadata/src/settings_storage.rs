//! Settings storages own a manager namespace rather than records or configured value fields.

use crate::ObjectBelonging;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Current identity selects the native storage manager and its separately enrolled module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsStorage {
    uuid: Uuid,
    name: String,
    belonging: ObjectBelonging,
    extends: Option<Uuid>,
}

impl SettingsStorage {
    /// Preserve the source identity without manufacturing MDO attributes or runtime stored values.
    pub fn from_parts(
        uuid: Uuid,
        name: String,
        belonging: ObjectBelonging,
        extends: Option<Uuid>,
    ) -> Self {
        Self { uuid, name, belonging, extends }
    }

    /// The source UUID remains independent of its physical descriptor and module paths.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// The configured name is the member of the platform settings-storage container.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Borrowed managers require their proven original rather than an own-manager fallback.
    pub fn object_belonging(&self) -> ObjectBelonging {
        self.belonging
    }

    /// Contributions require the original UUID and native case-insensitive configured name.
    pub fn adopts(&self, base: &Self) -> bool {
        self.belonging == ObjectBelonging::Adopted
            && stdx::case::eq_ignore_case(&self.name, &base.name)
            && self.extends == Some(base.extends.unwrap_or(base.uuid))
    }

    /// A proven contribution replaces identity bookkeeping while module composition stays separate.
    pub fn apply_extension_overlay(&mut self, overlay: &Self) {
        self.uuid = overlay.uuid;
        self.belonging = overlay.belonging;
        self.extends = overlay.extends;
    }

    /// Canonical accounting owns only the configured name allocation.
    pub fn estimated_heap_size(&self) -> usize {
        self.name.capacity()
    }
}
