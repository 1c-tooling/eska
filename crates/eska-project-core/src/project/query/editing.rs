//! Pure source plans are re-extracted from the original snapshot and never write the owner.

mod bsl;
mod xml;

use std::ops::Range;

use crate::project::metadata_edit::{self, EditError, EditPlan, Replacement};

use super::{Query, QueryError, QueryIdentity};

/// A missing destination means explicit deletion; omitted decisions never delete comments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommentPlacement {
    pub comment: usize,
    pub query_offset: Option<usize>,
}

/// Newly inserted physical lines must match the editor model used to apply a byte plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    /// Render a physical source newline without changing decoded query characters.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }

    /// Normalize only newly emitted source syntax; unchanged owner bytes stay outside the plan.
    pub(super) fn render(self, value: &str) -> String {
        let mut output = String::with_capacity(value.len());
        let mut characters = value.chars().peekable();
        while let Some(character) = characters.next() {
            if matches!(character, '\r' | '\n') {
                if character == '\r' && characters.peek() == Some(&'\n') {
                    characters.next();
                }
                output.push_str(self.as_str());
            } else {
                output.push(character);
            }
        }
        output
    }
}

/// Plan against exact owner bytes, independently of later filesystem/editor publication.
///
/// # Errors
/// Rejects stale snapshots, unsupported owners/values and unresolved source-comment placement.
pub fn plan(
    input: &str,
    expected_snapshot: &str,
    identity: &QueryIdentity,
    value: &str,
    decisions: &[CommentPlacement],
    line_ending: Option<LineEnding>,
) -> Result<EditPlan, QueryError> {
    super::check_size(input)?;
    super::check_size(value)?;
    if metadata_edit::snapshot(input) != expected_snapshot {
        return Err(QueryError::Conflict);
    }
    let query = resolve(input, identity)?;
    let normalized;
    let value = if matches!(identity, QueryIdentity::BslLiteral(_)) {
        normalized = value.replace("\r\n", "\n");
        if normalized.contains('\r')
            || normalized
                .chars()
                .any(|character| character < ' ' && !matches!(character, '\n' | '\t'))
        {
            return Err(QueryError::InvalidValue);
        }
        normalized.as_str()
    } else {
        value
    };
    if value == query.text && decisions.is_empty() {
        return EditPlan::from_replacements(input, Vec::new()).map_err(|error| patch_error(&error));
    }
    let placed = placements(&query, value, decisions)?;
    let result = if matches!(identity, QueryIdentity::BslLiteral(_)) {
        bsl::plan(input, &query, value, decisions, &placed, line_ending)?
    } else {
        xml::plan(input, &query, value, decisions, &placed, line_ending)?
    };
    super::check_size(result.output())?;
    let updated_identity = match identity {
        QueryIdentity::BslLiteral(_) => {
            let current = super::read_bsl(result.output(), query.source.start)?;
            current.identity
        }
        _ => identity.clone(),
    };
    let updated = resolve(result.output(), &updated_identity)?;
    if updated.text != value || updated.comments.len() != placed.len() {
        return Err(QueryError::UnsupportedStructure);
    }
    for (comment, placement) in updated.comments.iter().zip(&placed) {
        if comment.text != query.comments[placement.comment].text
            || comment.query_offset != placement.query_offset.unwrap_or_default()
        {
            return Err(QueryError::UnsupportedStructure);
        }
    }
    Ok(result)
}

/// Do not accept a caller-supplied range or decoded value in place of checked source structure.
fn resolve(input: &str, identity: &QueryIdentity) -> Result<Query, QueryError> {
    match identity {
        QueryIdentity::BslLiteral(range) => {
            let query = super::read_bsl(input, range.start)?;
            if &query.identity != identity {
                return Err(QueryError::Conflict);
            }
            Ok(query)
        }
        _ => super::read_xml(input)?
            .into_iter()
            .find(|query| &query.identity == identity)
            .ok_or(QueryError::NotFound),
    }
}

/// Only the differing middle is replaced; matching Unicode scalars retain their original bytes.
fn difference(before: &str, after: &str) -> (Range<usize>, Range<usize>) {
    let prefix: usize = before
        .chars()
        .zip(after.chars())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum();
    let suffix: usize = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum();
    (prefix..before.len() - suffix, prefix..after.len() - suffix)
}

/// Move anchors only through unchanged text; edits across a comment need an explicit decision.
fn placements(
    query: &Query,
    value: &str,
    decisions: &[CommentPlacement],
) -> Result<Vec<CommentPlacement>, QueryError> {
    let mut decisions_by_comment = std::collections::BTreeMap::new();
    for decision in decisions {
        if decision.comment >= query.comments.len()
            || decisions_by_comment
                .insert(decision.comment, decision.query_offset)
                .is_some()
        {
            return Err(QueryError::InvalidValue);
        }
    }
    let (old, new) = difference(&query.text, value);
    let mut placed = Vec::new();
    for (index, comment) in query.comments.iter().enumerate() {
        let offset = if let Some(offset) = decisions_by_comment.get(&index) {
            *offset
        } else if old.is_empty() && comment.query_offset == old.start && query.text != value {
            return Err(QueryError::CommentsRequireDecision);
        } else if comment.query_offset <= old.start {
            Some(comment.query_offset)
        } else if comment.query_offset >= old.end {
            Some(new.end + comment.query_offset - old.end)
        } else {
            return Err(QueryError::CommentsRequireDecision);
        };
        if let Some(offset) = offset {
            if !value.is_char_boundary(offset) {
                return Err(QueryError::InvalidValue);
            }
            if matches!(query.identity, QueryIdentity::BslLiteral(_))
                && (offset == 0 || !value[..offset].ends_with('\n'))
            {
                return Err(QueryError::CommentsRequireDecision);
            }
            placed.push(CommentPlacement {
                comment: index,
                query_offset: Some(offset),
            });
        }
    }
    placed.sort_by_key(|placement| (placement.query_offset, placement.comment));
    Ok(placed)
}

/// The shared byte plan rejects overlapping ranges before allocating the changed owner.
fn replacement(input: &str, replacement: Replacement) -> Result<EditPlan, QueryError> {
    let length = input
        .len()
        .checked_sub(replacement.range.len())
        .and_then(|length| length.checked_add(replacement.text.len()))
        .ok_or(QueryError::TooLarge)?;
    if length > super::MAX_SNAPSHOT_BYTES {
        return Err(QueryError::TooLarge);
    }
    EditPlan::from_replacements(input, vec![replacement]).map_err(|error| patch_error(&error))
}

/// Keep structured core errors; human wording belongs to the protocol/CLI presentation layer.
const fn patch_error(error: &EditError) -> QueryError {
    match error {
        EditError::InvalidXml => QueryError::InvalidXml,
        EditError::InvalidValue => QueryError::InvalidValue,
        EditError::Conflict => QueryError::Conflict,
        _ => QueryError::UnsupportedStructure,
    }
}

#[cfg(test)]
mod tests;
