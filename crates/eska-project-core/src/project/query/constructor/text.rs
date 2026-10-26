//! Text navigation, comment ownership, diffs and explicit buffer publication share the constructor snapshot.

use super::{
    Action, Constructor, Error, Model, Selection, Unavailable,
    lex::{self, Kind},
};
use crate::project::metadata_edit::{EditPlan, Replacement};
use std::{collections::BTreeMap, ops::Range};

/// Offsets refer to the raw snapshot's UTF-16 units; byte ranges never split a UTF-8 scalar.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum TextTarget {
    Item { selection: Selection },
    Offset { offset: usize },
    Range { range: Range<usize> },
}

/// Both coordinate systems are calculated once by ESKA, without an IDE-specific parser.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TextLocation {
    pub statement: usize,
    pub selection: Option<Selection>,
    pub range: Range<usize>,
    pub utf16: Range<usize>,
}

/// Comments outside recognized items remain visible, even in otherwise opaque text.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TextComment {
    pub range: Range<usize>,
    pub utf16: Range<usize>,
    pub statement: Option<usize>,
    pub selection: Option<Selection>,
}

/// Paging bounds response size and DOM work without dropping comments from the shared source.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentPage {
    pub comments: Vec<TextComment>,
    pub total: usize,
    pub next_offset: Option<usize>,
}

/// A minimal changed range is relative to the immutable opening and current working snapshots.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextChange {
    pub before: Range<usize>,
    pub after: Range<usize>,
    pub before_utf16: Range<usize>,
    pub after_utf16: Range<usize>,
}

/// The original is returned on demand; it is never another mutable query document.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TextDiff {
    pub original: String,
    pub change: Option<TextChange>,
}

impl Constructor {
    /// Locate the innermost statement/row at a checked byte or UTF-16 boundary.
    ///
    /// # Errors
    /// Rejects stale selections, out-of-range positions and split surrogate/UTF-8 boundaries.
    pub fn locate(&self, target: &TextTarget) -> Result<TextLocation, Error> {
        let range = match target {
            TextTarget::Item { selection } => self
                .model
                .statements
                .get(selection.statement)
                .and_then(|owner| owner.clauses.get(selection.clause))
                .and_then(|clause| clause.items.get(selection.item))
                .map(|item| item.range.clone())
                .ok_or(Error::InvalidSelection)?,
            TextTarget::Offset { offset } => {
                let byte = utf16_byte(self.text(), *offset).ok_or(Error::InvalidSelection)?;
                let (statement, selection) = self.owner_at(byte).ok_or(Error::InvalidSelection)?;
                selection.map_or_else(
                    || self.model.statements[statement].range.clone(),
                    |selection| {
                        self.model.statements[selection.statement].clauses[selection.clause].items
                            [selection.item]
                            .range
                            .clone()
                    },
                )
            }
            TextTarget::Range { range } => range.clone(),
        };
        let utf16 = utf16_range(self.text(), &range)?;
        let (statement, selection) = match target {
            TextTarget::Item { selection } => (selection.statement, Some(*selection)),
            _ => self.owner_at(range.start).ok_or(Error::InvalidSelection)?,
        };
        Ok(TextLocation {
            statement,
            selection,
            range,
            utf16,
        })
    }

    /// Inspect comments lazily, optionally restricted to the selected item's original comment association.
    ///
    /// # Errors
    /// Rejects invalid paging/selections or bounded lexer exhaustion.
    pub fn comment_page(
        &self,
        offset: usize,
        limit: usize,
        selection: Option<Selection>,
    ) -> Result<CommentPage, Error> {
        if limit == 0 || limit > 128 {
            return Err(Error::InvalidSelection);
        }
        let filter = selection
            .map(|selection| self.locate(&TextTarget::Item { selection }))
            .transpose()?;
        let comments: Vec<_> = lex::tokens(self.text())?
            .into_iter()
            .filter(|token| {
                token.kind == Kind::Comment
                    && filter.as_ref().is_none_or(|location| {
                        token.range.start >= location.range.start
                            && token.range.end <= location.range.end
                    })
            })
            .map(|token| token.range)
            .collect();
        if offset > comments.len() {
            return Err(Error::InvalidSelection);
        }
        let mut cursor = 0;
        let mut units = 0;
        let mut page = Vec::new();
        for range in comments.iter().skip(offset).take(limit) {
            units += self.text()[cursor..range.start].encode_utf16().count();
            let start = units;
            units += self.text()[range.clone()].encode_utf16().count();
            cursor = range.end;
            let owner = self.owner_at(range.start);
            page.push(TextComment {
                range: range.clone(),
                utf16: start..units,
                statement: owner.map(|(statement, _)| statement),
                selection: owner.and_then(|(_, selection)| selection),
            });
        }
        let next = offset + page.len();
        Ok(CommentPage {
            comments: page,
            total: comments.len(),
            next_offset: (next < comments.len()).then_some(next),
        })
    }

