//! Public semantic events, completeness information and structured errors.

use super::ChangeStage;
use crate::{project::object_model::ObjectModelError, vcs::repository::Error as RepositoryError};
use gix::bstr::{BStr, BString, ByteSlice};
use std::{fmt, path::PathBuf};

/// Stable semantic event emitted from one pair of Designer source snapshots.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SemanticEvent {
    pub(super) kind: SemanticEventKind,
    pub(super) stage: ChangeStage,
    pub(super) object: SemanticObject,
    pub(super) member: Option<String>,
    pub(super) location: Option<SourceLocation>,
    pub(super) path: BString,
}

impl SemanticEvent {
    /// Return the stable event kind.
    #[must_use]
    pub const fn kind(&self) -> SemanticEventKind {
        self.kind
    }

    /// Return the comparison edge that produced the event.
    #[must_use]
    pub const fn stage(&self) -> ChangeStage {
        self.stage
    }

    /// Return the affected logical object.
    #[must_use]
    pub const fn object(&self) -> &SemanticObject {
        &self.object
    }

    /// Return the affected procedure or function name when applicable.
    #[must_use]
    pub fn member(&self) -> Option<&str> {
        self.member.as_deref()
    }

    /// Return the one-based BSL declaration coordinates for a procedure or function.
    #[must_use]
    pub const fn location(&self) -> Option<SourceLocation> {
        self.location
    }

    /// Return the original project-relative source path.
    #[must_use]
    pub fn path(&self) -> &BStr {
        self.path.as_bstr()
    }
}

/// One-based source coordinates used only by the human semantic presentation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

/// Locale-independent classification of reliable semantic changes.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SemanticEventKind {
    ObjectAdded,
    ObjectRemoved,
    ObjectChanged,
    ModuleChanged,
    MethodAdded,
    MethodRemoved,
    MethodChanged,
    FunctionAdded,
    FunctionRemoved,
    FunctionChanged,
    FormChanged,
    MetadataAttributeChanged,
}

impl SemanticEventKind {
    /// Return the stable JSON and raw-output value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ObjectAdded => "object_added",
            Self::ObjectRemoved => "object_removed",
            Self::ObjectChanged => "object_changed",
            Self::ModuleChanged => "module_changed",
            Self::MethodAdded => "method_added",
            Self::MethodRemoved => "method_removed",
            Self::MethodChanged => "method_changed",
            Self::FunctionAdded => "function_added",
            Self::FunctionRemoved => "function_removed",
            Self::FunctionChanged => "function_changed",
            Self::FormChanged => "form_changed",
            Self::MetadataAttributeChanged => "metadata_attribute_changed",
        }
    }
}

/// Stable identity fields shared by human and machine presentations.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SemanticObject {
    pub(super) id: String,
    pub(super) metadata_type: &'static str,
    pub(super) name: String,
}

impl SemanticObject {
    /// Return the readable hierarchical identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Return the stable Designer metadata type.
    #[must_use]
    pub const fn metadata_type(&self) -> &'static str {
        self.metadata_type
    }

    /// Return the Designer object name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Deterministically ordered semantic events for one comparison.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticDiff {
    pub(super) events: Vec<SemanticEvent>,
    pub(super) fallbacks: Vec<SemanticFallback>,
}

impl SemanticDiff {
    /// Return semantic events sorted by kind, stage, object, member, location and path.
    #[must_use]
    pub fn events(&self) -> &[SemanticEvent] {
        &self.events
    }

    /// Return whether no reliable semantic event was detected.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Return paths whose semantic details were conservatively reduced to file-level data.
    #[must_use]
    pub fn fallbacks(&self) -> &[SemanticFallback] {
        &self.fallbacks
    }

    /// Return whether every changed object was analyzed at the most specific reliable level.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.fallbacks.is_empty()
    }
}

/// Stable reason why one changed path needs file-level fallback presentation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SemanticFallbackReason {
    DescriptorParse,
    RoutineParse,
    OwnerInferred,
    OwnerUnresolved,
}

impl SemanticFallbackReason {
    /// Return the locale-independent machine code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DescriptorParse => "descriptor-parse",
            Self::RoutineParse => "routine-parse",
            Self::OwnerInferred => "owner-inferred",
            Self::OwnerUnresolved => "owner-unresolved",
        }
    }
}

/// One non-fatal semantic-analysis fallback for an exact comparison edge and path.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SemanticFallback {
    pub(super) reason: SemanticFallbackReason,
    pub(super) stage: ChangeStage,
    pub(super) path: BString,
}

impl SemanticFallback {
    /// Return the stable fallback reason.
    #[must_use]
    pub const fn reason(&self) -> SemanticFallbackReason {
        self.reason
    }

    /// Return the comparison edge whose semantic analysis was incomplete.
    #[must_use]
    pub const fn stage(&self) -> ChangeStage {
        self.stage
    }

    /// Return the exact project-relative path requiring file-level presentation.
    #[must_use]
    pub fn path(&self) -> &BStr {
        self.path.as_bstr()
    }
}

/// Failures while loading exact before/after snapshots for semantic analysis.
#[derive(Debug)]
pub enum SemanticDiffError {
    Repository(RepositoryError),
    ObjectModel(ObjectModelError),
    ProjectOutsideRepository {
        project: PathBuf,
        repository: PathBuf,
    },
}

impl fmt::Display for SemanticDiffError {
    /// Render a locale-independent diagnostic for library callers.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(_) => formatter.write_str("repository operation failed"),
            Self::ObjectModel(error) => write!(formatter, "object model failed: {error}"),
            Self::ProjectOutsideRepository {
                project,
                repository,
            } => write!(
                formatter,
                "project {} is outside repository {}",
                project.display(),
                repository.display()
            ),
        }
    }
}

impl std::error::Error for SemanticDiffError {
    /// Preserve repository causes for diagnostics.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ObjectModel(error) => Some(error),
            Self::Repository(_) | Self::ProjectOutsideRepository { .. } => None,
        }
    }
}
