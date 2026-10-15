//! Functional-option identity and storage remain separate from its runtime value and interface content.

use crate::{MdoType, ObjectBelonging};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// This admitted storage profile covers constants, catalog attributes and information-register resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionalOptionLocation {
    Empty,
    Constant { name: String },
    CatalogAttribute { catalog: String, attribute: String },
    InformationRegisterResource { register: String, resource: String },
}

impl FunctionalOptionLocation {
    /// Return the one native owner whose current type supplies this stored value.
    pub fn owner(&self) -> Option<(MdoType, &str)> {
        match self {
            Self::Empty => None,
            Self::Constant { name } => Some((MdoType::Constant, name)),
            Self::CatalogAttribute { catalog, .. } => Some((MdoType::Catalog, catalog)),
            Self::InformationRegisterResource { register, .. } => {
                Some((MdoType::InformationRegister, register))
            }
        }
    }

    /// Storage variants own only their configured owner and optional member names.
    fn estimated_heap_size(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Constant { name } => name.capacity(),
            Self::CatalogAttribute { catalog, attribute } => {
                catalog.capacity() + attribute.capacity()
            }
            Self::InformationRegisterResource { register, resource } => {
                register.capacity() + resource.capacity()
            }
        }
    }
}

/// Current metadata describes where a value is stored without evaluating that value or hiding code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionalOption {
    uuid: Uuid,
    name: String,
    location: Option<FunctionalOptionLocation>,
    privileged_get: Option<bool>,
    content: Option<Vec<String>>,
    belonging: ObjectBelonging,
    extends: Option<Uuid>,
}

impl FunctionalOption {
    /// Retain absent borrowed properties until the original UUID has been proven.
    pub fn from_parts(
        uuid: Uuid,
        name: String,
        location: Option<FunctionalOptionLocation>,
        privileged_get: Option<bool>,
        content: Option<Vec<String>>,
        belonging: ObjectBelonging,
        extends: Option<Uuid>,
    ) -> Self {
        Self { uuid, name, location, privileged_get, content, belonging, extends }
    }

    /// Keep the source UUID independent of filenames and display names.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }
    /// Preserve the configured name used by the native global function.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The current storage declaration controls the result type, never a presumed Boolean default.
    pub fn location(&self) -> Option<&FunctionalOptionLocation> {
        self.location.as_ref()
    }
    /// Runtime privilege policy is retained without executing privileged reads.
    pub fn privileged_get(&self) -> Option<bool> {
        self.privileged_get
    }
    /// Interface membership does not alter which BSL bodies are analyzed.
    pub fn content(&self) -> Option<&[String]> {
        self.content.as_deref()
    }
    /// A borrowed declaration cannot manufacture its absent original.
    pub fn object_belonging(&self) -> ObjectBelonging {
        self.belonging
    }

    /// This profile permits content additions with inherited or identical storage and exact original identity.
    pub fn adopts(&self, base: &Self) -> bool {
        self.belonging == ObjectBelonging::Adopted
            && stdx::case::eq_ignore_case(&self.name, &base.name)
            && self.extends == Some(base.extends.unwrap_or(base.uuid))
            && self.location.as_ref().is_none_or(|value| Some(value) == base.location())
    }

    /// A proven contribution adds content while preserving the original storage declaration.
    pub fn apply_extension_overlay(&mut self, overlay: &Self) {
        self.uuid = overlay.uuid;
        self.belonging = overlay.belonging;
        self.extends = overlay.extends;
        if overlay.privileged_get.is_some() {
            self.privileged_get = overlay.privileged_get;
        }
        if let Some(content) = &overlay.content {
            let previous = self.content.get_or_insert_with(Vec::new);
            for path in content {
                if !previous.iter().any(|value| stdx::case::eq_ignore_case(value, path)) {
                    previous.push(path.clone());
                }
            }
        }
    }

    /// Count owned declarations once at their canonical input rather than at every dependent query.
    pub fn estimated_heap_size(&self) -> usize {
        self.name.capacity()
            + self.location.as_ref().map_or(0, FunctionalOptionLocation::estimated_heap_size)
            + self.content.as_ref().map_or(0, |values| {
                values.capacity() * std::mem::size_of::<String>()
                    + values.iter().map(String::capacity).sum::<usize>()
            })
    }
}
