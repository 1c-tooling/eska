//! Exact native facts retain package shape and refuse unproven names/types.

use ide_db::{
    base_db::{SourceDatabase, SourceRoot, SourceRootId},
    RootDatabaseImpl,
};
use syntax::TextSize;
use vfs::{FileId, FileSet, VfsPath};

use super::*;

/// No source text is registered: the standalone route must work without a synthetic BSL body.
fn analysis() -> Analysis {
    let mut db = RootDatabaseImpl::new();
    let mut files = FileSet::default();
    files.insert(FileId(0), VfsPath::from(std::path::PathBuf::from("/query-output/owner.xml")));
    db.set_file_source_root(FileId(0), SourceRootId(0));
    db.set_source_root(SourceRootId(0), SourceRoot::new_local(files));
    db.set_file_unreadable(FileId(0));
    Analysis::from_database(db)
}

/// Every returned range slices the caller's exact UTF-8 bytes, including CRLF and non-BMP prefixes.
fn model(text: &str) -> QueryTextModel {
    let analysis = analysis();
    analysis.attached(|analysis| analysis.query_text_model(FileId(0), text))
}

/// SELECT helpers assert package identity rather than silently skipping DROP or missing branches.
fn select(model: &QueryTextModel, index: usize) -> &QuerySelectStatement {
    let QueryTextStatement::Select(select) = &model.statements[index] else {
        panic!("expected SELECT statement")
    };
    select
}

/// Native byte offsets preserve aliases, duplicates and literal semicolons without string splitting.
#[test]
fn output_fields_keep_order_types_and_exact_unicode_ranges() {
    let text = "// 😀 &Comment\r\nВЫБРАТЬ 1 КАК Повтор, \"a;b😀\" КАК Повтор";
    let result = model(text);
    assert!(result.syntax_complete);
    assert!(result.parameters.is_empty());
    assert_eq!(result.statements.len(), 1);
    let select = select(&result, 0);
    assert!(select.output_names_complete && select.output_types_complete);
    assert_eq!(select.branches.len(), 1);
    let fields = &select.branches[0].fields;
    assert_eq!(fields.len(), 2);
    for (ordinal, field) in fields.iter().enumerate() {
        assert_eq!(field.ordinal, ordinal);
        assert_eq!(field.name.as_deref(), Some("Повтор"));
        assert_eq!(field.name_origin, QueryOutputNameOrigin::Alias);
        assert_eq!(&text[field.name_range.unwrap()], "Повтор");
        assert!(text[field.range].contains("КАК"));
    }
    assert!(matches!(fields[0].ty, SdblType::Number { .. }));
    assert!(matches!(fields[1].ty, SdblType::String { .. }));
    assert_eq!(&text[fields[1].expression_range], "\"a;b😀\"");
}

/// Flattened HIR UNION branches cannot become separate result sets or merged complete types.
#[test]
fn package_boundaries_keep_temporary_select_union_and_drop() {
    let text = "ВЫБРАТЬ 1 КАК Поле ПОМЕСТИТЬ ВТ; ВЫБРАТЬ Поле КАК Результат ИЗ ВТ ОБЪЕДИНИТЬ ВСЕ ВЫБРАТЬ 2 КАК Другое; УНИЧТОЖИТЬ ВТ";
    let result = model(text);
    assert!(result.syntax_complete);
    assert!(!result.temporary_table_model_complete);
    assert_eq!(result.statements.len(), 3);
    assert_eq!(select(&result, 0).into_table.as_deref(), Some("ВТ"));
    let union = select(&result, 1);
    assert_eq!(union.branches.len(), 2);
    assert_eq!(union.branches[0].fields[0].name.as_deref(), Some("Результат"));
    assert_eq!(union.branches[1].fields[0].name.as_deref(), Some("Другое"));
    assert!(union.output_names_complete && union.union_arity_consistent);
    assert!(!union.output_types_complete);
    assert!(matches!(result.statements[2], QueryTextStatement::Drop { .. }));
    for branch in &union.branches {
        assert!(union.range.contains_range(branch.range));
        assert_eq!(
            &text[branch.fields[0].name_range.unwrap()],
            branch.fields[0].name.as_deref().unwrap()
        );
    }
}

/// Unknown expression names and unresolved stars cannot prove that a consumer field is absent.
#[test]
fn stars_heuristic_names_parse_errors_and_union_arity_are_incomplete() {
    let result = model("SELECT T.*, T.Field + T.Other FROM Catalog.Items AS T");
    let output = select(&result, 0);
    assert!(!output.output_names_complete && !output.output_types_complete);
    assert!(output.branches[0].fields[0].is_asterisk);
    let expression = &output.branches[0].fields[1];
    assert_eq!(expression.name, None);
    assert_eq!(expression.name_origin, QueryOutputNameOrigin::Unknown);
    assert!(expression.raw_name.is_some());
    let broken = model("SELECT 1 AS A UNION ALL SELECT 2 AS B, 3 AS C");
    assert!(!select(&broken, 0).union_arity_consistent);
    assert!(!select(&broken, 0).output_names_complete);
    let broken = model("SELECT 1 AS");
    assert!(!broken.syntax_complete);
    assert!(!select(&broken, 0).output_names_complete);
}

