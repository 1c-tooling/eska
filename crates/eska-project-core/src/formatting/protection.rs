//! Formatting eligibility is stricter than ordinary editing and cannot be relaxed by a style.

use super::{
    Error, Language,
    protocol::{Range, Request, Response},
};
use crate::project::{
    metadata_workspace::{ProjectSession, QueryDocument},
    query::{self, QueryIdentity, bsl::Kind},
    support::State,
};
use std::{ops::Range as ByteRange, path::Path};

#[cfg(test)]
mod tests;

/// A supplier's editable rule authorizes only an explicitly selected part of its source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Unrestricted,
    SelectionOnly,
}

impl Access {
    /// Reject whole-document and cursor-only requests before starting the formatter.
    ///
    /// # Errors
    /// Returns a stable policy failure for absent, empty, complete or invalid selections.
    pub fn check_range(self, text: &str, range: Option<Range>) -> Result<(), Error> {
        if self == Self::Unrestricted {
            return Ok(());
        }
        if range.is_none_or(|range| {
            range.start >= range.end
                || range.start == 0 && range.end == text.len()
                || !text.is_char_boundary(range.start)
                || !text.is_char_boundary(range.end)
        }) {
            return Err(Error::new("selection_required", "range"));
        }
        Ok(())
    }

    /// Syntax expansion may never authorize edits outside a supplier-supported selection.
    ///
    /// # Errors
    /// Rejects edits outside the requested bytes, including insertion at an unselected EOF.
    pub fn check_response(self, request: &Request, response: &Response) -> Result<(), Error> {
        self.check_range(&request.text, request.range)?;
        if self == Self::SelectionOnly
            && let Some(range) = request.range
            && response.edits.iter().any(|edit| {
                edit.range.start < range.start
                    || edit.range.end > range.end
                    || edit.range.start == range.end
            })
        {
            return Err(Error::new("selection_expansion", "range"));
        }
        check_response(request, response)
    }

    /// Publication compares untouched prefixes and suffixes instead of trusting a caller's plan.
    ///
    /// # Errors
    /// Rejects a widened selection or any changed controlled method immediately before writing.
    pub fn check_output(
        self,
        before: &str,
        after: &str,
        range: Option<Range>,
        bsl: bool,
    ) -> Result<(), Error> {
        self.check_range(before, range)?;
        if self == Self::SelectionOnly
            && let Some(range) = range
            && (!after.starts_with(&before[..range.start])
                || !after.ends_with(&before[range.end..])
                || after.len() < range.start + before.len() - range.end)
        {
            return Err(Error::new("selection_expansion", "range"));
        }
        if bsl {
            let old = controlled_ranges(before)?;
            let new = controlled_ranges(after)?;
            if old.len() != new.len()
                || old
                    .iter()
                    .zip(new)
                    .any(|(old, new)| before[old.clone()] != after[new])
            {
                return Err(Error::new("change_control_protected", "method"));
            }
        }
        Ok(())
    }
}

/// Resolve every physical owner, including mixed XML, from the current supplier rules.
///
/// # Errors
/// Missing, malformed, locked or unresolved support never grants formatting permission.
pub fn support_access(project: &mut ProjectSession, path: &Path) -> Result<Access, Error> {
    let snapshot = project
        .support_files(&[path.to_owned()])
        .map_err(|_| Error::new("support_unknown", path.to_string_lossy()))?;
    if !snapshot.diagnostics.is_empty()
        || snapshot
            .files
            .first()
            .is_none_or(|file| file.unknown || file.objects.is_empty())
        || snapshot
            .objects
            .iter()
            .any(|object| object.state == State::Unknown)
    {
        return Err(Error::new("support_unknown", path.to_string_lossy()));
    }
    if snapshot.files.iter().any(|file| file.read_only)
        || snapshot
            .objects
            .iter()
            .any(|object| object.state == State::Locked)
    {
        return Err(Error::new("source_read_only", path.to_string_lossy()));
    }
    Ok(
        if snapshot
            .objects
            .iter()
            .any(|object| object.state == State::EditableWithSupport)
        {
            Access::SelectionOnly
        } else {
            Access::Unrestricted
        },
    )
}

