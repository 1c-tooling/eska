//! Source coordinates are independent of a transport, parser, filesystem and locale.

use super::*;

/// Selected boundaries preserve BOM, non-BMP scalars, CRLF, CR, LF, duplicates and EOF.
#[test]
fn exact_current_unicode_and_line_boundaries_are_captured_once() {
    let text = "\u{feff}Я😀\r\nZ\rR\n";
    let coordinates = TextCoordinates::new(text, &[15, 0, 3, 5, 9, 10, 11, 12, 13, 14, 5]).unwrap();
    assert_eq!(coordinates.stored_boundaries(), 10);
    let emoji = coordinates.range(5..9).unwrap();
    assert_eq!(
        emoji.start,
        TextPosition {
            line: 0,
            character: 2
        }
    );
    assert_eq!(
        emoji.end,
        TextPosition {
            line: 0,
            character: 4
        }
    );
    let cr = coordinates.range(9..10).unwrap();
    assert_eq!(cr.start, cr.end);
    assert_eq!(
        coordinates.range(11..13).unwrap().end,
        TextPosition {
            line: 2,
            character: 0
        }
    );
    assert_eq!(
        coordinates.range(14..15).unwrap().end,
        TextPosition {
            line: 3,
            character: 0
        }
    );
    assert_eq!(
        TextCoordinates::new("", &[0])
            .unwrap()
            .range(0..0)
            .unwrap()
            .start,
        TextPosition::default()
    );
    assert_eq!(
        TextCoordinates::new(text, &[]).unwrap().stored_boundaries(),
        0
    );
}

/// Invalid input never floors split scalars or synthesizes a plausible location.
#[test]
fn split_outside_unrecorded_and_reversed_ranges_remain_typed_errors() {
    for offset in [1, 6, 16, usize::MAX] {
        assert_eq!(
            TextCoordinates::new("\u{feff}Я😀\r\nZ\rR\n", &[offset]),
            Err(CoordinateError::InvalidBoundary { offset })
        );
    }
    let coordinates = TextCoordinates::new("xy", &[0, 2]).unwrap();
    assert_eq!(
        coordinates.range(0..1),
        Err(CoordinateError::NotCaptured { offset: 1 })
    );
    assert_eq!(
        coordinates.range(Range { start: 2, end: 0 }),
        Err(CoordinateError::ReversedRange { start: 2, end: 0 })
    );
}