/// Parameter occurrences use AST nodes in all clauses, excluding comment/string lookalikes.
#[test]
fn parameters_preserve_occurrences_spelling_and_name_ranges() {
    let text = "// &Comment 😀\r\nSELECT \"&String\" AS A FROM Catalog.Items AS T WHERE T.Code = &Параметр OR T.Code = &параметр";
    let result = model(text);
    assert!(result.syntax_complete);
    assert_eq!(
        result.parameters.iter().map(|parameter| parameter.name.as_str()).collect::<Vec<_>>(),
        ["Параметр", "параметр"]
    );
    for parameter in result.parameters {
        assert_eq!(&text[parameter.name_range], parameter.name);
        assert_eq!(&text[parameter.range], format!("&{}", parameter.name));
        assert_eq!(parameter.name_range.start(), parameter.range.start() + TextSize::from(1));
    }
}

/// Nested SELECT and JOIN parameters stay occurrences without becoming extra package outputs.
#[test]
fn nested_parameters_do_not_create_additional_result_sets() {
    let text = "SELECT T.Code AS Code FROM Catalog.Items AS T INNER JOIN Catalog.Items AS R ON T.Code = &JoinValue WHERE T.Code IN (SELECT X.Code FROM Catalog.Items AS X WHERE X.Code = &NestedValue)";
    let result = model(text);
    assert!(result.syntax_complete);
    assert_eq!(result.statements.len(), 1);
    assert_eq!(select(&result, 0).branches.len(), 1);
    assert_eq!(
        result.parameters.iter().map(|parameter| parameter.name.as_str()).collect::<Vec<_>>(),
        ["JoinValue", "NestedValue"]
    );
}

/// A native unresolved defined/composite type is not complete merely because its outer variant is known.
#[test]
fn nested_unknown_types_remain_incomplete() {
    for ty in [
        SdblType::DefinedType { name: "Missing".into(), underlying_type: None },
        SdblType::Aggregate(Box::new(SdblType::Unknown)),
        SdblType::Composite { types: vec![SdblType::Boolean, SdblType::Error] },
        SdblType::Composite { types: Vec::new() },
    ] {
        assert!(!type_complete(&ty));
    }
    assert!(type_complete(&SdblType::DefinedType {
        name: "Flag".into(),
        underlying_type: Some(Box::new(SdblType::Boolean)),
    }));
}

/// BOM and broken input keep original coordinates even when the parser refuses completeness.
#[test]
fn bom_and_empty_input_never_shift_native_ranges() {
    let text = "\u{feff}// 😀\r\nSELECT 1 AS Alias";
    let result = model(text);
    assert_eq!(result.statements.len(), 1);
    for statement in &result.statements {
        if let QueryTextStatement::Select(statement) = statement {
            for branch in &statement.branches {
                for field in &branch.fields {
                    if let Some(range) = field.name_range {
                        assert_eq!(&text[range], "Alias");
                    }
                }
            }
        }
    }
    let empty = model("");
    assert!(empty.statements.is_empty());
    assert!(empty.parameters.is_empty());
}

/// Cancellation at the caller's current generation refuses facts before parsing or metadata resolution.
#[test]
fn cancelled_native_generation_cannot_return_a_model() {
    let analysis = analysis();
    let token = analysis.database().cancellation_token();
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        analysis.attached(|analysis| {
            token.cancel();
            analysis.query_text_model(FileId(0), "SELECT 1 AS A")
        })
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)));
}

/// Native diagnostics can be empty for unknown calls; a known enclosing type cannot hide that gap.
#[test]
fn unknown_calls_in_nested_and_non_output_expressions_remain_unsupported() {
    for text in [
        "SELECT UnknownFunction(1) AS Value",
        "SELECT UnknownFunction(1) + 2 AS Value",
        "SELECT 1 AS Value WHERE UnknownFunction(1) = 2",
        "SELECT 1 AS Value UNION ALL SELECT UnknownFunction(1) AS Other",
        "SELECT CASE WHEN 1 = 1 THEN 1 ELSE UnknownFunction(1) END AS Value",
    ] {
        let result = model(text);
        assert!(result.syntax_complete, "{text}");
        assert!(!result.native_expressions_supported, "{text}");
    }
    assert!(
        model("SELECT UPPER(\"x\") AS Value WHERE 1 = &Parameter").native_expressions_supported
    );
}
