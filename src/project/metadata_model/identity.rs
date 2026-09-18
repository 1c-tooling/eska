//! Logical identifiers retain the existing escaping and never contain filesystem paths.

use std::{fmt, sync::Arc};

use super::MetadataKind;
use crate::project::ProjectName;

/// Stable readable identity built from the logical metadata hierarchy.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectId(Arc<str>);

impl serde::Serialize for ObjectId {
    /// Cache serialization retains the existing string identity, not the shared allocation.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for ObjectId {
    /// Restore an owned identity without borrowing the cache input buffer.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        Ok(Self(text.into()))
    }
}

impl ObjectId {
    /// Return the stable machine-facing hierarchical identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return the logical parent encoded in this identity, without interpreting a source path.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        self.0
            .rsplit_once('/')
            .map(|(parent, _)| Self(Arc::from(parent)))
    }

    /// Build an identity after the caller has validated the object's kind and name.
    pub(crate) fn from_parts(parent: Option<&Self>, kind: &str, name: &str) -> Self {
        let escaped = name
            .replace('%', "%25")
            .replace('/', "%2F")
            .replace(':', "%3A");
        let segment = format!("{kind}:{escaped}");
        Self(
            parent
                .map_or_else(|| segment.clone(), |parent| format!("{parent}/{segment}"))
                .into(),
        )
    }
}

impl fmt::Display for ObjectId {
    /// Write the existing CLI identifier without adding project or tree-node prefixes.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Module role within its owner, separate from a real metadata object's identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ModuleRole {
    Module,
    Object,
    Manager,
    RecordSet,
    ValueManager,
    ManagedApplication,
    OrdinaryApplication,
    Session,
    ExternalConnection,
    Command,
}

impl ModuleRole {
    /// Return a locale-independent role key, not a source filename.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Object => "object",
            Self::Manager => "manager",
            Self::RecordSet => "record-set",
            Self::ValueManager => "value-manager",
            Self::ManagedApplication => "managed-application",
            Self::OrdinaryApplication => "ordinary-application",
            Self::Session => "session",
            Self::ExternalConnection => "external-connection",
            Self::Command => "command",
        }
    }
}

/// Virtual collections do not masquerade as metadata objects.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CollectionKind {
    Metadata(MetadataKind),
    Modules,
    Common,
    /// Unsupported or malformed fragments are visible rather than silently discarded.
    Unsupported,
}

/// Identity of one tree node within a project; display text is never a key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NodeId {
    Object(ObjectId),
    Module {
        owner: ObjectId,
        role: ModuleRole,
    },
    Collection {
        owner: ObjectId,
        kind: CollectionKind,
    },
}

/// A project within one discovered eska context, stable when that context is moved.
///
/// Independent discovery contexts have separate namespaces. A future IDE session
/// opening multiple contexts must keep their namespace separate as well.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProjectScope {
    Standalone,
    Member(ProjectName),
}

/// A tree node explicitly scoped to its project, avoiding member-name collisions.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ScopedNodeId {
    pub project: ProjectScope,
    pub node: NodeId,
}
