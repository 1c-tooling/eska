//! Explicit, cooperative metadata indexing independent of visible tree expansion.
mod indexing;
mod query;

use super::WorkspaceError;
use crate::project::{
    metadata_model::{LocalizedText, MetadataKind, NodeId, ObjectId, ProjectScope},
    metadata_parser::Diagnostic,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Incomplete work and failed XML are never represented as a successful empty index.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IndexState {
    #[default]
    NotStarted,
    Building,
    Cancelled,
    Ready,
    Incomplete,
}

/// Progress belongs to one project session and generation.
#[derive(Clone, Debug)]
pub struct IndexProgress {
    pub state: IndexState,
    pub generation: u64,
    pub indexed_objects: usize,
    pub pending_descriptors: usize,
    pub failed_descriptors: usize,
}

/// Retain structured reasons for incomplete coverage, including recoverable parser diagnostics.
#[derive(Debug)]
pub enum IndexFailure {
    Read(WorkspaceError),
    Unsupported(Vec<Diagnostic>),
}

/// Literal substring matching; language filters synonym keys only, never object names.
#[derive(Clone, Debug)]
pub struct SearchOptions {
    pub limit: usize,
    pub synonym_language: Option<String>,
}

impl Default for SearchOptions {
    /// Keep interactive responses small; callers can request at most 500 hits.
    fn default() -> Self {
        Self {
            limit: 50,
            synonym_language: None,
        }
    }
}

/// Stable ranking: exact, prefix, substring; names win over synonyms within each category.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MatchRank {
    ExactName,
    ExactSynonym,
    PrefixName,
    PrefixSynonym,
    SubstringName,
    SubstringSynonym,
}

/// A real object hit, scoped independently from objects with identical labels elsewhere.
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub project: ProjectScope,
    pub generation: u64,
    pub object: ObjectId,
    pub node: NodeId,
    pub kind: MetadataKind,
    pub name: String,
    pub synonyms: Vec<LocalizedText>,
    /// Inclusive root-to-object tree ancestry, including virtual collections.
    pub ancestry: Vec<NodeId>,
    pub rank: MatchRank,
}

#[derive(Clone, Debug)]
pub struct SearchResponse {
    pub progress: IndexProgress,
    pub hits: Vec<SearchHit>,
    pub truncated: bool,
}

#[derive(Debug)]
pub(super) struct Record {
    pub owner: ObjectId,
    pub kind: MetadataKind,
    pub name: String,
    pub folded: String,
    pub synonyms: Vec<LocalizedText>,
    pub folded_synonyms: Vec<(String, String)>,
    pub ancestry: Vec<NodeId>,
}

#[derive(Debug, Default)]
pub(crate) struct SearchIndex {
    pub(super) records: BTreeMap<ObjectId, Record>,
    pub(super) pending: BTreeMap<ObjectId, Vec<NodeId>>,
    pub(super) references: BTreeMap<ObjectId, BTreeSet<ObjectId>>,
    pub(super) failures: BTreeMap<ObjectId, IndexFailure>,
    pub(super) dirty: BTreeSet<ObjectId>,
    pub(super) state: IndexState,
    pub(crate) paths: BTreeMap<PathBuf, ObjectId>,
}

impl SearchIndex {
    /// Hide superseded records immediately while scheduling just their owning descriptor.
    pub(crate) fn invalidate(&mut self, owner: &ObjectId, root: &ObjectId) {
        if self.state == IndexState::NotStarted {
            return;
        }
        let ancestry = if owner == root {
            Some(Vec::new())
        } else {
            self.records
                .get(owner)
                .map(|record| record.ancestry[..record.ancestry.len() - 1].to_vec())
        };
        if let Some(ancestry) = ancestry {
            self.pending.insert(owner.clone(), ancestry);
            self.dirty.insert(owner.clone());
            self.failures.remove(owner);
            if self.state != IndexState::Cancelled {
                self.state = IndexState::Building;
            }
        }
    }

    /// Membership is tested by typed ancestry, never by ambiguous textual ID prefixes.
    pub(super) fn visible(&self, record: &Record) -> bool {
        !record
            .ancestry
            .iter()
            .any(|node| matches!(node, NodeId::Object(id) if self.dirty.contains(id)))
    }

    /// Settle completed cooperative work without losing cancellation intent.
    pub(super) fn settle(&mut self) {
        if self.state == IndexState::Cancelled || self.state == IndexState::NotStarted {
            return;
        }
        self.state = if !self.pending.is_empty() {
            IndexState::Building
        } else if self.failures.is_empty() {
            IndexState::Ready
        } else {
            IndexState::Incomplete
        };
    }
}
