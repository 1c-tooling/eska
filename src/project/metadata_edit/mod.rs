//! Validated edits of existing Designer properties, without serializing the document.

mod document;
pub(crate) mod ext_dimensions;
mod hierarchy;
pub(crate) mod lengths;
pub(crate) mod numbering;
mod patch;
pub(crate) mod references;
mod schema;
mod standard;
pub(crate) mod types;
pub(crate) mod value_schema;
mod values;
mod write;
pub(crate) use write::publish_snapshot_in;

pub(crate) use document::EditingDocument;
pub(crate) use document::ReadOnlyProperty;
pub use document::{EditableField, FieldStep, PropertyChange, PropertyEditing};
pub use schema::ScalarSchema;
pub use value_schema::{EditableValueType, ValueConstraints};

use std::{fmt::Write, io, ops::Range};

use sha2::{Digest, Sha256};

/// A replacement is always computed by the backend from a parsed property.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Replacement {
    pub range: Range<usize>,
    pub text: String,
}

/// A checked candidate retains both snapshots for conflict detection and exact undo.
#[derive(Debug)]
pub struct EditPlan {
    original: String,
    updated: String,
    replacements: Vec<Replacement>,
}

/// A related descriptor uses the same exact-byte plan as the property directly edited by the user.
#[derive(Debug)]
pub struct PropertyFileEdit {
    pub object_id: crate::project::metadata_model::ObjectId,
    pub path: std::path::PathBuf,
    pub plan: EditPlan,
}

/// Editing failures never imply that an unsuccessful write has changed the source.
#[derive(Debug)]
pub enum EditError {
    InvalidXml,
    UnsupportedValue,
    InvalidValue,
    IncompatibleProperty(crate::project::metadata_model::PropertyKey),
    Conflict,
    UnsafePath,
    ReadOnly,
    HistoryUnavailable,
    ContextRequired,
    Busy,
    RecoveryRequired,
    Io(io::Error),
}

/// Identify the exact source bytes, including BOM and line endings.
#[must_use]
pub fn snapshot(input: &str) -> String {
    Sha256::digest(input.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        })
}

impl EditPlan {
    /// Dependency checks compare the planned source, including during history replay.
    pub(crate) fn original(&self) -> &str {
        &self.original
    }

    /// Inspect the validated output without publishing it.
    #[must_use]
    pub fn output(&self) -> &str {
        &self.updated
    }

    /// Expose the minimal byte replacements for previews and regression checks.
    #[must_use]
    pub fn replacements(&self) -> &[Replacement] {
        &self.replacements
    }

    /// Whether publication can be skipped entirely, including metadata timestamps.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.replacements.is_empty()
    }

    /// Retain only changed bytes for exact undo; whole descriptors never accumulate in history.
    #[must_use]
    pub fn history(&self) -> SavedEdit {
        let mut removed = 0;
        let mut inserted = 0;
        let reverse = self
            .replacements
            .iter()
            .map(|replacement| {
                let start = replacement.range.start - removed + inserted;
                let old = self.original[replacement.range.clone()].to_owned();
                inserted += replacement.text.len();
                removed += replacement.range.len();
                Replacement {
                    range: start..start + replacement.text.len(),
                    text: old,
                }
            })
            .collect();
        SavedEdit {
            before: snapshot(&self.original),
            after: snapshot(&self.updated),
            forward: self.replacements.clone(),
            reverse,
        }
    }
}

/// Opaque replay data created exclusively from a successful backend plan.
#[derive(Clone, Debug)]
pub struct SavedEdit {
    before: String,
    after: String,
    forward: Vec<Replacement>,
    reverse: Vec<Replacement>,
}

impl SavedEdit {
    /// Construct an inverse plan only when the complete source still matches that history step.
    ///
    /// # Errors
    /// Returns a conflict when any part of the descriptor changed in another editor.
    pub fn replay(&self, input: &str, undo: bool) -> Result<EditPlan, EditError> {
        let (expected, replacements) = if undo {
            (&self.after, &self.reverse)
        } else {
            (&self.before, &self.forward)
        };
        if &snapshot(input) != expected {
            return Err(EditError::Conflict);
        }
        let mut updated = input.to_owned();
        for replacement in replacements.iter().rev() {
            updated.replace_range(replacement.range.clone(), &replacement.text);
        }
        Ok(EditPlan {
            original: input.to_owned(),
            updated,
            replacements: replacements.clone(),
        })
    }

    /// Bound session history by changed-byte storage as well as operation count.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.forward
            .iter()
            .chain(&self.reverse)
            .map(|edit| edit.text.len())
            .sum()
    }
}
