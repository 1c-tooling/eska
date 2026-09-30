//! Binding regressions protect unrelated identifiers and exact byte coordinates in bilingual modules.

use super::*;

/// A verified configuration has no hidden global declarations in these focused scope fixtures.
fn globals() -> GlobalBindings {
    GlobalBindings {
        complete: true,
        ..GlobalBindings::default()
    }
}

/// Analyze the same resolved catalog identity independently of the module's display language.
fn analyze(input: &str) -> XmlAnalysis {
    BslRename::new(&[("Catalog".into(), "Товары".into())], "Номенклатура").analyze(
        input,
        &globals(),
        true,
    )
}

/// Every replacement is a UTF-8 range in original BOM/CRLF bytes, not a reconstructed token stream.
#[test]
fn manager_and_metadata_roots_keep_exact_byte_ranges() {
    let input = "\u{feff}Procedure Run()\r\n A = Справочники.Товары.ПустаяСсылка();\r\n B = catalogs.товары;\r\n C = Metadata.Catalogs.Товары;\r\n D = Метаданные.Справочники.Товары;\r\nEndProcedure\r\n";
    let output = analyze(input);
    assert_eq!(output.replacements.len(), 4);
    assert!(output.uncertain.is_empty());
    for change in output.replacements {
        assert_eq!(input[change.range], change.before);
        assert_eq!(change.after, "Номенклатура");
    }
}

/// A dotted platform collection reached through an arbitrary value is not a global binding.
#[test]
fn foreign_receivers_and_different_collections_remain_uncertain() {
    let input = "A = Connection.Catalogs.Товары; B = ThisObject.Metadata.Catalogs.Товары; C = Documents.Товары; D = Catalogs.Другой.Товары; E = Get().Catalogs.Товары;";
    let output = analyze(input);
    assert!(output.replacements.is_empty());
    assert_eq!(output.uncertain.len(), 5);
}

/// Local parameters, explicit variables, assignments and iterators all defeat a global interpretation.
#[test]
fn local_shadowing_does_not_leak_into_another_routine() {
    for declaration in [
        "Procedure Run(Catalogs)",
        "Procedure Run(Val Catalogs = Undefined)",
        "Procedure Run() Var Catalogs;",
        "Procedure Run() Catalogs = New Structure;",
        "Procedure Run() For Each Catalogs In Values Do EndDo;",
    ] {
        let input = format!(
            "{declaration}\n A = Catalogs.Товары; EndProcedure\nProcedure Other() A = Catalogs.Товары; EndProcedure"
        );
        let output = analyze(&input);
        assert_eq!(output.replacements.len(), 1, "{declaration}");
        assert_eq!(output.uncertain.len(), 1, "{declaration}");
        assert_eq!(output.uncertain[0].reason, "bsl_shadowed_name");
    }
}

/// Module variables and procedures share a context with all routines regardless of source order.
#[test]
fn module_declarations_and_dynamic_code_block_bindings() {
    for prefix in [
        "Var Catalogs;",
        "Procedure Catalogs() EndProcedure",
        "Execute(\"Var Catalogs;\");",
    ] {
        let output = analyze(&format!(
            "{prefix}\nProcedure Run() A = Catalogs.Товары; EndProcedure"
        ));
        assert!(output.replacements.is_empty(), "{prefix}");
        assert_eq!(output.uncertain[0].reason, "bsl_shadowed_name");
    }
    let output = analyze(
        "Procedure Run() Execute(Code); A = Catalogs.Товары; EndProcedure\nProcedure Other() A = Catalogs.Товары; EndProcedure",
    );
    assert_eq!(output.replacements.len(), 1);
}

/// Quotes, doubled quotes, multiline strings, comments and compiler directives are different source regions.
#[test]
fn strings_comments_and_directives_never_become_code() {
    let input = "// Catalogs.Товары\r\n#Region Товары\r\n&Before(\"Товары\")\r\nProcedure Run()\r\n A = \"Catalogs.Товары \"\"quoted\"\"\r\n | Catalogs.Товары\";\r\n B = Catalogs // preserve trivia\r\n .Товары;\r\nEndProcedure\r\n#EndRegion";
    let output = analyze(input);
    assert_eq!(output.replacements.len(), 1);
    assert_eq!(output.uncertain.len(), 5);
    assert_eq!(output.uncertain[0].reason, "bsl_comment");
    assert_eq!(output.uncertain[1].reason, "bsl_directive");
    assert_eq!(output.uncertain[3].reason, "bsl_literal");
    for item in output.uncertain {
        assert_eq!(input[item.range], item.text);
    }
}

/// Compiler branch separators cannot join fragments into a fabricated access chain.
#[test]
fn incomplete_or_split_code_cannot_authorize_replacements() {
    for input in [
        "Procedure Run() A = Catalogs.Товары;",
        "A = \"broken Catalogs.Товары",
        "Catalogs\n#If Server Then\n.Товары;\n#EndIf",
        "A = 1Catalogs.Товары;",
        "A = 1@Catalogs.Товары;",
    ] {
        assert!(analyze(input).replacements.is_empty(), "{input}");
    }
}

