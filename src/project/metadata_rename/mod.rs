//! Rename analysis keeps proven semantic references separate from textual candidates.

pub(crate) mod inventory;
mod name;
mod xml;

pub use name::{NameError, validate_name};
pub use xml::{ReferenceRename, XmlAnalysis};

use std::ops::Range;
use std::path::PathBuf;

use serde::Serialize;

/// A reference is changed only when its enclosing XML contract identifies its meaning.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameReplacement {
    pub range: Range<usize>,
    pub before: String,
    pub after: String,
}

/// A textual match has no authority to change an identifier by itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UncertainReference {
    pub range: Range<usize>,
    pub text: String,
    pub reason: &'static str,
}

/// File changes remain minimal byte ranges in the original source snapshot.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameFile {
    pub path: PathBuf,
    pub snapshot: String,
    pub replacements: Vec<RenameReplacement>,
    pub uncertain: Vec<UncertainReference>,
}

/// A descriptor and its payload directory may move together while retaining every UUID.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameMove {
    pub from: PathBuf,
    pub to: PathBuf,
    pub directory: bool,
}

/// Preview issues are explicit; absence of a confirmed writer never licenses a textual fallback.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameIssue {
    pub path: PathBuf,
    pub reason: &'static str,
}

/// Backend-owned project snapshot and resolved edits, shared by CLI and future IDE mutation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePlan {
    pub object_id: crate::project::metadata_model::ObjectId,
    pub new_object_id: crate::project::metadata_model::ObjectId,
    pub uuid: String,
    pub old_name: String,
    pub new_name: String,
    pub snapshot: String,
    pub files: Vec<RenameFile>,
    pub moves: Vec<RenameMove>,
    pub issues: Vec<RenameIssue>,
}

/// Case-insensitive identifier comparison does not normalize or merge multiple characters.
pub(crate) fn same_name(left: &str, right: &str) -> bool {
    left.chars().count() == right.chars().count()
        && left.chars().zip(right.chars()).all(|(left, right)| {
            left == right
                || left.to_lowercase().eq(right.to_lowercase())
                || left.to_uppercase().eq(right.to_uppercase())
        })
}
