use ide::{Analysis, CompletionItem, CompletionItemKind};
use ide_db::base_db::{SourceDatabase, SourceRoot, SourceRootId};
use ide_db::RootDatabaseImpl;
use test_fixture::Fixture;
use vfs::{FileId, FileSet};

fn setup(fixture_text: &str) -> (Analysis, FileId, u32) {
    let (fixture_text, test_path, cursor_offset) = extract_cursor(fixture_text);
    let fixture = Fixture::parse(&fixture_text);

    let mut db = RootDatabaseImpl::new();
    let source_root_id = SourceRootId(0);

    let mut file_set = FileSet::default();
    for (file_id, file) in &fixture.files {
        file_set.insert(*file_id, file.path.clone());
    }
    let source_root = SourceRoot::new_local(file_set);
    db.set_source_root(source_root_id, source_root);

    for (file_id, file) in &fixture.files {
        db.set_file_source_root(*file_id, source_root_id);
        db.set_file_text(*file_id, &file.content);
    }

    let test_file = fixture
        .files
        .iter()
        .find(|(_, f)| f.path.as_path().to_string_lossy().ends_with(&test_path))
        .map(|(id, _)| *id)
        .expect("cursor-bearing file not found");

    (Analysis::from_database(db), test_file, cursor_offset)
}

fn extract_cursor(fixture_text: &str) -> (String, String, u32) {
    let abs_idx = fixture_text.find("$0").expect("fixture must contain $0 cursor marker");
    let prefix = &fixture_text[..abs_idx];
    let last_header_start = prefix.rfind("//- ").expect("cursor must be inside a //- file");
    let header_end =
        prefix[last_header_start..].find('\n').expect("//- header must end with newline")
            + last_header_start;
    let path_line = &prefix[last_header_start + 4..header_end];
    let file_offset_in_prefix = header_end + 1;
    let cursor_in_file = (abs_idx - file_offset_in_prefix) as u32;
    let cleaned = fixture_text.replacen("$0", "", 1);
    (cleaned, path_line.to_string(), cursor_in_file)
}

fn complete(fixture: &str) -> Vec<CompletionItem> {
    let (analysis, file_id, offset) = setup(fixture);
    analysis.completions(file_id, offset, None, ide::Locale::Ru)
}

/// A local value sharing a module's name must not inherit that module's exported methods.
#[test]
fn direct_common_module_completion_respects_local_receiver_ownership() {
    for body in [
        "Procedure Run(Utility)\n Value = Utility.$0;\nEndProcedure\n",
        "Procedure Run()\n Var Utility;\n Value = Utility.$0;\nEndProcedure\n",
        "Var Utility;\nProcedure Run()\n Value = Utility.$0;\nEndProcedure\n",
        "Procedure Run()\n Utility = 1;\n Value = Utility.$0;\nEndProcedure\n",
    ] {
        let text=format!("//- /test.bsl\n{body}//- /CommonModules/Utility/Ext/Module.bsl\nProcedure SavedExport() Export\nEndProcedure\n");
        assert!(!complete(&text).iter().any(|item| item.label == "SavedExport"), "{body}");
    }
}

/// Dependency discovery shares the direct native receiver and case-insensitive resident module identity.
#[test]
fn direct_common_module_dependency_selects_declared_owner_and_body_with_prefix() {
    let (analysis,file,offset)=setup("//- /test.bsl\nProcedure Run()\n Value = uTiLiTy.Sav$0;\nEndProcedure\n//- /CommonModules/Utility/Ext/Module.bsl\nProcedure SavedExport() Export\nEndProcedure\nProcedure HiddenMethod()\nEndProcedure\n//- /CommonModules/Neighbor/Ext/Module.bsl\nProcedure NeighborExport() Export\nEndProcedure\n");
    assert_eq!(
        analysis.completion_metadata_target(file, offset),
        Some((bsl_metadata::MdoType::CommonModule, "uTiLiTy".into()))
    );
    let module = analysis.completion_common_module(file, offset).unwrap();
    assert_ne!(module, file);
    assert_eq!(analysis.completion_manager_module(file, offset), None);
    let items = analysis.completions(file, offset, None, ide::Locale::En);
    assert!(items.iter().any(|item| item.label == "SavedExport"));
    assert!(!items
        .iter()
        .any(|item| item.label == "HiddenMethod" || item.label == "NeighborExport"));
}