    /// Diff ranges come from the same minimal-plan implementation used for final native application.
    ///
    /// # Errors
    /// Rejects invalid internal range boundaries without changing any constructor state.
    pub fn text_diff(&self) -> Result<TextDiff, Error> {
        let plan = super::super::minimal_text_plan(&self.original, self.text())
            .map_err(|_| Error::InvalidExpression)?;
        let change = plan
            .replacements()
            .first()
            .map(|edit| {
                let after = edit.range.start..edit.range.start + edit.text.len();
                Ok(TextChange {
                    before: edit.range.clone(),
                    before_utf16: utf16_range(&self.original, &edit.range)?,
                    after_utf16: utf16_range(self.text(), &after)?,
                    after,
                })
            })
            .transpose()?;
        Ok(TextDiff {
            original: self.original.to_string(),
            change,
        })
    }

    /// Explicit text edits validate structure and retain unchanged opaque fragments, then publish once.
    pub(super) fn apply_text(
        &mut self,
        action: &Action,
        text: &str,
        editor_normalized: bool,
    ) -> Result<EditPlan, Error> {
        if text.len() > super::super::MAX_SNAPSHOT_BYTES {
            return Err(Error::TooLarge);
        }
        let plan = if editor_normalized {
            editor_plan(self.text(), text)?
        } else {
            super::super::minimal_text_plan(self.text(), text)
                .map_err(|_| Error::InvalidExpression)?
        };
        if plan.is_empty() {
            return Ok(plan);
        }
        lex::tokens(plan.output())?;
        let model = Model::parse(plan.output())?;
        checked_text(self.text(), &self.model, plan.output(), &model)?;
        self.accept(&plan, model, action)?;
        if self.focus.is_none() && !self.model.statements.is_empty() {
            self.focus = Some(0);
        }
        Ok(plan)
    }

    /// The narrowest containing statement prevents a nested header from selecting its parent source row.
    fn owner_at(&self, offset: usize) -> Option<(usize, Option<Selection>)> {
        let (statement, owner) = self
            .model
            .statements
            .iter()
            .enumerate()
            .filter(|(_, owner)| {
                owner.range.contains(&offset)
                    || offset == self.text().len() && owner.range.end == offset
            })
            .min_by_key(|(_, owner)| owner.range.len())?;
        let selected = owner
            .clauses
            .iter()
            .enumerate()
            .flat_map(|(clause, section)| {
                section.items.iter().enumerate().map(move |(item, value)| {
                    (
                        Selection {
                            statement,
                            clause,
                            item,
                        },
                        value,
                    )
                })
            })
            .filter(|(_, item)| item.range.contains(&offset))
            .min_by_key(|(_, item)| item.range.len())
            .map(|(selection, _)| selection);
        Some((statement, selected))
    }
}

/// A location cannot split a scalar in either transport coordinate system.
fn utf16_range(input: &str, range: &Range<usize>) -> Result<Range<usize>, Error> {
    let before = input.get(..range.start).ok_or(Error::InvalidSelection)?;
    let inside = input.get(range.clone()).ok_or(Error::InvalidSelection)?;
    let start = before.encode_utf16().count();
    Ok(start..start + inside.encode_utf16().count())
}

/// HTML/native text control offsets count UTF-16 code units, including both CR and LF in the raw snapshot.
fn utf16_byte(input: &str, target: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, character) in input.char_indices() {
        if units == target {
            return Some(byte);
        }
        units += character.len_utf16();
        if units > target {
            return None;
        }
    }
    (units == target).then_some(input.len())
}