/// A private query is already an explicit source selection, but retains its enclosing method's prohibition.
///
/// # Errors
/// Rejects a controlled method, an unknown query identity or current supplier restrictions.
pub fn check_query_owner(
    project: &mut ProjectSession,
    document: &QueryDocument,
    identity: &QueryIdentity,
) -> Result<(), Error> {
    let query = match identity {
        QueryIdentity::BslLiteral(range) => query::read_bsl(document.input(), range.start),
        _ => document.xml_queries().and_then(|queries| {
            queries
                .into_iter()
                .find(|query| &query.identity == identity)
                .ok_or(query::QueryError::NotFound)
        }),
    }
    .map_err(|_| Error::new("unsupported_source", "query"))?;
    check_query(document.input(), identity)?;
    support_access(project, document.path())?.check_range(
        document.input(),
        Some(Range {
            start: query.source.start,
            end: query.source.end,
        }),
    )
}

/// Source maps cannot be used to format a literal inside an otherwise frozen method.
///
/// # Errors
/// Returns a protection failure before constructing or encoding any query edit.
pub fn check_query(input: &str, identity: &QueryIdentity) -> Result<(), Error> {
    if let QueryIdentity::BslLiteral(selected) = identity
        && controlled_ranges(input)?
            .iter()
            .any(|range| intersects(range, selected))
    {
        return Err(Error::new("change_control_protected", "query").at(selected.start));
    }
    Ok(())
}

/// Independently verify component replies, including a component predating these protection rules.
///
/// # Errors
/// No edit may replace any bytes belonging to a change-and-validate method.
pub fn check_response(request: &Request, response: &Response) -> Result<(), Error> {
    response.validate(request)?;
    if request.language == Language::Bsl {
        let ranges = controlled_ranges(&request.text)?;
        if response.edits.iter().any(|edit| {
            ranges.iter().any(|range| {
                intersects(range, &(edit.range.start..edit.range.end))
                    || edit.range.start == edit.range.end && range.contains(&edit.range.start)
            })
        }) {
            return Err(Error::new("change_control_protected", "method"));
        }
    }
    Ok(())
}

/// Read annotations and method boundaries lexically, ignoring strings, comments and deleted reference text.
///
/// # Errors
/// Rejects incomplete control annotations and ambiguous conditional method boundaries conservatively.
pub fn controlled_ranges(input: &str) -> Result<Vec<ByteRange<usize>>, Error> {
    if input.len() > query::MAX_SNAPSHOT_BYTES {
        return Err(Error::new("text_too_large", "source"));
    }
    if !input.contains('&') {
        return Ok(Vec::new());
    }
    let tokens = query::bsl::tokens(input)
        .map_err(|_| Error::new("unsupported_source", "control_boundaries"))?;
    let mut state = Control::default();
    for (index, token) in tokens.iter().enumerate() {
        let text = &input[token.range.clone()];
        if token.kind == Kind::Other && text.starts_with(['#', '&']) {
            state.directive(input, token.range.start, text)?;
        } else if !state.deleted
            && token.kind == Kind::Word
            && (index == 0 || tokens[index - 1].kind != Kind::Punctuation(b'.'))
        {
            state.word(input, token.range.start, token.range.end, text)?;
        }
    }
    if state.controlled || state.method.is_some() {
        return Err(Error::new("unsupported_source", "control_boundaries"));
    }
    Ok(state.ranges)
}

#[derive(Default)]
struct Control {
    annotation: Option<usize>,
    controlled: bool,
    method: Option<(usize, usize)>,
    depth: usize,
    deleted: bool,
    ranges: Vec<ByteRange<usize>>,
}

