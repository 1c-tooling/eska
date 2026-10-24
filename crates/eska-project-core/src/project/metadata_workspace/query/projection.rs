//! Immutable language views have no source-write authority and never retain the source session's lease.

use std::{ops::Range, sync::Arc};

use crate::project::{
    metadata_edit::{EditPlan, Replacement, snapshot},
    query::{self, Affinity, Position, Query, QueryError},
};

/// An external language server reads the exact module with only its selected literal projected from the draft.
#[derive(Debug)]
pub struct QueryProjection {
    original: Arc<str>,
    original_range: Range<usize>,
    owner: Arc<str>,
    query: Query,
    snapshot: String,
    revision: u64,
}

/// A projected location either belongs to the draft or addresses unchanged native owner text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryProjectionTarget {
    Draft(Position),
    Owner(Position),
}

impl QueryProjection {
    /// Re-extract the output with the common codec; caller ranges cannot substitute a fabricated query identity.
    pub(super) fn new(
        original: Arc<str>,
        range: Range<usize>,
        owner: Arc<str>,
        revision: u64,
    ) -> Result<Self, QueryError> {
        let query = query::read_bsl(&owner, range.start)?;
        if query.map.spans().len() > 65_536 {
            return Err(QueryError::TooLarge);
        }
        let snapshot = snapshot(&owner);
        Ok(Self {
            original,
            original_range: range,
            owner,
            query,
            snapshot,
            revision,
        })
    }

    /// Language input is immutable even when the native draft or owner advances afterwards.
    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// The baseline for language edit translation is the decoded projection, with logical LF endings.
    #[must_use]
    pub fn draft(&self) -> &str {
        &self.query.text
    }

    /// The hash identifies exactly the bytes supplied to the language server.
    #[must_use]
    pub fn snapshot(&self) -> &str {
        &self.snapshot
    }

    /// A later response cannot borrow a newer draft revision just because its URI is unchanged.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Charge both retained texts and mappings, including original text after its source session closes.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.original.len()
            + self.owner.len()
            + self.query.text.len()
            + std::mem::size_of_val(self.query.map.spans())
    }

    /// Convert the query editor's UTF-16 position through the codec, without rounding across source-only syntax.
    #[must_use]
    pub fn owner_position(&self, position: Position, affinity: Affinity) -> Option<Position> {
        let offset = query::position_byte(&self.query.text, position)?;
        let source = self
            .query
            .map
            .query_to_source(&self.query.text, offset, affinity)?;
        query::byte_position(&self.owner, source)
    }

    /// Quotes, escaped-character interiors and continuation prefixes cannot become query coordinates.
    #[must_use]
    pub fn draft_position(&self, position: Position) -> Option<Position> {
        let source = query::position_byte(&self.owner, position)?;
        let offset = self.query.map.source_to_query(&self.owner, source)?;
        query::byte_position(&self.query.text, offset)
    }

    /// Navigation into unchanged code compensates for the projected literal's length; query locations target its draft.
    #[must_use]
    pub fn target(&self, position: Position) -> Option<QueryProjectionTarget> {
        let offset = query::position_byte(&self.owner, position)?;
        if let Some(draft) = self.draft_position(position) {
            return Some(QueryProjectionTarget::Draft(draft));
        }
        let original = if offset < self.query.source.start {
            offset
        } else if offset >= self.query.source.end {
            self.original_range
                .end
                .checked_add(offset - self.query.source.end)?
        } else {
            return None;
        };
        query::byte_position(&self.original, original).map(QueryProjectionTarget::Owner)
    }

    /// Translate a complete language edit set into one minimal decoded-draft edit, never a native owner write.
    ///
    /// # Errors
    /// Rejects overlaps, source-only boundaries, oversized output and edits that escape or split the selected literal.
    pub fn translate_edits(&self, edits: Vec<Replacement>) -> Result<EditPlan, QueryError> {
        if edits.len() > 64 {
            return Err(QueryError::TooLarge);
        }
        let mut removed = 0_usize;
        let mut added = 0_usize;
        for edit in &edits {
            if edit.range.start > edit.range.end
                || self
                    .query
                    .map
                    .source_to_query(&self.owner, edit.range.start)
                    .is_none()
                || self
                    .query
                    .map
                    .source_to_query(&self.owner, edit.range.end)
                    .is_none()
            {
                return Err(QueryError::InvalidPosition);
            }
            removed = removed
                .checked_add(edit.range.len())
                .ok_or(QueryError::TooLarge)?;
            added = added
                .checked_add(edit.text.len())
                .ok_or(QueryError::TooLarge)?;
        }
        if self
            .owner
            .len()
            .saturating_sub(removed)
            .saturating_add(added)
            > query::MAX_SNAPSHOT_BYTES
        {
            return Err(QueryError::TooLarge);
        }
        let source = EditPlan::from_replacements(&self.owner, edits)
            .map_err(|_| QueryError::InvalidValue)?;
        let result = query::read_bsl(source.output(), self.query.source.start)?;
        if source.output().get(..result.source.start) != self.owner.get(..self.query.source.start)
            || source.output().get(result.source.end..) != self.owner.get(self.query.source.end..)
        {
            return Err(QueryError::UnsupportedExpression);
        }
        let edits = minimal_edit(&self.query.text, &result.text)
            .into_iter()
            .collect();
        EditPlan::from_replacements(&self.query.text, edits).map_err(|_| QueryError::InvalidValue)
    }
}

/// Keep a completion replacement local to its changed word instead of replacing the entire SDBL editor model.
fn minimal_edit(before: &str, after: &str) -> Option<Replacement> {
    if before == after {
        return None;
    }
    let start = before
        .chars()
        .zip(after.chars())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum();
    let suffix: usize = before[start..]
        .chars()
        .rev()
        .zip(after[start..].chars().rev())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum();
    Some(Replacement {
        range: start..before.len() - suffix,
        text: after[start..after.len() - suffix].to_owned(),
    })
}