/// Editor controls normalize CRLF/CR to LF; only the changed hunk is mapped back into original physical bytes.
fn editor_plan(input: &str, edited: &str) -> Result<EditPlan, Error> {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut edited = edited.replace("\r\n", "\n").replace('\r', "\n");
    if input.starts_with('\u{feff}') && !edited.starts_with('\u{feff}') {
        edited.insert(0, '\u{feff}');
    }
    let plan = super::super::minimal_text_plan(&normalized, &edited)
        .map_err(|_| Error::InvalidExpression)?;
    let newline = if input.contains("\r\n") {
        "\r\n"
    } else if input.contains('\r') {
        "\r"
    } else {
        "\n"
    };
    let edits = plan
        .replacements()
        .iter()
        .map(|edit| {
            let range = editor_byte(input, edit.range.start).ok_or(Error::InvalidExpression)?
                ..editor_byte(input, edit.range.end).ok_or(Error::InvalidExpression)?;
            Ok(Replacement {
                text: physical_newlines(&input[range.clone()], &edit.text, newline),
                range,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    EditPlan::from_replacements(input, edits).map_err(|_| Error::InvalidExpression)
}

/// When line count is unchanged, preserve every physical separator inside a multi-line edit as well.
fn physical_newlines(before: &str, after: &str, preferred: &str) -> String {
    let mut physical = Vec::new();
    let mut chars = before.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\r' if chars.peek() == Some(&'\n') => {
                chars.next();
                physical.push("\r\n");
            }
            '\r' => physical.push("\r"),
            '\n' => physical.push("\n"),
            _ => {}
        }
    }
    if physical.len() != after.matches('\n').count() {
        return after.replace('\n', preferred);
    }
    let mut separators = physical.into_iter();
    let mut output = String::with_capacity(after.len() + before.len().saturating_sub(after.len()));
    for part in after.split_inclusive('\n') {
        if let Some(line) = part.strip_suffix('\n') {
            output.push_str(line);
            output.push_str(separators.next().unwrap_or(preferred));
        } else {
            output.push_str(part);
        }
    }
    output
}

/// Map normalized UTF-8 boundaries without an allocation per character or rounding inside a CRLF pair.
fn editor_byte(input: &str, target: usize) -> Option<usize> {
    let mut normalized = 0;
    let mut chars = input.char_indices().peekable();
    while let Some((byte, character)) = chars.next() {
        if normalized == target {
            return Some(byte);
        }
        if character == '\r' {
            if chars.peek().is_some_and(|(_, next)| *next == '\n') {
                chars.next();
            }
            normalized += 1;
        } else {
            normalized += character.len_utf8();
        }
    }
    (normalized == target).then_some(input.len())
}

/// New unsupported syntax is rejected; identical opaque fragments survive edits elsewhere without regeneration.
fn checked_text(before: &str, old: &Model, after: &str, new: &Model) -> Result<(), Error> {
    if new.statements.is_empty() {
        return Err(Error::InvalidExpression);
    }
    let mut opaque = BTreeMap::new();
    for owner in old
        .statements
        .iter()
        .filter(|owner| owner.unavailable.is_some())
    {
        *opaque.entry(&before[owner.range.clone()]).or_insert(0usize) += 1;
    }
    for owner in &new.statements {
        if let Some(reason) = owner.unavailable {
            if reason == Unavailable::InvalidSyntax {
                return Err(Error::InvalidSyntax);
            }
            let count = opaque
                .get_mut(&after[owner.range.clone()])
                .ok_or(Error::Unsupported)?;
            *count = count.checked_sub(1).ok_or(Error::Unsupported)?;
        } else {
            for clause in &owner.clauses {
                for item in &clause.items {
                    complete(
                        &item.expression(after),
                        item.wildcard_source(after, clause).is_some(),
                    )?;
                }
            }
        }
    }
    Ok(())
}

/// Structural validation rejects obvious unfinished operands; types and names remain analyzer services.
fn complete(input: &str, wildcard: bool) -> Result<(), Error> {
    let tokens = lex::tokens(input)?;
    let tokens: Vec<_> = tokens
        .iter()
        .filter(|token| token.kind != Kind::Comment)
        .collect();
    let last = tokens.last().ok_or(Error::InvalidExpression)?;
    if matches!(
        last.kind,
        Kind::Symbol('+' | '-' | '/' | '=' | '<' | '>' | ',' | '.' | '&')
    ) || last.kind == Kind::Symbol('*') && !wildcard
        || last.keyword(
            input,
            &[
                "AS",
                "КАК",
                "AND",
                "И",
                "OR",
                "ИЛИ",
                "NOT",
                "НЕ",
                "BETWEEN",
                "МЕЖДУ",
                "IN",
                "В",
                "IS",
                "ЕСТЬ",
                "LIKE",
                "ПОДОБНО",
            ],
        ) && tokens
            .get(tokens.len().saturating_sub(2))
            .is_none_or(|token| token.kind != Kind::Symbol('.'))
    {
        return Err(Error::InvalidExpression);
    }
    Ok(())
}
