//! Formatting anchors source comments through an unchanged query token stream before checked source encoding.

use super::{
    CommentPlacement, LineEnding, QueryError, QueryIdentity,
    constructor::lex::{self, Kind, Token},
};
use crate::project::metadata_edit::EditPlan;

/// Encode a formatted query into one selected owner without serializing surrounding XML or BSL.
///
/// # Errors
/// Rejects a stale owner, changed token values and ambiguous source-comment anchors.
pub fn format_plan(
    input: &str,
    expected_snapshot: &str,
    identity: &QueryIdentity,
    formatted: &str,
    line_ending: Option<LineEnding>,
) -> Result<EditPlan, QueryError> {
    if crate::project::metadata_edit::snapshot(input) != expected_snapshot {
        return Err(QueryError::Conflict);
    }
    let query = match identity {
        QueryIdentity::BslLiteral(range) => super::read_bsl(input, range.start)?,
        _ => super::read_xml(input)?
            .into_iter()
            .find(|query| &query.identity == identity)
            .ok_or(QueryError::NotFound)?,
    };
    if &query.identity != identity {
        return Err(QueryError::Conflict);
    }
    if query.text == formatted {
        return super::plan(
            input,
            expected_snapshot,
            identity,
            formatted,
            &[],
            line_ending,
        );
    }
    let old = lex::tokens(&query.text).map_err(|_| QueryError::UnsupportedStructure)?;
    let bsl = matches!(identity, QueryIdentity::BslLiteral(_));
    let mut value = if bsl {
        formatted.replace("\r\n", "\n")
    } else {
        formatted.to_owned()
    };
    let new = lex::tokens(&value).map_err(|_| QueryError::UnsupportedStructure)?;
    if old.len() != new.len()
        || !old.iter().zip(&new).all(|(a, b)| {
            a.kind == b.kind
                && if a.kind == Kind::Word {
                    query.text[a.range.clone()].to_lowercase()
                        == value[b.range.clone()].to_lowercase()
                } else {
                    query.text[a.range.clone()] == value[b.range.clone()]
                }
        })
    {
        return Err(QueryError::InvalidValue);
    }
    let mut decisions = query
        .comments
        .iter()
        .enumerate()
        .map(|(index, comment)| {
            let offset = anchor(&old, &new, comment.query_offset, &value)?;
            let offset = if bsl {
                value[..offset].rfind('\n').map_or(0, |at| at + 1)
            } else {
                offset
            };
            Ok(CommentPlacement {
                comment: index,
                query_offset: Some(offset),
            })
        })
        .collect::<Result<Vec<_>, QueryError>>()?;
    // BSL source comments require a physical continuation line; retain one query newline if needed.
    if bsl
        && decisions
            .iter()
            .any(|decision| decision.query_offset == Some(0))
    {
        value.insert(0, '\n');
        for decision in &mut decisions {
            decision.query_offset = decision.query_offset.map(|offset| offset + 1);
        }
    }
    if value == query.text
        && decisions
            .iter()
            .zip(&query.comments)
            .all(|(decision, comment)| decision.query_offset == Some(comment.query_offset))
    {
        return super::plan(input, expected_snapshot, identity, &value, &[], line_ending);
    }
    super::plan(
        input,
        expected_snapshot,
        identity,
        &value,
        &decisions,
        line_ending,
    )
}

/// A whitespace anchor follows the next unchanged token; an interior XML comment retains its character offset.
fn anchor(old: &[Token], new: &[Token], offset: usize, value: &str) -> Result<usize, QueryError> {
    let index = old.partition_point(|token| token.range.end <= offset);
    let Some(token) = old.get(index) else {
        return Ok(value.len());
    };
    let current = &new[index];
    if offset <= token.range.start {
        return Ok(current.range.start);
    }
    let inner = offset - token.range.start;
    if inner > current.range.len() || !value.is_char_boundary(current.range.start + inner) {
        return Err(QueryError::CommentsRequireDecision);
    }
    Ok(current.range.start + inner)
}
