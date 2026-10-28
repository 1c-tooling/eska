//! Horizontal groups follow the whitespace emitted in the same formatting pass.

use super::{Options, formatted, options};

/// Removing blank lines must align newly adjacent rows without requiring another formatting pass.
#[test]
fn removed_blank_lines_join_call_and_assignment_tables_immediately() {
    let options = Options {
        max_empty_lines: 0,
        ..options()
    };
    assert_eq!(
        formatted("Call(\"A\",1);\n\nCall(\"Long\",2);\n", options.clone()),
        "Call(\"A\"   , 1);\nCall(\"Long\", 2);\n"
    );
    assert_eq!(
        formatted("A=1;\n\nLong=2;\n", options),
        "A    = 1;\nLong = 2;\n"
    );
}

/// Initializer separation uses the same final group boundaries as ordinary assignment alignment.
#[test]
fn removed_blank_lines_expose_initializer_dependencies_immediately() {
    let source = "Flag=1;\nObject=Factory();\n\nObject.A=1;\nObject.Long=2;\n";
    assert_eq!(
        formatted(
            source,
            Options {
                max_empty_lines: 0,
                ..options()
            }
        ),
        "Flag = 1;\nObject = Factory();\nObject.A    = 1;\nObject.Long = 2;\n"
    );
}

/// Calls collapsed by the selected wrapping policy must enter their table during the first pass.
#[test]
fn collapsed_multiline_calls_align_without_a_second_pass() {
    assert_eq!(
        formatted(
            "Call(\n\"A\",\n1);\nCall(\"Long\",2);\n",
            Options {
                preserve_existing_line_breaks: false,
                ..options()
            }
        ),
        "Call(\"A\"   , 1);\nCall(\"Long\", 2);\n"
    );
}

/// A break outside the argument list is collapsed even when argument breaks are preserved.
#[test]
fn split_callee_uses_its_rendered_single_line_layout() {
    assert_eq!(
        formatted("Call\n(\"A\",1);\nCall(\"Long\",2);\n", options()),
        "Call(\"A\"   , 1);\nCall(\"Long\", 2);\n"
    );
}

/// Retained expression breaks and opaque multiline literals must never receive table padding.
#[test]
fn retained_multiline_calls_remain_table_boundaries() {
    for source in [
        "Call(\n\"A\",1);\nCall(\"Long\",2);\n",
        "Call(\"A\n|B\",1);\nCall(\"Long\",2);\n",
        "Call(A\n+ B,1);\nCall(Long,2);\n",
    ] {
        assert_eq!(
            formatted(source, options()),
            formatted(
                source,
                Options {
                    align_call_arguments: false,
                    ..options()
                }
            )
        );
    }
}