/// A same-spelled enum value is bound only through the specified enumeration's manager.
#[test]
fn enum_values_require_the_resolved_owner() {
    let reference = BslRename::new(
        &[
            ("Enum".into(), "Статусы".into()),
            ("EnumValue".into(), "Готов".into()),
        ],
        "Завершен",
    );
    let output = reference.analyze("A = Перечисления.Статусы.Готов; B = Enums.Статусы.Готов; C = Enums.Другой.Готов; D = Metadata.Enums.Статусы.Готов;", &globals(), true);
    assert_eq!(output.replacements.len(), 2);
    assert_eq!(output.uncertain.len(), 2);
}

/// Common module names require a verified non-global module and a direct exported-method access.
#[test]
fn common_module_calls_check_project_binding_and_shadowing() {
    let reference = BslRename::new(&[("CommonModule".into(), "Сервис".into())], "ОбщийСервис");
    let mut bindings = globals();
    bindings.common_modules.insert("Сервис".into());
    let input = "Procedure Run() Сервис.Выполнить(); C = Сервис; D = Obj.Сервис.Выполнить(); EndProcedure\nProcedure Other(Сервис) Сервис.Выполнить(); EndProcedure";
    let output = reference.analyze(input, &bindings, true);
    assert_eq!(output.replacements.len(), 1);
    assert_eq!(output.uncertain.len(), 4); // Includes the same-spelled local declaration.
    bindings.common_modules.clear();
    assert!(
        reference
            .analyze(input, &bindings, true)
            .replacements
            .is_empty()
    );
}

/// Missing base/global context or unchecked form attributes cannot be replaced by assumptions.
#[test]
fn unknown_contexts_and_global_collisions_remain_uncertain() {
    let reference = BslRename::new(&[("Catalog".into(), "Товары".into())], "Новые");
    let input = "A = Catalogs.Товары;";
    assert!(
        reference
            .analyze(input, &globals(), false)
            .replacements
            .is_empty()
    );
    assert!(
        reference
            .analyze(input, &GlobalBindings::default(), true)
            .replacements
            .is_empty()
    );
    let mut bindings = globals();
    bindings.shadowed.insert("catalogs".into());
    assert!(
        reference
            .analyze(input, &bindings, true)
            .replacements
            .is_empty()
    );
    bindings.shadowed.clear();
    bindings.common_modules.insert("catalogs".into());
    assert!(
        reference
            .analyze(input, &bindings, true)
            .replacements
            .is_empty()
    );
}

/// Scope boundaries include parameter defaults and bilingual async declarations without external dependencies.
#[test]
fn reads_parameters_defaults_and_global_names() {
    let input = "Перем Общая, Другая Экспорт; Асинх Функция Получить(Знач П = \"abc\", Ч = -1.25, Д = '20260101') Экспорт Возврат Catalogs.Товары; КонецФункции";
    assert_eq!(analyze(input).replacements.len(), 1);
    assert_eq!(
        module_names(input).unwrap(),
        BTreeSet::from(["Общая".into(), "Другая".into(), "Получить".into()])
    );
}

/// Unicode case-equivalent identifiers must shadow each other even if lowercase spellings differ.
#[test]
fn unicode_common_module_names_respect_case_equivalent_locals() {
    let reference = BslRename::new(&[("CommonModule".into(), "Σ".into())], "Новый");
    let mut bindings = globals();
    bindings.common_modules.insert("Σ".into());
    let output = reference.analyze("Procedure Run(ς) Σ.Method(); EndProcedure", &bindings, true);
    assert!(output.replacements.is_empty());
    assert!(
        output
            .uncertain
            .iter()
            .any(|item| item.reason == "bsl_shadowed_name")
    );
}

/// A new common-module name must not rebind a proven call to an existing local variable.
#[test]
fn common_module_destination_cannot_shadow_an_existing_binding() {
    let reference = BslRename::new(&[("CommonModule".into(), "Сервис".into())], "Новый");
    let mut bindings = globals();
    bindings.common_modules.insert("Сервис".into());
    let output = reference.analyze(
        "Procedure Run(Новый) Сервис.Method(); EndProcedure",
        &bindings,
        true,
    );
    assert!(output.replacements.is_empty());
    assert_eq!(output.uncertain[0].reason, "bsl_destination_shadowed");
}

/// Oversized token streams retain candidates without spending memory on an unbounded syntax model.
#[test]
fn bounded_lexer_preserves_uncertain_matches_when_it_reaches_its_limit() {
    let input = format!("{}Catalogs.Товары;", "A;".repeat(500_000));
    let output = analyze(&input);
    assert!(output.replacements.is_empty());
    assert_eq!(output.uncertain.len(), 1);
    assert_eq!(output.uncertain[0].reason, "bsl_lexical_unavailable");
}