/// Non-code tokens, shadowing, unknown modules and chained receivers do not request direct namespace payloads.
#[test]
fn direct_common_module_dependency_rejects_non_namespace_receivers() {
    for body in [
        "Procedure Run(Utility)\n Value = Utility.$0;\nEndProcedure\n",
        "Procedure Run()\n Value = \"Utility.$0\";\nEndProcedure\n",
        "Procedure Run()\n // Utility.$0\nEndProcedure\n",
        "Procedure Run()\n Value = Missing.$0;\nEndProcedure\n",
        "Procedure Run()\n Value = Utility.Factory().$0;\nEndProcedure\n",
    ] {
        let text=format!("//- /test.bsl\n{body}//- /CommonModules/Utility/Ext/Module.bsl\nProcedure SavedExport() Export\nEndProcedure\n");
        let (analysis, file, offset) = setup(&text);
        assert_eq!(analysis.completion_metadata_target(file, offset), None, "{body}");
        assert_eq!(analysis.completion_common_module(file, offset), None, "{body}");
    }
}

/// Removing a textual namespace override must preserve the actual typed local value's fields.
#[test]
fn shadowed_common_module_completion_uses_the_local_structure_value() {
    let items=complete("//- /test.bsl\nProcedure Run()\n Var Utility;\n Utility = New Structure(\"UserField\",1);\n Utility.$0;\nEndProcedure\n//- /CommonModules/Utility/Ext/Module.bsl\nProcedure SavedExport() Export\nEndProcedure\n");
    assert!(items.iter().any(|item| item.label == "UserField"));
    assert!(!items.iter().any(|item| item.label == "SavedExport"));
}

fn find_item<'a>(items: &'a [CompletionItem], label: &str) -> Option<&'a CompletionItem> {
    items.iter().find(|i| i.label == label)
}

fn count_label(items: &[CompletionItem], label: &str) -> usize {
    items.iter().filter(|i| i.label == label).count()
}

fn hbk_globals_available() -> bool {
    !bsl_platform::PlatformDataInner::instance().all_global_properties().is_empty()
}

