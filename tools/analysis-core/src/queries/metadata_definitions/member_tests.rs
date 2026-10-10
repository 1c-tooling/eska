//! Nested metadata identities come from typed receivers and exact parents, not field spelling alone.

use super::*;
use crate::session::tests::{draft, fixture};
use eska::{analysis_core::InputRequest, project_core::source_format::NativeUuid};

const CALLER: &str = "Catalogs/Sample/Ext/ObjectModule.bsl";

/// Every field cursor selects a complete ASCII fixture character inside the actual occurrence.
fn query(
    session: &AnalysisSession,
    caller: &SourcePath,
    body: &str,
    field: &str,
) -> Option<MetadataLocation> {
    let at = u32::try_from(body.rfind(field).unwrap() + 1).unwrap();
    session
        .module_metadata_definition(
            caller,
            session.revision(),
            at,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
}

/// One canonical fixture covers enum values, attributes, tabular sections and row columns.
#[test]
fn typed_metadata_members_resolve_their_current_nested_uuid_in_the_correct_parent() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let request = InputRequest::default();
    for path in [
        "Catalogs/Sample/object.yaml",
        "Catalogs/Other/object.yaml",
        "Enums/Choice/object.yaml",
    ] {
        let owner = SourcePath::parse(path).unwrap();
        session
            .load_saved(&owner, session.revision(), &request)
            .unwrap();
    }
    for (version, body, field, path, uuid) in [
        (
            1,
            "Объект=Catalogs.Other.CreateItem();\nЗначение=Объект.oTHERfIELD;",
            "oTHERfIELD",
            "Catalogs/Other/object.yaml",
            "30000000-0000-4000-8000-000000000009",
        ),
        (
            2,
            "Объект=Catalogs.Sample.CreateItem();\nЗначение=Объект.iTEMS;",
            "iTEMS",
            "Catalogs/Sample/object.yaml",
            "40000000-0000-4000-8000-000000000001",
        ),
        (
            3,
            "Объект=Catalogs.Sample.CreateItem();\nСтрока=Объект.Items.Add();\nЗначение=Строка.cOUNT;",
            "cOUNT",
            "Catalogs/Sample/object.yaml",
            "50000000-0000-4000-8000-000000000001",
        ),
        (
            4,
            "Значение=Enums.Choice.fIRST;",
            "fIRST",
            "Enums/Choice/object.yaml",
            "30000000-0000-4000-8000-000000000008",
        ),
        (
            5,
            "Менеджер=Перечисления.Choice;\nЗначение=Менеджер.First;",
            "First",
            "Enums/Choice/object.yaml",
            "30000000-0000-4000-8000-000000000008",
        ),
    ] {
        let body = format!(
            "\u{feff}// Я😀\r\nПроцедура Тест()\r\n{}\r\nКонецПроцедуры\r\n",
            body.replace('\n', "\r\n")
        );
        session
            .apply_module_overlay(&caller, session.revision(), version, &body, &request)
            .unwrap();
        let target = query(&session, &caller, &body, field).unwrap();
        let path = SourcePath::parse(path).unwrap();
        assert_eq!(target.source, path);
        assert_eq!(target.uuid, uuid);
        let direct = session
            .metadata_location(
                &path,
                session.revision(),
                &NativeUuid::parse(uuid).unwrap(),
                None,
            )
            .unwrap();
        assert_eq!(target, direct);
    }
}

/// Platform properties, unrelated owners and structural fields never become same-named metadata.
#[test]
fn member_navigation_does_not_guess_platform_structure_or_sibling_targets() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let request = InputRequest::default();
    for path in [
        "Catalogs/Sample/object.yaml",
        "Catalogs/Other/object.yaml",
        "Enums/Choice/object.yaml",
    ] {
        let owner = SourcePath::parse(path).unwrap();
        session
            .load_saved(&owner, session.revision(), &request)
            .unwrap();
    }
    for (version, body, field) in [
        (
            1,
            "Объект=Catalogs.Other.CreateItem();\nЗначение=Объект.Code;",
            "Code",
        ),
        (
            2,
            "Объект=Новый Структура(\"OtherField\", 1);\nЗначение=Объект.OtherField;",
            "OtherField",
        ),
        (
            3,
            "Объект=Catalogs.Sample.CreateItem();\nЗначение=Объект.OtherField;",
            "OtherField",
        ),
        (
            4,
            "Объект=Catalogs.Other.CreateItem();\nОбъект=Новый Структура;\nЗначение=Объект.OtherField;",
            "OtherField",
        ),
        (5, "Значение=Перечисления.Choice.Missing;", "Missing"),
    ] {
        let body = format!("Процедура Тест()\n{body}\nКонецПроцедуры\n");
        session
            .apply_module_overlay(&caller, session.revision(), version, &body, &request)
            .unwrap();
        assert!(query(&session, &caller, &body, field).is_none(), "{body}");
    }
}

/// Renaming an inline enum value clears the old semantic target and close restores the saved one.
#[test]
fn enum_member_navigation_tracks_renamed_current_yaml_without_stale_lookup() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let owner = SourcePath::parse("Enums/Choice/object.yaml").unwrap();
    let request = InputRequest::default();
    session
        .load_saved(&owner, session.revision(), &request)
        .unwrap();
    let body = "Процедура Тест()\nЗначение=Перечисления.Choice.First;\nКонецПроцедуры\n";
    session
        .apply_module_overlay(&caller, session.revision(), 1, body, &request)
        .unwrap();
    let first = query(&session, &caller, body, "First").unwrap();
    let changed = draft(&root, &owner, "First", "Second");
    session
        .apply_overlay(&owner, session.revision(), 7, &changed, &request)
        .unwrap();
    assert!(query(&session, &caller, body, "First").is_none());
    let current = body.replace("First", "Second");
    session
        .apply_module_overlay(&caller, session.revision(), 2, &current, &request)
        .unwrap();
    let second = query(&session, &caller, &current, "Second").unwrap();
    assert_eq!(second.uuid, first.uuid);
    assert_ne!(second.source_sha256, first.source_sha256);
    session
        .apply_overlay(&owner, session.revision(), 8, "broken: [\n", &request)
        .unwrap();
    assert!(query(&session, &caller, &current, "Second").is_none());
    session
        .close_overlay(&owner, session.revision(), &request)
        .unwrap();
    assert!(query(&session, &caller, &current, "Second").is_none());
    session
        .apply_module_overlay(&caller, session.revision(), 3, body, &request)
        .unwrap();
    let restored = query(&session, &caller, body, "First").unwrap();
    assert_eq!(restored.uuid, first.uuid);
    assert_eq!(restored.range, first.range);
    assert_eq!(restored.source_sha256, first.source_sha256);
}
