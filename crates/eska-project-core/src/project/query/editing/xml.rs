//! Reuse metadata scalar patches; split text values retain separately anchored XML comments.

use crate::project::metadata_edit::{self, EditError, EditPlan, Replacement};

use super::{CommentPlacement, LineEnding, Query, QueryError, patch_error};

/// Ordinary values use the established scalar patch without serializing ancestors or siblings.
pub(super) fn plan(
    input: &str,
    query: &Query,
    value: &str,
    decisions: &[CommentPlacement],
    placed: &[CommentPlacement],
    line_ending: Option<LineEnding>,
) -> Result<EditPlan, QueryError> {
    if decisions.is_empty() {
        match metadata_edit::text_plan(input, query.source.clone(), value) {
            Ok(plan) => {
                let Some(ending) = line_ending else {
                    return Ok(plan);
                };
                return metadata_edit::xml_replacements(
                    input,
                    plan.replacements()
                        .iter()
                        .map(|replacement| Replacement {
                            range: replacement.range.clone(),
                            text: ending.render(&replacement.text),
                        })
                        .collect(),
                )
                .map_err(|error| patch_error(&error));
            }
            Err(EditError::UnsupportedValue) => {}
            Err(error) => return Err(patch_error(&error)),
        }
    }
    let mut content = String::new();
    let mut offset = 0;
    for placement in placed {
        let target = placement.query_offset.ok_or(QueryError::InvalidValue)?;
        content.push_str(&escaped(&value[offset..target], line_ending)?);
        content.push_str(&query.comments[placement.comment].text);
        offset = target;
    }
    content.push_str(&escaped(&value[offset..], line_ending)?);
    metadata_edit::xml_replacements(
        input,
        vec![Replacement {
            range: crate::project::query::xml::content_range(input, query.source.clone())?,
            text: content,
        }],
    )
    .map_err(|error| patch_error(&error))
}

/// XML entities retain logical CR characters; only physical LF uses the editor's line ending.
fn escaped(value: &str, line_ending: Option<LineEnding>) -> Result<String, QueryError> {
    let value = metadata_edit::escape_xml(value).map_err(|error| patch_error(&error))?;
    Ok(match line_ending {
        Some(ending) => ending.render(&value),
        None => value,
    })
}
