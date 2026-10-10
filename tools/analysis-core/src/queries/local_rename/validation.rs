//! Conservative capture checks inspect native tokens; only the kernel enumerates symbol references.

use std::collections::BTreeSet;

use base_db::{RootQueryDb, SourceDatabase};
use hir::Name;

use super::{DefinitionTarget, LocalRenameError, LocalRenameTarget};
use crate::{AnalysisSession, QueryCancellation, QueryError, ReferenceOccurrence};

/// A whole single name must parse in its declaration context, including the kernel's reserved words.
pub(super) fn identifier(name: &str, parameter: bool) -> Result<(), LocalRenameError> {
    if name.len() > super::MAX_RENAME_NAME_BYTES {
        return Err(LocalRenameError::NameTooLarge {
            bytes: name.len(),
            limit: super::MAX_RENAME_NAME_BYTES,
        });
    }
    let prefix = if parameter {
        "Процедура _("
    } else {
        "Процедура _()\nПерем "
    };
    let start = u32::try_from(prefix.len()).map_err(|_| LocalRenameError::InvalidName)?;
    let end = u32::try_from(
        prefix
            .len()
            .checked_add(name.len())
            .ok_or(LocalRenameError::InvalidName)?,
    )
    .map_err(|_| LocalRenameError::InvalidName)?;
    if name.is_empty() {
        return Err(LocalRenameError::InvalidName);
    }
    let parsed = parser::parse(&format!(
        "{prefix}{name}{}\n{name}=1;\nСообщить({name});\nКонецПроцедуры",
        if parameter { ")" } else { ";" }
    ));
    let token = parsed
        .syntax_node()
        .token_at_offset(start.into())
        .right_biased();
    if parsed.has_errors()
        || token.is_none_or(|token| {
            !token.kind().is_name_token()
                || u32::from(token.text_range().start()) != start
                || u32::from(token.text_range().end()) != end
        })
    {
        return Err(LocalRenameError::InvalidName);
    }
    Ok(())
}

/// Every replacement remains in the captured document; new names cannot capture existing tokens.
pub(super) fn occurrences(
    session: &AnalysisSession,
    target: &LocalRenameTarget,
    hits: Vec<ReferenceOccurrence>,
    new_name: &str,
    control: &QueryCancellation,
) -> Result<Vec<DefinitionTarget>, LocalRenameError> {
    session
        .query_module(
            &target.source,
            target.revision,
            control,
            |analysis, file| {
                let text = analysis.database().file_text(file);
                let old_name = Name::new(&target.symbol);
                let mut ranges = BTreeSet::new();
                let mut changed = Vec::new();
                let mut previous_end = 0;
                for hit in hits {
                    if control.is_cancelled() {
                        return Err(QueryError::Cancelled);
                    }
                    let hit = hit.target;
                    let start = hit.start_byte;
                    let end = hit.end_byte;
                    if hit.source != target.source
                        || hit.source_sha256 != target.source_sha256
                        || start >= end
                        || start < previous_end
                    {
                        return Ok(Err(LocalRenameError::InconsistentOccurrences));
                    }
                    let Some(spelling) = text.get(start as usize..end as usize) else {
                        return Ok(Err(LocalRenameError::InconsistentOccurrences));
                    };
                    if !Name::new(spelling).eq_ignore_case(&old_name) {
                        return Ok(Err(LocalRenameError::InconsistentOccurrences));
                    }
                    ranges.insert((start, end));
                    previous_end = end;
                    if spelling != new_name {
                        changed.push(hit);
                    }
                }
                if !ranges.contains(&(target.start_byte, target.end_byte)) {
                    return Ok(Err(LocalRenameError::InconsistentOccurrences));
                }
                let replacement = Name::new(new_name);
                // Case-only edits preserve the binding; rejecting old names in other scopes would be spurious.
                if !replacement.eq_ignore_case(&old_name) {
                    let parse = analysis.database().parse(file);
                    for element in parse.syntax_node().descendants_with_tokens() {
                        if control.is_cancelled() {
                            return Err(QueryError::Cancelled);
                        }
                        let Some(token) = element.into_token() else {
                            continue;
                        };
                        if token.kind().is_name_token()
                            && Name::new(token.text()).eq_ignore_case(&replacement)
                        {
                            return Ok(Err(LocalRenameError::NameConflict {
                                start_byte: token.text_range().start().into(),
                                end_byte: token.text_range().end().into(),
                            }));
                        }
                    }
                }
                Ok(Ok(changed))
            },
        )
        .map_err(LocalRenameError::Query)?
        .value
}
