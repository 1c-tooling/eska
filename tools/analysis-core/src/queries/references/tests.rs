//! Semantic keys, not equal spellings, determine each current reference occurrence.

use super::*;
use crate::session::tests::fixture;
use eska::analysis_core::InputRequest;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "\u{feff}Процедура Пример(Аргумент)\r\nПерем Локальная;\r\nЛокальная=Аргумент;\r\nСообщить(Локальная);\r\nСтрока=\"Аргумент\";\r\nКонецПроцедуры\r\nПроцедура Другой(Аргумент)\r\nСообщить(Аргумент);\r\nКонецПроцедуры\r\nПример(1);\r\n";

/// Same-named parameters in another method and string contents are excluded by native semantics.
#[test]
fn current_references_keep_declaration_policy_lexical_scope_and_current_hashes() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &InputRequest::default())
        .unwrap();
    let control = QueryCancellation::default();
    let at = u32::try_from(BODY.find("Аргумент").unwrap()).unwrap();
    let first = session
        .module_references(&path, session.revision(), at, false, &control)
        .unwrap();
    let found = first.value.unwrap();
    assert_eq!(found.occurrences.len(), 1);
    assert_eq!(found.occurrences[0].kind, ReferenceKind::Read);
    assert_eq!(found.files_scanned, 1);
    assert!(!found.files_capped && !found.hits_capped);
    let hit = &found.occurrences[0].target;
    assert_eq!(hit.source, path);
    assert_eq!(hit.source_sha256, first.source_sha256);
    assert_eq!(hit.start_byte as usize, BODY.find("=Аргумент").unwrap() + 1);
    let both = session
        .module_references(
            &path,
            session.revision(),
            at + u32::try_from("Аргумент".len()).unwrap(),
            true,
            &control,
        )
        .unwrap();
    let occurrences = both.value.unwrap().occurrences;
    assert_eq!(occurrences.len(), 2);
    assert_eq!(occurrences[0].kind, ReferenceKind::Declaration);
    assert_eq!(occurrences[1].kind, ReferenceKind::Read);
    let previous = session.revision();
    let changed = format!("\u{feff}\r\n{}", BODY.trim_start_matches('\u{feff}'))
        .replace("Аргумент", "Параметр");
    session
        .apply_module_overlay(&path, previous, 2, &changed, &InputRequest::default())
        .unwrap();
    let at = u32::try_from(changed.find("Параметр").unwrap()).unwrap();
    let current = session
        .module_references(&path, session.revision(), at, true, &control)
        .unwrap();
    assert_ne!(current.source_sha256, first.source_sha256);
    for occurrence in current.value.unwrap().occurrences {
        let hit = occurrence.target;
        assert_eq!(hit.source_sha256, current.source_sha256);
        assert_eq!(
            &changed[hit.start_byte as usize..hit.end_byte as usize],
            "Параметр"
        );
    }
    assert!(matches!(
        session.module_references(&path, previous, at, true, &control),
        Err(QueryError::Request(_))
    ));
    assert_eq!(session.resources().metadata_source_bytes, 0);
}

/// Pending, invalid byte boundaries and cancellation do not mutate current source or revision.
#[test]
fn reference_preconditions_preserve_source_clock_and_resource_accounting() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let control = QueryCancellation::default();
    assert!(matches!(
        session.module_references(&path, session.revision(), 0, false, &control),
        Err(QueryError::ModuleUnavailable { .. })
    ));
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &InputRequest::default())
        .unwrap();
    let revision = session.revision();
    let resources = session.resources();
    for at in [1, u32::MAX] {
        assert!(matches!(
            session.module_references(&path, revision, at, true, &control),
            Err(QueryError::InvalidOffset { .. })
        ));
    }
    assert!(
        session
            .module_references(&path, revision, 0, true, &control)
            .unwrap()
            .value
            .is_none()
    );
    control.cancel();
    assert!(matches!(
        session.module_references(&path, revision, 0, true, &control),
        Err(QueryError::Cancelled)
    ));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
}