#[test]
fn completion_hbk_property_appears_at_top_level() {
    if !hbk_globals_available() {
        return;
    }
    let items = complete(
        r#"//- /test.bsl
Процедура Тест()
    Мет$0
КонецПроцедуры
"#,
    );
    let meta = find_item(&items, "Метаданные").unwrap_or_else(|| {
        panic!(
            "Метаданные must appear in completion; got: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        )
    });
    assert_eq!(meta.kind, CompletionItemKind::Property);
    let detail = meta.detail.as_deref().unwrap_or("");
    assert!(detail.contains("ОбъектМетаданныхКонфигурация"), "detail = {detail:?}");
}

#[test]
fn completion_hbk_property_readonly_marker_in_detail() {
    if !hbk_globals_available() {
        return;
    }
    let items = complete(
        r#"//- /test.bsl
Процедура Тест()
    Обр$0
КонецПроцедуры
"#,
    );
    let prop = find_item(&items, "ОбработкаОшибок").unwrap_or_else(|| {
        panic!(
            "ОбработкаОшибок must appear; labels: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        )
    });
    let detail = prop.detail.as_deref().unwrap_or("");
    assert!(detail.ends_with("[Только чтение]"), "detail = {detail:?}");
}

#[test]
fn completion_bilingual_prefix_finds_property() {
    if !hbk_globals_available() {
        return;
    }
    let items = complete(
        r#"//- /test.bsl
Процедура Тест()
    Meta$0
КонецПроцедуры
"#,
    );
    assert!(
        find_item(&items, "Метаданные").is_some(),
        "Метаданные must surface for English 'Meta' prefix; labels: {:?}",
        items.iter().map(|i| &i.label).collect::<Vec<_>>()
    );
}

#[test]
fn completion_russian_english_collision_emits_once() {
    if !hbk_globals_available() {
        return;
    }
    let items = complete(
        r#"//- /test.bsl
Процедура Тест()
    Doc$0
КонецПроцедуры
"#,
    );
    assert_eq!(
        count_label(&items, "Документы"),
        1,
        "Документы must appear exactly once; labels: {:?}",
        items.iter().map(|i| &i.label).collect::<Vec<_>>()
    );
    let docs = find_item(&items, "Документы").unwrap();
    assert_eq!(docs.kind, CompletionItemKind::MdoType);
    assert!(
        find_item(&items, "Documents").is_none(),
        "must not emit separate `Documents` item; labels: {:?}",
        items.iter().map(|i| &i.label).collect::<Vec<_>>()
    );
}

#[test]
fn completion_mdo_plural_not_duplicated() {
    let items = complete(
        r#"//- /test.bsl
Процедура Тест()
    Док$0
КонецПроцедуры
"#,
    );
    assert_eq!(count_label(&items, "Документы"), 1);
    let docs = find_item(&items, "Документы").unwrap();
    assert_eq!(docs.kind, CompletionItemKind::MdoType);
    let detail = docs.detail.as_deref().unwrap_or("");
    assert!(detail.starts_with("Коллекция метаданных"), "detail = {detail:?}");
}

#[test]
fn completion_workspace_common_module_shadows_hbk_global() {
    if !hbk_globals_available() {
        return;
    }
    let items = complete(
        r#"//- /CommonModules/Метаданные/Ext/Module.bsl
Функция МойМетод() Экспорт
    Возврат 1;
КонецФункции

//- /test.bsl
Процедура Тест()
    Мет$0
КонецПроцедуры
"#,
    );
    let meta_items: Vec<&CompletionItem> =
        items.iter().filter(|i| i.label == "Метаданные").collect();
    assert_eq!(meta_items.len(), 1, "Метаданные must appear once; got {} items", meta_items.len());
    assert_eq!(
        meta_items[0].kind,
        CompletionItemKind::MdoObject,
        "workspace CommonModule must win over HBK property"
    );
}

#[test]
fn completion_sort_text_bands_stable() {
    let items = complete(
        r#"//- /test.bsl
Процедура Тест(Парам)
    Перем Лок;
    Сообщить($0);
КонецПроцедуры
"#,
    );
    // sort_text layout: <tier digit><band 2 digits>_<freq><score><label>.
    for s in items.iter().filter_map(|i| i.sort_text.as_deref()) {
        let tier = s.chars().next().unwrap_or(' ');
        assert!(
            ('0'..='3').contains(&tier),
            "sort_text must start with a quality tier 0..3; got {s:?}"
        );
        let band = s.get(1..).and_then(|rest| rest.split('_').next()).unwrap_or("");
        assert!(
            matches!(band, "00" | "10" | "15" | "18" | "20" | "24" | "25" | "30"),
            "unexpected band {band:?} in sort_text {s:?}"
        );
    }
}

#[test]
fn completion_local_shadows_hbk_global() {
    if !hbk_globals_available() {
        return;
    }
    let items = complete(
        r#"//- /test.bsl
Процедура Тест()
    Перем Метаданные;
    Мет$0
КонецПроцедуры
"#,
    );
    let meta_items: Vec<&CompletionItem> =
        items.iter().filter(|i| i.label == "Метаданные").collect();
    assert_eq!(meta_items.len(), 1, "Метаданные must appear once");
    assert_ne!(
        meta_items[0].kind,
        CompletionItemKind::Property,
        "local Перем must shadow HBK property; got kind {:?}",
        meta_items[0].kind
    );
}

#[test]
fn completion_workspace_common_module_appears_standalone() {
    let items = complete(
        r#"//- /CommonModules/МояБиблиотека/Ext/Module.bsl
Функция МойМетод() Экспорт
    Возврат 1;
КонецФункции

//- /test.bsl
Процедура Тест()
    Мо$0
КонецПроцедуры
"#,
    );
    let item = find_item(&items, "МояБиблиотека").unwrap_or_else(|| {
        panic!(
            "МояБиблиотека must appear; labels: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        )
    });
    assert_eq!(item.kind, CompletionItemKind::MdoObject);
}

#[test]
fn completion_english_cm_does_not_cross_alias_shadow_hbk_global() {
    if !hbk_globals_available() {
        return;
    }
    let items = complete(
        r#"//- /CommonModules/Metadata/Ext/Module.bsl
Функция Foo() Экспорт
    Возврат 1;
КонецФункции

//- /test.bsl
Процедура Тест()
    Мет$0
КонецПроцедуры
"#,
    );
    let meta = find_item(&items, "Метаданные").unwrap_or_else(|| {
        panic!(
            "HBK Метаданные must surface for RU prefix `Мет` even with EN-named CM in workspace; \
             labels: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        )
    });
    assert_eq!(meta.kind, CompletionItemKind::Property);
}

#[test]
fn completion_global_function_not_shadowed_by_same_named_cm() {
    if bsl_platform::PlatformDataInner::instance().all_global_functions().is_empty() {
        return;
    }
    let items = complete(
        r#"//- /CommonModules/НачатьТранзакцию/Ext/Module.bsl
Функция Foo() Экспорт
    Возврат 1;
КонецФункции

//- /test.bsl
Процедура Тест()
    Начат$0
КонецПроцедуры
"#,
    );
    let fn_item = items
        .iter()
        .find(|i| i.label == "НачатьТранзакцию" && i.kind == CompletionItemKind::Function);
    assert!(
        fn_item.is_some(),
        "platform function `НачатьТранзакцию` must surface even with same-named workspace CM; \
         labels: {:?}",
        items
            .iter()
            .filter(|i| i.label == "НачатьТранзакцию")
            .map(|i| (&i.label, i.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn completion_global_function_sorts_after_mdo_plural() {
    let items = complete(
        r#"//- /CommonModules/ОбщегоНазначения/Ext/Module.bsl
Функция Знач() Экспорт
    Возврат 1;
КонецФункции

//- /test.bsl
Процедура Тест()
    Сообщить($0);
КонецПроцедуры
"#,
    );

    let docs_sort = find_item(&items, "Документы").and_then(|i| i.sort_text.clone());
    let begin_tx_sort = find_item(&items, "НачатьТранзакцию").and_then(|i| i.sort_text.clone());
    let cm_sort = find_item(&items, "ОбщегоНазначения").and_then(|i| i.sort_text.clone());

    if let (Some(docs), Some(begin)) = (docs_sort.as_ref(), begin_tx_sort.as_ref()) {
        assert!(
            docs.as_str() < begin.as_str(),
            "MDO plural sort_text {:?} must precede global function sort_text {:?}",
            docs,
            begin
        );
    }
    if let (Some(begin), Some(cm)) = (begin_tx_sort.as_ref(), cm_sort.as_ref()) {
        assert!(
            begin.as_str() < cm.as_str(),
            "global function sort_text {:?} must precede workspace CM sort_text {:?}",
            begin,
            cm
        );
    }
}

/// A user symbol holding the name of a platform global must not surface that
/// global's members. Each shape below reaches the receiver through a different
/// route: an implicit local is typed by inference, a declared `Перем` makes
/// inference return `Unknown`, and both must end up with no platform members.
#[test]
fn implicit_local_holding_global_name_offers_no_platform_members() {
    let items = complete(
        r#"//- /test.bsl
Функция Тест()
    ОбработкаОшибок = НеизвестнаяФункция();
    ОбработкаОшибок.$0
КонецФункции
"#,
    );
    assert!(
        !items.iter().any(|i| i.label == "КраткоеПредставлениеОшибки"),
        "a local holds the name — the global's members must not be offered; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn declared_local_holding_global_name_offers_no_platform_members() {
    let items = complete(
        r#"//- /test.bsl
Функция Тест()
    Перем ОбработкаОшибок;
    ОбработкаОшибок.$0
КонецФункции
"#,
    );
    assert!(
        !items.iter().any(|i| i.label == "КраткоеПредставлениеОшибки"),
        "a declared local holds the name — the global's members must not be offered; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn implicit_local_holding_metadata_name_offers_no_metadata_members() {
    let items = complete(
        r#"//- /test.bsl
Функция Тест()
    Метаданные = НеизвестнаяФункция();
    Метаданные.$0
КонецФункции
"#,
    );
    assert!(
        !items.iter().any(|i| i.label == "НайтиПоПолномуИмени"),
        "a local holds `Метаданные` — configuration-metadata members must not be offered; got: {:?}",
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}

/// Positive control for the three tests above: with nobody holding the name the
/// very same receivers still complete against the platform globals.
#[test]
fn unheld_global_names_still_offer_platform_members() {
    let error_items = complete(
        r#"//- /test.bsl
Функция Тест()
    ОбработкаОшибок.$0
КонецФункции
"#,
    );
    assert!(
        error_items.iter().any(|i| i.label == "КраткоеПредставлениеОшибки"),
        "an unheld global property must still complete; got: {:?}",
        error_items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );

    let metadata_items = complete(
        r#"//- /test.bsl
Функция Тест()
    Метаданные.$0
КонецФункции
"#,
    );
    assert!(
        metadata_items.iter().any(|i| i.label == "НайтиПоПолномуИмени"),
        "an unheld `Метаданные` must still complete; got: {:?}",
        metadata_items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
}
