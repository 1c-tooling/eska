//! Actual semantic receiver types select canonical YAML, while shadowed names keep BSL ownership.

use super::*;
use crate::session::tests::fixture;
use eska::analysis_core::InputRequest;
use std::fs;

const CALLER: &str = "Catalogs/Sample/Ext/ObjectModule.bsl";
const OWNER: &str = "Catalogs/Sample/object.yaml";

/// Pick a byte inside a fixture identifier, never punctuation at the adjacent AST token.
fn offset(text: &str, name: &str) -> u32 {
    u32::try_from(text.find(name).unwrap() + 1).unwrap()
}

/// Ordinary reads always use the caller's current snapshot and a fresh request control.
pub(super) fn query(
    session: &AnalysisSession,
    caller: &SourcePath,
    at: u32,
) -> ModuleResult<Option<MetadataLocation>> {
    session
        .module_metadata_definition(
            caller,
            session.revision(),
            at,
            &QueryCancellation::default(),
        )
        .unwrap()
}

/// A canonical location follows editor changes and close, and does not reread its YAML file.
#[test]
fn metadata_definition_uses_current_canonical_object_and_exact_owner_coordinates() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let owner = SourcePath::parse(OWNER).unwrap();
    let request = InputRequest::default();
    let body = "\u{feff}// Я😀\r\nПроцедура Тест()\r\nОбъект=СПРАВОЧНИКИ.sAmPlE.СоздатьЭлемент();\r\nКонецПроцедуры\r\n";
    session
        .apply_module_overlay(&caller, session.revision(), 1, body, &request)
        .unwrap();
    let at = offset(body, "sAmPlE");
    assert!(query(&session, &caller, at).value.is_none());
    session
        .load_saved(&owner, session.revision(), &request)
        .unwrap();
    let saved = fs::read_to_string(root.join(OWNER)).unwrap();
    let first = query(&session, &caller, at);
    let target = first.value.unwrap();
    assert_eq!(target.source, owner);
    assert_eq!(target.native_path.as_str(), "Catalogs/Sample.xml");
    assert!(saved[target.byte_range.clone()].contains(&target.uuid));
    assert!(!first.metadata_models_complete);
    assert!(!first.module_inputs_complete);
    let original = session.revision();
    let draft = format!(
        "# Я😀 sAmPlE is comment text\r\n{}",
        saved.replace('\n', "\r\n")
    );
    session
        .apply_overlay(&owner, original, 7, &draft, &request)
        .unwrap();
    let editor = query(&session, &caller, at).value.unwrap();
    assert_eq!(editor.range.start.line, target.range.start.line + 1);
    assert_ne!(editor.source_sha256, target.source_sha256);
    fs::remove_file(root.join(OWNER)).unwrap();
    assert_eq!(query(&session, &caller, at).value, Some(editor));
    session
        .apply_overlay(&owner, session.revision(), 8, "broken: [\n", &request)
        .unwrap();
    assert!(query(&session, &caller, at).value.is_none());
    fs::write(root.join(OWNER), saved).unwrap();
    session
        .close_overlay(&owner, session.revision(), &request)
        .unwrap();
    let restored = query(&session, &caller, at).value.unwrap();
    assert_eq!(restored.range, target.range);
    assert_eq!(restored.source_sha256, target.source_sha256);
    assert!(matches!(
        session.module_metadata_definition(&caller, original, at, &QueryCancellation::default()),
        Err(QueryError::Request(_))
    ));
    let cancelled = QueryCancellation::default();
    cancelled.cancel();
    assert_eq!(
        session.module_metadata_definition(&caller, session.revision(), at, &cancelled),
        Err(QueryError::Cancelled)
    );
}

/// Inferred collection aliases resolve, while a same-named local or parameter never becomes metadata.
#[test]
fn collection_alias_and_shadowed_bindings_have_distinct_semantic_targets() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let owner = SourcePath::parse(OWNER).unwrap();
    let request = InputRequest::default();
    session
        .load_saved(&owner, session.revision(), &request)
        .unwrap();
    let body =
        "Процедура Тест()\nКоллекция=Справочники;\nЗначение=Коллекция.sAmPlE;\nКонецПроцедуры\n";
    session
        .apply_module_overlay(&caller, session.revision(), 1, body, &request)
        .unwrap();
    assert_eq!(
        query(&session, &caller, offset(body, "sAmPlE"))
            .value
            .unwrap()
            .source,
        owner
    );
    for (version, shadow) in [
        (
            2,
            "Процедура Тест()\nПерем Справочники;\nСправочники=Новый Структура;\nЗначение=Справочники.Sample;\nКонецПроцедуры\n",
        ),
        (
            3,
            "Процедура Тест(Справочники)\nЗначение=Справочники.Sample;\nКонецПроцедуры\n",
        ),
        (
            4,
            "Перем Справочники;\nПроцедура Тест()\nЗначение=Справочники.Sample;\nКонецПроцедуры\n",
        ),
    ] {
        session
            .apply_module_overlay(&caller, session.revision(), version, shadow, &request)
            .unwrap();
        assert!(
            query(&session, &caller, offset(shadow, "Sample"))
                .value
                .is_none(),
            "{shadow}"
        );
    }
    assert!(matches!(
        session.module_metadata_definition(
            &caller,
            session.revision(),
            1,
            &QueryCancellation::default()
        ),
        Err(QueryError::InvalidOffset { offset: 1 })
    ));
}

/// Collection type, not its spelling, selects each supported canonical family.
#[test]
fn metadata_targets_follow_collection_types_for_catalogs_constants_enums_and_registers() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let request = InputRequest::default();
    for (version, body, name, path) in [
        (
            1,
            "Значение=Константы.Flag;",
            "Flag",
            "Constants/Flag/object.yaml",
        ),
        (
            2,
            "Значение=Перечисления.Choice.First;",
            "Choice",
            "Enums/Choice/object.yaml",
        ),
        (
            3,
            "Значение=РегистрыСведений.Rates;",
            "Rates",
            "InformationRegisters/Rates/object.yaml",
        ),
        (
            4,
            "Перем Справочники;\nСправочники=Перечисления;\nЗначение=Справочники.Choice;",
            "Choice",
            "Enums/Choice/object.yaml",
        ),
        // A write to a read-only global collection is invalid BSL, not an implicit declaration.
        (
            5,
            "Справочники=Новый Структура;\nЗначение=Справочники.Sample;",
            "Sample",
            OWNER,
        ),
    ] {
        let owner = SourcePath::parse(path).unwrap();
        session
            .load_saved(&owner, session.revision(), &request)
            .unwrap();
        let body = format!("Процедура Тест()\n{body}\nКонецПроцедуры\n");
        session
            .apply_module_overlay(&caller, session.revision(), version, &body, &request)
            .unwrap();
        let result = query(&session, &caller, offset(&body, name)).value.unwrap();
        assert_eq!(result.source, owner);
    }
}
