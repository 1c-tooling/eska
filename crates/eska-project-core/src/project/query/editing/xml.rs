//! Reuse metadata scalar patches; split text values retain separately anchored XML comments.

use crate::project::metadata_edit::{self, EditError, EditPlan, Replacement};

use super::{CommentPlacement, Query, QueryError, patch_error};

/// Ordinary values use the established scalar patch without serializing ancestors or siblings.
pub(super) fn plan(
    input: &str,
    query: &Query,
    value: &str,
    decisions: &[CommentPlacement],
    placed: &[CommentPlacement],
) -> Result<EditPlan, QueryError> {
    if decisions.is_empty() {
        match metadata_edit::text_plan(input, query.source.clone(), value) {
            Ok(plan) => return Ok(plan),
            Err(EditError::UnsupportedValue) => {}
            Err(error) => return Err(patch_error(&error)),
        }
    }
    let mut content = String::new();
    let mut offset = 0;
    for placement in placed {
        let target = placement.query_offset.ok_or(QueryError::InvalidValue)?;
        content.push_str(
            &metadata_edit::escape_xml(&value[offset..target])
                .map_err(|error| patch_error(&error))?,
        );
        content.push_str(&query.comments[placement.comment].text);
        offset = target;
    }
    content.push_str(
        &metadata_edit::escape_xml(&value[offset..]).map_err(|error| patch_error(&error))?,
    );
    metadata_edit::xml_replacements(
        input,
        vec![Replacement {
            range: crate::project::query::xml::content_range(input, query.source.clone())?,
            text: content,
        }],
    )
    .map_err(|error| patch_error(&error))
}