/// A resolved unused parameter differs from a missing anchor and a non-enumerable platform symbol.
#[test]
fn zero_references_and_unsupported_scope_remain_distinct_native_outcomes() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let body = "Процедура Пример(Аргумент)\nЛокальная=Новый Массив;Локальная.Вставить(1, 2);\nКонецПроцедуры\n";
    session
        .apply_module_overlay(&path, session.revision(), 1, body, &InputRequest::default())
        .unwrap();
    let control = QueryCancellation::default();
    let at = u32::try_from(body.find("Аргумент").unwrap()).unwrap();
    assert!(
        session
            .module_references(&path, session.revision(), at, false, &control)
            .unwrap()
            .value
            .unwrap()
            .occurrences
            .is_empty()
    );
    let at = u32::try_from(body.find("Вставить").unwrap()).unwrap();
    assert!(matches!(
        session.module_references(&path, session.revision(), at, false, &control),
        Err(QueryError::UnsupportedReferenceSymbol {
            category: "unknown_scope"
        })
    ));
}

/// A native occurrence remains bound to actual source bytes when its projected list is capped.
#[test]
fn reference_projection_caps_occurrences_without_claiming_a_complete_zero() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let body = "Процедура Пример(Аргумент)\nСообщить(Аргумент);\nКонецПроцедуры\n";
    session
        .apply_module_overlay(&path, session.revision(), 1, body, &InputRequest::default())
        .unwrap();
    let control = QueryCancellation::default();
    let at = u32::try_from(body.find("Аргумент").unwrap()).unwrap();
    let resources = session.resources();
    let found = session
        .query_module(&path, session.revision(), &control, |analysis, file| {
            let mut native = ide::find_references_by_name(
                analysis.database(),
                &ide::ReferencesRequest {
                    anchor: ide::ReferenceAnchor::Position {
                        file_id: file,
                        line: 0,
                        column: u32::try_from(body[..at as usize].encode_utf16().count()).unwrap(),
                    },
                    anchor_files: None,
                    area: ide::ReferenceArea::default(),
                    kinds: None,
                    include_declaration: false,
                    max_files: MAX_REFERENCE_FILES,
                },
                &[],
            );
            assert_eq!(native.hits.len(), 1);
            native.hits = vec![native.hits[0]; MAX_REFERENCE_HITS + 1];
            project(&session, analysis, native, &control)
        })
        .unwrap()
        .value
        .unwrap();
    assert_eq!(found.occurrences.len(), MAX_REFERENCE_HITS);
    assert!(found.hits_capped && !found.files_capped);
    assert_eq!(found.files_scanned, 1);
    assert_eq!(session.resources(), resources);
}

/// A positional request cannot turn dictionary-backed or ambiguous data into a confident location.
#[test]
fn incompatible_native_reference_data_and_target_ranges_are_explicit_errors() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let control = QueryCancellation::default();
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &InputRequest::default())
        .unwrap();
    session
        .query_module(&path, session.revision(), &control, |analysis, file| {
            assert!(matches!(
                current_target(&session, analysis, file, 1, 2),
                Err(QueryError::InconsistentNavigation { .. })
            ));
            let native = ide::find_references_by_name(
                analysis.database(),
                &ide::ReferencesRequest {
                    anchor: ide::ReferenceAnchor::Position {
                        file_id: file,
                        line: 0,
                        column: 17,
                    },
                    anchor_files: None,
                    area: ide::ReferenceArea::default(),
                    kinds: None,
                    include_declaration: false,
                    max_files: MAX_REFERENCE_FILES,
                },
                &[],
            );
            let mut dictionary = native.clone();
            dictionary.body_source = ide::BodySource::NameDictionary;
            let mut ambiguous = native.clone();
            ambiguous.outcome = ide::ReferencesOutcome::Ambiguous;
            let mut anchor_cap = native;
            anchor_cap.anchor_candidates_capped = true;
            for invalid in [dictionary, ambiguous, anchor_cap] {
                assert!(matches!(
                    project(&session, analysis, invalid, &control),
                    Err(QueryError::InconsistentReferences)
                ));
            }
            Ok(())
        })
        .unwrap();
}
