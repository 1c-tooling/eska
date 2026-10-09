//! Both protocol consumers require unambiguous JSON independent of their request DTOs.

use super::{ParseFailure, parse_unique};

/// Depth and duplicate keys are rejected even in fields a caller would otherwise ignore.
#[test]
fn ambiguity_and_depth_are_invalid_while_syntax_errors_stay_distinct() {
    assert_eq!(
        parse_unique(br#"{"x":{"a":1,"a":2}}"#),
        Err(ParseFailure::Invalid)
    );
    assert_eq!(
        parse_unique(format!("{}0{}", "[".repeat(65), "]".repeat(65)).as_bytes()),
        Err(ParseFailure::Invalid)
    );
    assert_eq!(parse_unique(b"{"), Err(ParseFailure::Syntax));
    assert!(parse_unique(br#"{"ignored":[1,true,null,"{}"]}"#).is_ok());
}
