//! Conservative BSL routine snapshots for semantic comparison.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum RoutineKind {
    Method,
    Function,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RoutineSnapshot {
    pub(super) kind: RoutineKind,
    pub(super) name: String,
    pub(super) body: String,
}

/// Parse only unambiguous top-level BSL declarations and their complete bodies.
pub(super) fn parse_routines(
    contents: &[u8],
) -> Option<BTreeMap<(RoutineKind, String), RoutineSnapshot>> {
    let text = std::str::from_utf8(contents).ok()?;
    let mut lines = text.lines();
    let mut routines = BTreeMap::new();
    while let Some(line) = lines.next() {
        let Some((kind, name)) = routine_declaration(line) else {
            continue;
        };
        let mut body = line.trim_end().to_owned();
        loop {
            let line = lines.next()?;
            body.push('\n');
            body.push_str(line.trim_end());
            if routine_end(line, kind) {
                break;
            }
        }
        let key = (kind, name.to_lowercase());
        if routines
            .insert(key, RoutineSnapshot { kind, name, body })
            .is_some()
        {
            return None;
        }
    }
    Some(routines)
}

/// Recognize Russian and English BSL declaration keywords at the start of a line.
fn routine_declaration(line: &str) -> Option<(RoutineKind, String)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return None;
    }
    let lowered = trimmed.to_lowercase();
    let async_prefix = ["асинх ", "async "]
        .into_iter()
        .find(|prefix| lowered.starts_with(prefix));
    let declaration = async_prefix.map_or(trimmed, |prefix| &trimmed[prefix.len()..]);
    let lowered = declaration.to_lowercase();
    let (kind, keyword) = [
        (RoutineKind::Method, "процедура "),
        (RoutineKind::Method, "procedure "),
        (RoutineKind::Function, "функция "),
        (RoutineKind::Function, "function "),
    ]
    .into_iter()
    .find(|(_, keyword)| lowered.starts_with(keyword))?;
    let remainder = &declaration[keyword.len()..];
    let name = remainder.split_once('(')?.0.trim();
    (!name.is_empty()
        && name
            .chars()
            .all(|value| value == '_' || value.is_alphanumeric()))
    .then(|| (kind, name.to_owned()))
}

/// Recognize the matching Russian or English end keyword.
fn routine_end(line: &str, kind: RoutineKind) -> bool {
    let lowered = line.trim_start().to_lowercase();
    let keyword = match kind {
        RoutineKind::Method => ["конецпроцедуры", "endprocedure"],
        RoutineKind::Function => ["конецфункции", "endfunction"],
    };
    keyword.iter().any(|keyword| {
        lowered.strip_prefix(keyword).is_some_and(|suffix| {
            suffix.is_empty()
                || suffix.starts_with(char::is_whitespace)
                || suffix.starts_with(';')
                || suffix.starts_with("//")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{RoutineKind, parse_routines};

    /// BSL parsing distinguishes procedure and function lifecycle without matching comments.
    #[test]
    fn parses_complete_russian_and_english_routines() {
        let routines = parse_routines(
            "// Процедура Ложная()\nПроцедура Выполнить()\nКонецПроцедуры\nFunction Value()\n    Return 1;\nEndFunction\n"
                .as_bytes(),
        )
        .expect("valid routines");

        assert!(routines.contains_key(&(RoutineKind::Method, "выполнить".to_owned())));
        assert!(routines.contains_key(&(RoutineKind::Function, "value".to_owned())));
        assert_eq!(routines.len(), 2);
    }

    /// Incomplete BSL is rejected so callers retain only the reliable module event.
    #[test]
    fn rejects_incomplete_routine_body() {
        assert!(parse_routines("Процедура Выполнить()\n".as_bytes()).is_none());
    }

    /// Line endings and trailing spaces are presentation; indentation and body text are not.
    #[test]
    fn normalizes_line_endings_without_changing_body_content() {
        let source = "Async Function Value()\n    Return 1;\nEndFunction";
        let expected = parse_routines(source.as_bytes()).unwrap();
        let formatted = "Async Function Value()  \r\n    Return 1;\t\r\nEndFunction \r\n";
        assert_eq!(parse_routines(formatted.as_bytes()).unwrap(), expected);
        assert_eq!(expected.values().next().unwrap().body, source);
        assert_ne!(
            parse_routines(source.replace("    Return", "  Return").as_bytes()).unwrap(),
            expected
        );
    }

    /// Ambiguous names and malformed input retain the module-level fallback.
    #[test]
    fn rejects_duplicates_invalid_utf8_and_wrong_end_keyword() {
        assert!(
            parse_routines(b"Procedure Run()\nEndProcedure\nProcedure RUN()\nEndProcedure")
                .is_none()
        );
        assert!(parse_routines(b"\xff").is_none());
        assert!(parse_routines(b"Function Value()\nEndProcedure").is_none());
        assert!(parse_routines(b"// no routines\r\n").unwrap().is_empty());
    }
}
