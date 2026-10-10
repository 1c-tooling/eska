//! Union navigation requires one shared declaration rather than the first visible field.

use base_db::RootQueryDb;
use eska::analysis_core::InputRequest;
use hir::{NameClass, Semantics, TypeKernelDb, TypeKind};

use super::*;
use crate::session::tests::{draft, fixture};

const CALLER: &str = "Catalogs/Sample/Ext/ObjectModule.bsl";
const OWNER: &str = "Catalogs/Other/object.yaml";

/// The cursor selects a full Unicode character within the final field occurrence.
fn cursor(body: &str, field: &str) -> u32 {
    u32::try_from(body.rfind(field).unwrap() + field.chars().next().unwrap().len_utf8()).unwrap()
}

/// Verify real inference produces a union; navigation must not be tested with a single inferred type.
fn assert_union(session: &AnalysisSession, path: &SourcePath, at: u32) {
    session
        .query_module(
            path,
            session.revision(),
            &QueryCancellation::default(),
            |analysis, file| {
                let parse = analysis.database().parse(file);
                let token = parse
                    .syntax_node()
                    .token_at_offset(at.into())
                    .right_biased()
                    .unwrap();
                let NameClass::FieldName { receiver, .. } = hir::classify_token(&token) else {
                    panic!("expected field receiver");
                };
                let ty = Semantics::new(analysis.database()).type_of_expr(file, &receiver);
                assert!(
                    matches!(analysis.database().lookup_type(ty), TypeKind::Union(_)),
                    "{:?}",
                    analysis.database().lookup_type(ty)
                );
                Ok(())
            },
        )
        .unwrap();
}

/// Nullable metadata receivers and object/reference variants share their real current child UUID.
#[test]
fn common_union_member_resolves_the_one_current_declaration() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let owner = SourcePath::parse(OWNER).unwrap();
    let request = InputRequest::default();
    session
        .load_saved(&owner, session.revision(), &request)
        .unwrap();
    for (version, choice) in [
        "СправочникОбъект.Other, Неопределено",
        "Null, СправочникОбъект.Other",
        "СправочникОбъект.Other, СправочникСсылка.oThEr",
    ]
    .into_iter()
    .enumerate()
    {
        let body = format!(
            "\u{feff}// Я😀\r\n// Возвращаемое значение:\r\n//  {choice} - Тест.\r\nФункция Данные()\r\nКонецФункции\r\nПроцедура Тест()\r\nОбъект=Данные();\r\nЗначение=Объект.oTHERfIELD;\r\nКонецПроцедуры\r\n"
        );
        session
            .apply_module_overlay(
                &caller,
                session.revision(),
                i64::try_from(version + 1).unwrap(),
                &body,
                &request,
            )
            .unwrap();
        let at = cursor(&body, "oTHERfIELD");
        assert_union(&session, &caller, at);
        let target = tests::query(&session, &caller, at).value.unwrap();
        assert_eq!(target.source, owner);
        assert_eq!(target.uuid.as_str(), "30000000-0000-4000-8000-000000000009");
    }
}

/// Unsupported concrete alternatives are never discarded to fabricate a common metadata target.
#[test]
fn union_with_structure_primitive_or_another_owner_does_not_choose_a_first_target() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let request = InputRequest::default();
    for path in [OWNER, "Catalogs/Sample/object.yaml"] {
        session
            .load_saved(
                &SourcePath::parse(path).unwrap(),
                session.revision(),
                &request,
            )
            .unwrap();
    }
    let sibling = SourcePath::parse("Catalogs/Sample/object.yaml").unwrap();
    let same_named_field = draft(&root, &sibling, "Description", "OtherField");
    session
        .apply_overlay(&sibling, session.revision(), 7, &same_named_field, &request)
        .unwrap();
    for (version, other) in ["Структура", "Число", "СправочникОбъект.Sample"]
        .into_iter()
        .enumerate()
    {
        let body = format!(
            "// Возвращаемое значение:\n//  СправочникОбъект.Other, {other} - Тест.\nФункция Данные()\nКонецФункции\nПроцедура Тест()\nОбъект=Данные();\nЗначение=Объект.OtherField;\nКонецПроцедуры\n"
        );
        session
            .apply_module_overlay(
                &caller,
                session.revision(),
                i64::try_from(version + 1).unwrap(),
                &body,
                &request,
            )
            .unwrap();
        let at = cursor(&body, "OtherField");
        assert_union(&session, &caller, at);
        assert!(
            tests::query(&session, &caller, at).value.is_none(),
            "{body}"
        );
    }
}

/// Replacing or invalidating the current owner affects every union arm in the same revision.
#[test]
fn union_member_follows_overlay_invalid_and_close_without_old_targets() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let owner = SourcePath::parse(OWNER).unwrap();
    let request = InputRequest::default();
    session
        .load_saved(&owner, session.revision(), &request)
        .unwrap();
    let body = "// Возвращаемое значение:\n//  СправочникОбъект.Other, СправочникСсылка.Other - Тест.\nФункция Данные()\nКонецФункции\nПроцедура Тест()\nОбъект=Данные();\nЗначение=Объект.OtherField;\nКонецПроцедуры\n";
    session
        .apply_module_overlay(&caller, session.revision(), 1, body, &request)
        .unwrap();
    let first = tests::query(&session, &caller, cursor(body, "OtherField"))
        .value
        .unwrap();
    let editor = format!(
        "# Я😀\r\n{}",
        draft(&root, &owner, "OtherField", "Renamed").replace('\n', "\r\n")
    );
    session
        .apply_overlay(&owner, session.revision(), 7, &editor, &request)
        .unwrap();
    assert!(
        tests::query(&session, &caller, cursor(body, "OtherField"))
            .value
            .is_none()
    );
    let current = body.replace("OtherField", "Renamed");
    session
        .apply_module_overlay(&caller, session.revision(), 2, &current, &request)
        .unwrap();
    let changed = tests::query(&session, &caller, cursor(&current, "Renamed"))
        .value
        .unwrap();
    assert_eq!(changed.uuid, first.uuid);
    assert_ne!(changed.source_sha256, first.source_sha256);
    session
        .apply_overlay(&owner, session.revision(), 8, "broken: [\n", &request)
        .unwrap();
    assert!(
        tests::query(&session, &caller, cursor(&current, "Renamed"))
            .value
            .is_none()
    );
    session
        .close_overlay(&owner, session.revision(), &request)
        .unwrap();
    session
        .apply_module_overlay(&caller, session.revision(), 3, body, &request)
        .unwrap();
    let restored = tests::query(&session, &caller, cursor(body, "OtherField"))
        .value
        .unwrap();
    assert_eq!(restored.revision, session.revision());
    assert_eq!(restored.uuid, first.uuid);
    assert_eq!(restored.source_sha256, first.source_sha256);
    assert_eq!(restored.range, first.range);
    assert_eq!(restored.byte_range, first.byte_range);
    session
        .invalidate_saved_inputs(&[owner], session.revision())
        .unwrap();
    assert!(
        tests::query(&session, &caller, cursor(body, "OtherField"))
            .value
            .is_none()
    );
}
