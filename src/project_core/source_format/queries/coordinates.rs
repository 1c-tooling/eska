//! Exact XML value tokens map to decoded UTF-8 bytes without interpreting query language.

use std::ops::Range;

use super::{Error, NativeQuery};

#[cfg(test)]
mod tests;

/// The inverse recipe must reproduce the supplied node before its offsets can address plain bytes.
pub(super) fn decoded_range(
    query: &NativeQuery,
    input: &str,
    range: &Range<usize>,
) -> Result<Option<Range<usize>>, Error> {
    if input
        .get(query.native_range())
        .is_none_or(|node| query.frame.render(&query.text) != node)
        || !input.is_char_boundary(range.start)
        || !input.is_char_boundary(range.end)
        || range.start > range.end
    {
        return Err(invalid());
    }
    let Some(body) = query.frame.content_range() else {
        return Ok(Some(0..0));
    };
    if range.start < body.start || range.end > body.end {
        return Ok(None);
    }
    let content = input.get(body.clone()).ok_or_else(invalid)?;
    Ok(Some(positions(content, &query.text, body.start, range)?))
}

/// CDATA delimiters supply no characters; raw XML CR/CRLF and numeric references stay distinct.
fn positions(
    content: &str,
    source: &str,
    base: usize,
    range: &Range<usize>,
) -> Result<Range<usize>, Error> {
    let (mut start, mut end) = (None, None);
    let (mut cursor, mut decoded, mut cdata) = (0, 0, false);
    while cursor < content.len() {
        let rest = content.get(cursor..).ok_or_else(invalid)?;
        if cdata && rest.starts_with("]]>") {
            cdata = false;
            cursor += 3;
            continue;
        }
        if !cdata && rest.starts_with("<![CDATA[") {
            cdata = true;
            cursor += 9;
            continue;
        }
        let (character, length) = if !cdata && rest.starts_with('&') {
            let end = rest.find(';').ok_or_else(invalid)?;
            (entity(&rest[1..end])?, end + 1)
        } else if rest.starts_with('\r') {
            ('\n', if rest.starts_with("\r\n") { 2 } else { 1 })
        } else {
            let character = rest.chars().next().ok_or_else(invalid)?;
            (character, character.len_utf8())
        };
        if source.get(decoded..).and_then(|rest| rest.chars().next()) != Some(character) {
            return Err(invalid());
        }
        let end_source = decoded + character.len_utf8();
        let native = base + cursor..base + cursor + length;
        if start.is_none() && range.start < native.end {
            start = Some(decoded);
        }
        if end.is_none() {
            if range.end <= native.start {
                end = Some(decoded);
            } else if range.end <= native.end {
                end = Some(end_source);
            }
        }
        if let (Some(start), Some(end)) = (start, end) {
            return Ok(start..if range.is_empty() { start } else { end });
        }
        cursor += length;
        decoded = end_source;
    }
    if cdata || decoded != source.len() {
        return Err(invalid());
    }
    let start = start.unwrap_or(source.len());
    Ok(start..if range.is_empty() {
        start
    } else {
        end.unwrap_or(source.len())
    })
}

/// Built-in XML entities and numeric references have exactly one decoded character.
fn entity(value: &str) -> Result<char, Error> {
    match value {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "quot" => Ok('"'),
        "apos" => Ok('\''),
        _ => {
            let number = if let Some(hex) = value.strip_prefix("#x") {
                u32::from_str_radix(hex, 16)
            } else {
                value.strip_prefix('#').ok_or_else(invalid)?.parse()
            }
            .map_err(|_| invalid())?;
            char::from_u32(number).ok_or_else(invalid)
        }
    }
}

/// No approximate location survives a changed byte sequence or an unverified lexical transform.
fn invalid() -> Error {
    Error::root("source_map_origin_mismatch")
}