impl Control {
    /// Conditional method endings must occur at the same depth as their declaration.
    fn directive(&mut self, input: &str, offset: usize, text: &str) -> Result<(), Error> {
        let name = text
            .split(|character: char| character == '(' || character.is_whitespace())
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if !self.deleted
            && self.controlled
            && matches!(
                name.as_str(),
                "#если"
                    | "#if"
                    | "#иначеесли"
                    | "#elsif"
                    | "#иначе"
                    | "#else"
                    | "#конецесли"
                    | "#endif"
            )
        {
            return Err(Error::new("unsupported_source", "control_boundaries"));
        }
        if !self.deleted && text.starts_with('&') && controlled_annotation(text) {
            self.annotation
                .get_or_insert_with(|| line_start(input, offset));
            self.controlled = true;
        }
        match name.as_str() {
            "#удаление" | "#delete" => self.deleted = true,
            "#конецудаления" | "#enddelete" => self.deleted = false,
            _ if self.deleted => (),
            "#если" | "#if" => self.depth += 1,
            "#конецесли" | "#endif" => self.depth = self.depth.saturating_sub(1),
            _ if text.starts_with('&') && self.method.is_none() => {
                self.annotation
                    .get_or_insert_with(|| line_start(input, offset));
            }
            _ => (),
        }
        if self.depth > 64 {
            return Err(Error::new("syntax_too_large", "preprocessor_depth"));
        }
        Ok(())
    }

    /// Only an annotated method is retained; unrelated declarations cannot widen the protected interval.
    fn word(&mut self, input: &str, offset: usize, end: usize, text: &str) -> Result<(), Error> {
        match text.to_lowercase().as_str() {
            "процедура" | "функция" | "procedure" | "function" if self.controlled =>
            {
                if self.method.is_some() {
                    return Err(Error::new("unsupported_source", "control_boundaries"));
                }
                self.method = Some((
                    self.annotation.unwrap_or_else(|| line_start(input, offset)),
                    self.depth,
                ));
                self.controlled = false;
            }
            "конецпроцедуры" | "конецфункции" | "endprocedure" | "endfunction" =>
            {
                if let Some((start, depth)) = self.method
                    && self.depth == depth
                {
                    let end = end + input[end..].find(['\r', '\n']).unwrap_or(input.len() - end);
                    self.ranges.push(start..end);
                    self.method = None;
                }
                self.annotation = None;
            }
            _ if self.method.is_none() && !self.controlled => self.annotation = None,
            _ => (),
        }
        Ok(())
    }
}

/// Multiple annotations on a physical line retain their lexical role; strings and comments cannot add one.
fn controlled_annotation(text: &str) -> bool {
    let mut offset = 0;
    let mut quoted = false;
    while let Some(character) = text[offset..].chars().next() {
        let rest = &text[offset..];
        if !quoted && rest.starts_with("//") {
            break;
        }
        if quoted && rest.starts_with("\"\"") {
            offset += 2;
            continue;
        }
        if character == '"' {
            quoted = !quoted;
        }
        if !quoted && character == '&' {
            let name = &rest[1..];
            let end = name
                .find(|character: char| !character.is_alphanumeric() && character != '_')
                .unwrap_or(name.len());
            if matches!(
                name[..end].to_lowercase().as_str(),
                "изменениеиконтроль" | "changeandvalidate"
            ) {
                return true;
            }
        }
        offset += character.len_utf8();
    }
    false
}

/// Annotation indentation belongs to the protected method, while the UTF-8 BOM belongs to its file.
fn line_start(input: &str, offset: usize) -> usize {
    input[..offset].rfind('\n').map_or_else(
        || usize::from(input.starts_with('\u{feff}')) * 3,
        |at| at + 1,
    )
}

/// Half-open source intervals do not accidentally include the following ordinary method.
const fn intersects(left: &ByteRange<usize>, right: &ByteRange<usize>) -> bool {
    left.start < right.end && right.start < left.end
}
