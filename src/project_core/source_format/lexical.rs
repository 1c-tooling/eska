//! Exact parser ranges retain namespace spelling and quoted punctuation without guessing XML text.

use std::ops::Range;

/// Native `QName` ranges end before attributes or closing punctuation.
pub(super) fn opening_name(input: &str, start: usize) -> Option<(&str, Range<usize>)> {
    let begin = start.checked_add(1)?;
    let tail = input.get(begin..)?;
    let length = tail.find(|character: char| {
        character.is_ascii_whitespace() || matches!(character, '/' | '>')
    })?;
    Some((&tail[..length], begin..begin + length))
}

/// Greater-than signs inside quoted attributes do not end the opening tag.
pub(super) fn opening_end(input: &str, range: Range<usize>) -> Option<usize> {
    let mut quote = None;
    for (offset, character) in input.get(range.clone())?.char_indices() {
        match (quote, character) {
            (None, '>') => return Some(range.start + offset + 1),
            (None, '\'' | '"') => quote = Some(character),
            (Some(delimiter), value) if delimiter == value => quote = None,
            _ => {}
        }
    }
    None
}
