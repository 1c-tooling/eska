use super::*;
use crate::project::query::constructor::TextTarget;

/// Editor newline normalization must never turn opening/applying an untouched buffer into a rewrite.
#[test]
fn editor_buffer_keeps_bom_crlf_and_unchanged_opaque_text() {
    let original =
        "\u{feff}SELECT CASE WHEN X THEN 1 ELSE 0 END AS A\r\n; // keep 😀\r\nSELECT 2 AS B\r\n";
    let mut value = constructor(original);
    let normalized = original.replace("\r\n", "\n");
    let plan = value
        .apply(
            1,
            &Action::SetText {
                text: normalized.clone(),
                editor_normalized: true,
            },
        )
        .unwrap();
    assert!(plan.is_empty());
    assert_eq!(value.text(), original);
    assert_eq!(value.revision(), 1);
    apply(
        &mut value,
        &Action::SetText {
            text: normalized.replace("2 AS B", "3 AS B"),
            editor_normalized: true,
        },
    );
    assert_eq!(value.text(), original.replace("2 AS B", "3 AS B"));
    assert_eq!(value.revision(), 2);
    let diff = value.text_diff().unwrap();
    assert_eq!(diff.original, original);
    let change = diff.change.unwrap();
    assert_eq!(&original[change.before], "2");
    assert_eq!(&value.text()[change.after], "3");
}

/// Separate edits around unchanged lines do not normalize a mixed-EOL source in the intervening hunk.
#[test]
fn editor_buffer_preserves_mixed_separators_with_unchanged_line_count() {
    let original = "\u{feff}SELECT 1 AS A,\r\n// keep 😀\n2 AS B,\r3 AS C\r\n";
    let mut value = constructor(original);
    apply(
        &mut value,
        &Action::SetText {
            text: original
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .replace("1 AS A", "4 AS A")
                .replace("3 AS C", "5 AS C"),
            editor_normalized: true,
        },
    );
    assert_eq!(
        value.text(),
        original
            .replace("1 AS A", "4 AS A")
            .replace("3 AS C", "5 AS C")
    );
}

/// Invalid and newly unsupported input leave the working snapshot and revision intact for correction.
#[test]
fn direct_text_failures_are_atomic_and_complete_text_publishes_once() {
    let mut value = constructor("SELECT 1 AS N");
    for text in [
        "SELECT \"unfinished",
        "SELECT 1 +",
        "SELECT FROM",
        "SELECT CASE WHEN X THEN 1 END",
    ] {
        assert!(
            value
                .apply(
                    1,
                    &Action::SetText {
                        text: text.into(),
                        editor_normalized: true
                    }
                )
                .is_err()
        );
        assert_eq!(value.text(), "SELECT 1 AS N");
        assert_eq!(value.revision(), 1);
    }
    apply(
        &mut value,
        &Action::SetText {
            text: "SELECT 2 AS N; SELECT 3 AS N".into(),
            editor_normalized: false,
        },
    );
    assert_eq!(value.revision(), 2);
    assert_eq!(value.model().scopes.len(), 2);
    assert_eq!(
        value
            .apply(
                1,
                &Action::SetText {
                    text: "SELECT 9".into(),
                    editor_normalized: false
                }
            )
            .unwrap_err(),
        Error::StaleRevision
    );
    assert_eq!(value.revision(), 2);
}

/// UTF-16 location rejects a half-surrogate while all source byte ranges remain UTF-8 scalar boundaries.
#[test]
fn text_locations_and_comment_pages_share_exact_unicode_coordinates() {
    let value = constructor("SELECT \"😀\" AS N");
    let location = value
        .locate(&TextTarget::Item {
            selection: selection(0, 0),
        })
        .unwrap();
    assert_eq!(location.utf16.start, 7);
    assert_eq!(location.utf16.end, 16);
    assert_eq!(location.range.start, 7);
    assert_eq!(location.range.end, 18);
    assert_eq!(
        value.locate(&TextTarget::Offset { offset: 9 }).unwrap_err(),
        Error::InvalidSelection
    );
    let value = constructor("SELECT \"😀\" AS N\r\n// outside 😀\r\nFROM T // source\r\n");
    let first = value.comment_page(0, 1, None).unwrap();
    assert_eq!(first.total, 2);
    assert_eq!(first.next_offset, Some(1));
    let second = value.comment_page(1, 1, None).unwrap();
    assert_eq!(second.next_offset, None);
    assert_eq!(&value.text()[second.comments[0].range.clone()], "// source");
    assert!(second.comments[0].selection.is_some());
    let range = second.comments[0].range.clone();
    let located = value
        .locate(&TextTarget::Range {
            range: range.clone(),
        })
        .unwrap();
    assert_eq!(located.range, range);
    assert_eq!(located.utf16, second.comments[0].utf16);
}

/// Text navigation selects the nested statement rather than its containing source row.
#[test]
fn reverse_text_navigation_enters_the_innermost_scope() {
    let value = constructor("SELECT Q.N FROM (SELECT 7 AS N) AS Q");
    let byte = value.text().find('7').unwrap();
    let offset = value.text()[..byte].encode_utf16().count();
    let location = value.locate(&TextTarget::Offset { offset }).unwrap();
    assert_eq!(location.statement, 1);
    assert_eq!(location.selection.unwrap().statement, 1);
}
