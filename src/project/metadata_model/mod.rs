//! Path-independent metadata values and identities. Filesystem resolution lives separately.

mod identity;
mod kind;
mod project;
mod value;

pub use identity::{CollectionKind, ModuleRole, NodeId, ObjectId, ProjectScope, ScopedNodeId};
pub use kind::{MetadataKind, UnknownMetadataKind};
pub use project::{MetadataProject, MetadataProjectError};
pub use value::{LocalizedText, MetadataProperty, MetadataValue, PropertyKey, ValueIssue};

/// One real metadata object, without XML buffers, source paths or presentation labels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataObject {
    pub(crate) id: ObjectId,
    pub(crate) kind: MetadataKind,
    pub(crate) name: String,
    pub(crate) uuid: String,
    pub(crate) parent: Option<ObjectId>,
}

impl MetadataObject {
    /// Construct a logical object; the UUID is auxiliary and need not be unique.
    ///
    /// # Errors
    /// Returns [`EmptyMetadataName`] for an empty logical name.
    pub fn new(
        kind: MetadataKind,
        name: String,
        uuid: String,
        parent: Option<ObjectId>,
    ) -> Result<Self, EmptyMetadataName> {
        if name.is_empty() {
            return Err(EmptyMetadataName);
        }
        Ok(Self {
            id: ObjectId::from_parts(parent.as_ref(), kind.as_str(), &name),
            kind,
            name,
            uuid,
            parent,
        })
    }

    /// Return the existing hierarchical object identity.
    #[must_use]
    pub const fn id(&self) -> &ObjectId {
        &self.id
    }

    /// Return the machine-facing metadata kind.
    #[must_use]
    pub const fn kind(&self) -> MetadataKind {
        self.kind
    }

    /// Return the logical name used in identity, not a localized synonym.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the auxiliary Designer UUID.
    #[must_use]
    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    /// Return the containing real object; virtual collections are not parents here.
    #[must_use]
    pub const fn parent(&self) -> Option<&ObjectId> {
        self.parent.as_ref()
    }
}

/// An object cannot have an empty name in the logical hierarchy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmptyMetadataName;

#[cfg(test)]
mod tests;
