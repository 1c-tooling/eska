//! Receiver groups preserve operation order and separate complete preparation and population runs.

use super::{Options, Range, Request, apply, formatted, options};

/// The supplied procedure keeps result assignments together between the query and context blocks.
#[test]
fn supplied_query_and_context_groups_have_outer_separators() {
    let source = include_str!("fixtures/receiver-groups.input.bsl");
    let expected = include_str!("fixtures/receiver-groups.expected.bsl");
    assert_eq!(formatted(source, options()), expected);
    assert_eq!(
        formatted(
            &format!("\u{feff}{}", source.replace('\n', "\r\n")),
            options()
        ),
        format!("\u{feff}{}", expected.replace('\n', "\r\n"))
    );
}

/// Different methods and field assignments can populate the same already-existing receiver.
#[test]
fn existing_receivers_and_mixed_member_operations_form_groups() {
    let source = "Before();\nContext.Value=1;\nContext.Insert(\"A\",2);\nContext.Reset();\nAfter(Context);\n";
    assert_eq!(
        formatted(source, options()),
        "Before();\n\nContext.Value = 1;\nContext.Insert(\"A\", 2);\nContext.Reset();\n\nAfter(Context);\n"
    );
    let source = "Before();\nQuery=New Query;\nQuery.SetParameter(\"A\",1);\nQuery.SetParameter(\"Long\",2);\nAfter();\n";
    assert_eq!(
        formatted(source, options()),
        "Before();\n\nQuery = New Query;\nQuery.SetParameter(\"A\"   , 1);\nQuery.SetParameter(\"Long\", 2);\n\nAfter();\n"
    );
}

/// RHS calls and argument references do not attach unrelated statements to a receiver's block.
#[test]
fn right_hand_sides_and_one_off_calls_do_not_create_groups() {
    let source = "Before();\nFirst=Query.Execute();\nSecond=Query.Execute();\nUse(Query,Context);\nQuery.Close();\nContext.Close();\nAfter();\n";
    assert_eq!(
        formatted(source, options()),
        "Before();\nFirst  = Query.Execute();\nSecond = Query.Execute();\nUse(Query, Context);\nQuery.Close();\nContext.Close();\nAfter();\n"
    );
}

/// A repeated initialization starts another object group even when its variable name stays the same.
#[test]
fn repeated_initializers_split_previous_population_runs() {
    let source =
        "Object=Factory();\nObject.Set(1);\nObject=Factory();\nObject.Set(2);\nUse(Object);\n";
    assert_eq!(
        formatted(source, options()),
        "Object = Factory();\nObject.Set(1);\n\nObject = Factory();\nObject.Set(2);\n\nUse(Object);\n"
    );
}

/// Receiver paths include nested fields and indices while names remain case-insensitive.
#[test]
fn nested_and_indexed_receivers_keep_exact_identities() {
    for receiver in [
        "Context.Document",
        "Rows[Index]",
        "Rows[Position()]",
        "Rows[\"A\"]",
    ] {
        let source = format!("Before();\n{receiver}.Value=1;\n{receiver}.Reset();\nAfter();\n");
        let expected =
            format!("Before();\n\n{receiver}.Value = 1;\n{receiver}.Reset();\n\nAfter();\n");
        assert_eq!(formatted(&source, options()), expected);
    }
    assert_eq!(
        formatted(
            "Before();\nObject.Set(1);\nobject.Reset();\nAfter();\n",
            options()
        ),
        "Before();\n\nObject.Set(1);\nobject.Reset();\n\nAfter();\n"
    );
    for source in [
        "Rows[\"A\"].Set();\nRows[\"a\"].Reset();\n",
        "Obj.Set();\nObj2.Reset();\n",
        "Factory().Set();\nFactory().Reset();\n",
    ] {
        assert_eq!(formatted(source, options()), source);
    }
    assert_eq!(
        formatted("Rows[\"A\"] = Factory();\nRows[\"a\"].X = 1;\n", options()),
        "Rows[\"A\"]   = Factory();\nRows[\"a\"].X = 1;\n"
    );
}

/// Comments stay with their operations; branch transitions and formatting fences stop a run.
#[test]
fn comments_control_flow_and_disabled_regions_are_group_boundaries() {
    let source = "Before();\n// Populate the context.\nContext.Set(1);\n// Complete population.\nContext.Reset();\n// Continue work.\nAfter();\n";
    let expected = "Before();\n\n// Populate the context.\nContext.Set(1);\n// Complete population.\nContext.Reset();\n\n// Continue work.\nAfter();\n";
    assert_eq!(formatted(source, options()), expected);
    let source = "If Flag Then\nContext.Set(1);\nElse\nContext.Reset();\nEndIf;\n";
    assert_eq!(
        formatted(source, options()),
        source.replacen("Context.Set", "\tContext.Set", 1).replacen(
            "Context.Reset",
            "\tContext.Reset",
            1
        )
    );
    let source =
        "Context.Set(1);\n// eska-fmt off\nContext.Reset(  );\n// eska-fmt on\nContext.Set(2);\n";
    assert_eq!(formatted(source, options()), source);
}

/// Multiline calls remain within their receiver group and can use spacing independently of that group.
#[test]
fn object_group_spacing_has_an_independent_opt_out() {
    let source = "Before();\nContext.Set(\n1,\n2);\nContext.Reset();\nAfter();\n";
    let policy = Options {
        blank_lines_around_multiline_statements: 0,
        ..options()
    };
    assert_eq!(
        formatted(source, policy.clone()),
        "Before();\n\nContext.Set(\n\t1,\n\t2\n);\nContext.Reset();\n\nAfter();\n"
    );
    assert_eq!(
        formatted(
            source,
            Options {
                blank_lines_between_object_groups: 0,
                ..policy
            }
        ),
        "Before();\nContext.Set(\n\t1,\n\t2\n);\nContext.Reset();\nAfter();\n"
    );
}

/// Formatting one selected operation cannot publish separators into the unselected group boundaries.
#[test]
fn selected_group_operation_keeps_its_unselected_neighbors() {
    let source = "Before();\nContext.Set(1);\nContext.Reset( 2 );\nAfter();\n";
    let start = source.find("Context.Reset").unwrap();
    let end = source.find("\nAfter").unwrap();
    let request = Request {
        schema_version: 1,
        language: super::Language::Bsl,
        text: source.into(),
        options: options(),
        range: Some(Range { start, end }),
    };
    let response = crate::format(&request).unwrap();
    response.validate(&request).unwrap();
    assert_eq!(
        apply(source, &response),
        "Before();\nContext.Set(1);\nContext.Reset(2);\nAfter();\n"
    );
}
