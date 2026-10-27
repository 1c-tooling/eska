//! These tests enforce mutation boundaries independently of the formatter process or IDE.

use super::*;
use crate::formatting::{
    Options,
    protocol::{Edit, source_sha256},
};

/// A cursor and Select All cannot grant whole-file formatting through the range endpoint.
#[test]
fn supported_sources_require_a_proper_nonempty_selection() {
    let text = "А=1;\nБ=2;\n";
    for range in [
        None,
        Some(Range { start: 0, end: 0 }),
        Some(Range {
            start: 0,
            end: text.len(),
        }),
        Some(Range { start: 1, end: 5 }),
    ] {
        assert_eq!(
            Access::SelectionOnly
                .check_range(text, range)
                .unwrap_err()
                .code,
            "selection_required"
        );
    }
    Access::SelectionOnly
        .check_range(text, Some(Range { start: 0, end: 5 }))
        .unwrap();
    Access::Unrestricted.check_range(text, None).unwrap();
}

/// A component is not trusted to keep its edits inside the native selection.
#[test]
fn syntax_expansion_is_rejected_even_with_valid_protocol_coordinates() {
    let request = Request {
        schema_version: 1,
        language: Language::Bsl,
        text: "A=1;\nB=2;\n".into(),
        options: Options::default(),
        range: Some(Range { start: 5, end: 9 }),
    };
    let mut response = Response {
        schema_version: 1,
        source_sha256: source_sha256(&request.text),
        edits: vec![Edit {
            range: Range { start: 5, end: 9 },
            text: "B = 2;".into(),
        }],
        formatted_range: Range {
            start: 0,
            end: request.text.len(),
        },
    };
    Access::SelectionOnly
        .check_response(&request, &response)
        .unwrap();
    response.edits[0].range.start = 4;
    assert_eq!(
        Access::SelectionOnly
            .check_response(&request, &response)
            .unwrap_err()
            .code,
        "selection_expansion"
    );
}

/// Protection recognizes bilingual annotations but ignores their spelling inside literals and comments.
#[test]
fn controlled_methods_include_annotations_comments_and_deleted_reference_code() {
    let method = "  &AtServer\r\n&cHaNgEaNdVaLiDaTe(\"Run\")\r\nProcedure Custom()\r\n#Delete\r\nEndProcedure\r\n#EndDelete\r\n#If Server Then\r\nA=1;\r\n#EndIf\r\nEndProcedure // tail  ";
    let input = format!(
        "\u{feff}// &ИзменениеИКонтроль\r\nS=\"&ChangeAndValidate\";\r\n{method}\r\nProcedure Other()\r\nB=2;\r\nEndProcedure\r\n"
    );
    let ranges = controlled_ranges(&input).unwrap();
    assert_eq!(ranges.len(), 1);
    assert_eq!(&input[ranges[0].clone()], method);
}

/// Branch-dependent method boundaries must fail closed instead of freezing only the first branch.
#[test]
fn conditional_method_ends_cannot_expose_a_controlled_branch() {
    let input = "&ИзменениеИКонтроль(\"Run\")\nПроцедура Run()\n#Если Сервер Тогда\nКонецПроцедуры\n#Иначе\nКонецПроцедуры\n#КонецЕсли\n";
    assert_eq!(
        controlled_ranges(input).unwrap_err().code,
        "unsupported_source"
    );
    let before_branches = "&ChangeAndValidate(\"Run\")\n#If Server Then\nProcedure Run()\nA=1;\nEndProcedure\n#Else\nProcedure Run()\nB=2;\nEndProcedure\n#EndIf\n";
    assert_eq!(
        controlled_ranges(before_branches).unwrap_err().field,
        "control_boundaries"
    );
}

/// A second annotation is still effective, while annotation-looking parameter strings remain inert.
#[test]
fn control_annotation_detection_ignores_quoted_and_commented_names() {
    for (header, protected) in [
        ("&AtServer &ChangeAndValidate(\"Run\")", true),
        ("&Before(\"&ChangeAndValidate\")", false),
        ("&AtServer // &ChangeAndValidate(\"Run\")", false),
    ] {
        let input = format!("{header}\nProcedure Run()\nA=1;\nEndProcedure\n");
        assert_eq!(!controlled_ranges(&input).unwrap().is_empty(), protected);
    }
}

/// Source encoding and ordinary publication cannot bypass the method prohibition.
#[test]
fn protected_queries_and_output_changes_are_rejected() {
    let input = "&ИзменениеИКонтроль(\"Run\")\nПроцедура Run()\nQ=\"select 1\";\nКонецПроцедуры\n";
    let query = query::read_bsl(input, input.find("select").unwrap()).unwrap();
    assert_eq!(
        check_query(input, &query.identity).unwrap_err().code,
        "change_control_protected"
    );
    assert_eq!(
        Access::Unrestricted
            .check_output(input, &input.replace("Q=", "Q = "), None, true)
            .unwrap_err()
            .code,
        "change_control_protected"
    );
    assert_eq!(
        Access::SelectionOnly
            .check_output(
                "A=1;\nB=2;",
                "A = 1;\nB = 2;",
                Some(Range { start: 5, end: 9 }),
                true
            )
            .unwrap_err()
            .code,
        "selection_expansion"
    );
}
