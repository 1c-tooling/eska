//! Current local and cross-module BSL declarations never borrow saved or native XML positions.

use eska::analysis_core::InputRequest;

use super::*;
use crate::session::tests::fixture;

const CALLER: &str = "Catalogs/Sample/Ext/ObjectModule.bsl";
const TARGET: &str = "CommonModules/Tools/Ext/Module.bsl";

/// The real kernel sees one current declaration in a declared ready body, including Unicode/EOLs.
#[test]
fn current_definition_targets_follow_editor_close_and_cross_module_input_changes() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let target = SourcePath::parse(TARGET).unwrap();
    let caller = SourcePath::parse(CALLER).unwrap();
    let body = "\u{feff}// Я😀\r\nПроцедура Цель() Экспорт\r\nКонецПроцедуры\r\nЦель();\r\n";
    let request = InputRequest::default();
    session
        .apply_module_overlay(&target, session.revision(), 1, body, &request)
        .unwrap();
    let previous = session.revision();
    let local = session
        .module_definition(
            &target,
            previous,
            u32::try_from(body.rfind("Цель").unwrap()).unwrap(),
            &QueryCancellation::default(),
        )
        .unwrap();
    let found = local.value.unwrap();
    assert_eq!(found.source, target);
    assert!(body[found.start_byte as usize..found.end_byte as usize].contains("Процедура Цель"));
    assert_eq!(found.source_sha256, local.source_sha256);
    assert!(!local.metadata_models_complete);
    let text = "Процедура Вызов()\r\nTools.Цель();\r\nКонецПроцедуры\r\n";
    session
        .apply_module_overlay(&caller, session.revision(), 1, text, &request)
        .unwrap();
    session
        .load_saved(
            &SourcePath::parse("CommonModules/Tools/object.yaml").unwrap(),
            session.revision(),
            &request,
        )
        .unwrap();
    let at = u32::try_from(text.find("Цель").unwrap()).unwrap();
    let cross = session
        .module_definition(
            &caller,
            session.revision(),
            at,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert_eq!(cross.value.as_ref().unwrap(), &found);
    session
        .apply_module_overlay(
            &target,
            session.revision(),
            2,
            &body.replace("Цель", "Другая"),
            &request,
        )
        .unwrap();
    assert!(
        session
            .module_definition(
                &caller,
                session.revision(),
                at,
                &QueryCancellation::default()
            )
            .unwrap()
            .value
            .is_none()
    );
    assert!(matches!(
        session.module_definition(&target, previous, 0, &QueryCancellation::default()),
        Err(QueryError::Request(_))
    ));
    session
        .close_module_overlay(&target, session.revision(), &request)
        .unwrap();
    assert!(
        session
            .module_definition(
                &caller,
                session.revision(),
                at,
                &QueryCancellation::default()
            )
            .unwrap()
            .value
            .is_none()
    );
    session
        .apply_module_overlay(&target, session.revision(), 3, body, &request)
        .unwrap();
    assert_eq!(
        session
            .module_definition(
                &caller,
                session.revision(),
                at,
                &QueryCancellation::default()
            )
            .unwrap()
            .value
            .unwrap(),
        found
    );
}

/// Both edges of a Cyrillic call name resolve to the same declaration before punctuation.
#[test]
fn local_definition_accepts_both_identifier_boundaries() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(TARGET).unwrap();
    let body = "\u{feff}// Я😀\r\nПроцедура Цель()\r\nКонецПроцедуры\r\nЦель();\r\n";
    session
        .apply_module_overlay(&path, session.revision(), 1, body, &InputRequest::default())
        .unwrap();
    let start = u32::try_from(body.rfind("Цель").unwrap()).unwrap();
    let end = start + u32::try_from("Цель".len()).unwrap();
    let cancellation = QueryCancellation::default();
    let at_start = session
        .module_definition(&path, session.revision(), start, &cancellation)
        .unwrap();
    let at_end = session
        .module_definition(&path, session.revision(), end, &cancellation)
        .unwrap();
    assert!(at_start.value.is_some());
    assert_eq!(at_start, at_end);
}

/// Pending/bad/cancelled input and split UTF-8 offsets remain errors rather than empty navigation.
#[test]
fn definition_preconditions_preserve_input_revision_and_resources() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(TARGET).unwrap();
    let token = QueryCancellation::default();
    assert!(matches!(
        session.module_definition(&path, session.revision(), 0, &token),
        Err(QueryError::ModuleUnavailable { .. })
    ));
    let body = "Процедура Цель()\nКонецПроцедуры\nЦель();\n";
    session
        .apply_module_overlay(&path, session.revision(), 1, body, &InputRequest::default())
        .unwrap();
    let before = session.resources();
    let revision = session.revision();
    assert!(matches!(
        session.module_definition(&path, revision, 1, &token),
        Err(QueryError::InvalidOffset { .. })
    ));
    assert!(matches!(
        session.module_definition(&path, revision, u32::MAX, &token),
        Err(QueryError::InvalidOffset { .. })
    ));
    token.cancel();
    assert_eq!(
        session.module_definition(&path, revision, 0, &token),
        Err(QueryError::Cancelled)
    );
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), before);
}
