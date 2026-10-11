//! Cold hover preparation retains native callee ownership and the same independent current-input authority.

use std::{fs, path::PathBuf};

use super::*;
use crate::{
    HoverKind, Locale, SymbolHover,
    session::tests::{owner, support::TestDirectory},
    test_common_module_project, test_manager_project,
};

const CALLER: &str = "CommonModules/Tools/Ext/Module.bsl";

/// Ready caller text allows an exact byte cursor before either preparation or a read-only card query.
fn fixture(body: &str, manager: bool) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    let folder = if manager {
        test_manager_project::add_managers(&root, &directory.0.join("native"));
        "src-with-managers"
    } else {
        test_common_module_project::add_common_modules(&root, &directory.0.join("native"));
        "src-with-common-modules"
    };
    let source = root.join(folder);
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_module_saved(&owner(CALLER), session.revision(), &InputRequest::default())
        .unwrap();
    (directory, source, session)
}

/// Discovery prepares a separate native dot while the card retains the originally hovered token.
fn hover(session: &mut AnalysisSession, offset: u32, locale: Locale) -> Option<SymbolHover> {
    session
        .prepare_symbol_hover_metadata(
            &owner(CALLER),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    session
        .module_symbol_hover(
            &owner(CALLER),
            session.revision(),
            offset,
            locale,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
}

/// Common and manager callee cards become available cold, accepting both boundaries without reading their neighbors.
#[test]
fn cold_member_hover_prepares_only_the_current_direct_callee_at_both_name_boundaries() {
    for (manager, receiver, method, target) in [
        (
            false,
            "Utility",
            "SavedUtility",
            "CommonModules/Utility/Ext/Module.bsl",
        ),
        (
            true,
            "Catalogs.Items",
            "SavedMethod",
            "Catalogs/Items/Ext/ManagerModule.bsl",
        ),
    ] {
        let body = format!(
            "\u{feff}// Я😀\r\nProcedure Run()\r\n Я=\"😀\"; {receiver}.{method}();\r\nEndProcedure\r\n"
        );
        let (_directory, source, mut session) = fixture(&body, manager);
        let start = u32::try_from(body.rfind(method).unwrap()).unwrap();
        assert!(
            session
                .module_symbol_hover(
                    &owner(CALLER),
                    session.revision(),
                    start,
                    Locale::En,
                    &QueryCancellation::default()
                )
                .unwrap()
                .value
                .is_none()
        );
        let first = hover(&mut session, start, Locale::En).unwrap();
        assert_eq!(first.kind, HoverKind::Method);
        assert_eq!(first.symbol, method);
        assert_eq!(first.start_byte, start);
        assert_eq!(first.end_byte, start + u32::try_from(method.len()).unwrap());
        let mut paths = vec![CALLER, "CommonModules/Tools/object.yaml", target];
        if manager {
            paths.extend([
                "Catalogs/Items/object.yaml",
                "Catalogs/Items/Ext/Predefined.xml",
            ]);
        } else {
            paths.push("CommonModules/Utility/object.yaml");
        }
        let expected = paths
            .into_iter()
            .map(|p| fs::metadata(source.join(p)).unwrap().len())
            .sum::<u64>();
        assert_eq!(session.resources().total_source_bytes, expected);
        let before = session.summary();
        for (locale, unknown) in [(Locale::En, "Unknown"), (Locale::Ru, "Неизвестно")] {
            let mut expected = first.clone();
            expected.return_type = Some(unknown.into());
            assert_eq!(hover(&mut session, first.end_byte, locale), Some(expected));
            assert_eq!(session.summary(), before);
        }
        let neighbor = if manager {
            "Catalogs/Neighbor/Ext/ManagerModule.bsl"
        } else {
            "CommonModules/Neighbor/Ext/Module.bsl"
        };
        assert_eq!(
            session.module(&owner(neighbor)).unwrap().state,
            ModuleState::Pending
        );
    }
}

/// Current purpose and inferred return type replace saved cards; invalid input and close preserve authority.
#[test]
fn member_hover_follows_current_docs_types_and_saved_restoration_without_changing_caller_span() {
    let body = "\u{feff}// Я😀\r\nProcedure Run()\r\n Utility.SavedUtility();\r\nEndProcedure\r\n";
    let (_directory, _source, mut session) = fixture(body, false);
    let at = u32::try_from(body.rfind("SavedUtility").unwrap()).unwrap();
    let saved = hover(&mut session, at, Locale::En).unwrap();
    let target = owner("CommonModules/Utility/Ext/Module.bsl");
    let current = "// Current purpose Я😀\r\nFunction SavedUtility(Value = \"😀\", Next = 2) Export\r\n Return \"😀\";\r\nEndFunction\r\n";
    let request = InputRequest::default();
    session
        .apply_module_overlay(&target, session.revision(), 1, current, &request)
        .unwrap();
    for (locale, expected) in [(Locale::En, "String"), (Locale::Ru, "Строка")] {
        let card = hover(&mut session, at, locale).unwrap();
        assert_eq!(card.start_byte, saved.start_byte);
        assert_eq!(card.end_byte, saved.end_byte);
        assert_eq!(card.return_type.as_deref(), Some(expected));
        assert!(
            card.declaration
                .unwrap()
                .starts_with("Функция SavedUtility(")
        );
        assert!(card.documentation.unwrap().contains("Current purpose"));
    }
    session
        .apply_module_overlay(&target, session.revision(), 2, "???", &request)
        .unwrap();
    assert!(hover(&mut session, at, Locale::En).is_none());
    session
        .close_module_overlay(&target, session.revision(), &request)
        .unwrap();
    assert_eq!(hover(&mut session, at, Locale::En), Some(saved));
}

/// Non-callee names and locally owned receivers never cause preparation to read a foreign namespace.
#[test]
fn hover_preparation_preserves_non_callee_inputs_and_local_shadowing_before_and_after_loading() {
    for template in [
        "Procedure Run()\n $0Utility.SavedUtility();\nEndProcedure\n",
        "Procedure Run()\n Value=Utility.$0Field;\nEndProcedure\n",
        "Procedure Local(Value)\nEndProcedure\nProcedure Run()\n $0Local(Utility.SavedUtility());\nEndProcedure\n",
        "Procedure Run()\n // Utility.$0SavedUtility()\nEndProcedure\n",
    ] {
        let at = u32::try_from(template.find("$0").unwrap()).unwrap();
        let (_directory, _source, mut session) = fixture(&template.replace("$0", ""), false);
        let before = session.summary();
        hover(&mut session, at, Locale::En);
        assert_eq!(session.summary(), before);
    }
    let body = "Procedure Run()\n Utility.SavedUtility();\nEndProcedure\n";
    let shadowed = "Procedure Run(Utility)\n Utility.SavedUtility();\nEndProcedure\n";
    for loaded in [false, true] {
        let (_directory, _source, mut session) = fixture(body, false);
        if loaded {
            assert!(
                hover(
                    &mut session,
                    u32::try_from(body.find("SavedUtility").unwrap()).unwrap(),
                    Locale::En
                )
                .is_some()
            );
        }
        session
            .apply_module_overlay(
                &owner(CALLER),
                session.revision(),
                1,
                shadowed,
                &InputRequest::default(),
            )
            .unwrap();
        assert!(
            hover(
                &mut session,
                u32::try_from(shadowed.rfind("SavedUtility").unwrap()).unwrap(),
                Locale::En
            )
            .is_none()
        );
        if !loaded {
            assert_eq!(
                session
                    .inputs
                    .owner(&owner("CommonModules/Utility/object.yaml"))
                    .unwrap()
                    .state,
                InputState::Pending
            );
            assert_eq!(
                session
                    .module(&owner("CommonModules/Utility/Ext/Module.bsl"))
                    .unwrap()
                    .state,
                ModuleState::Pending
            );
        }
    }
}

/// Stale snapshots, invalid Unicode offsets and cancellation reject preparation without publishing any metadata.
#[test]
fn hover_preparation_rejected_preflight_keeps_exact_source_state() {
    let body = "\u{feff}// Я😀\r\nProcedure Run()\r\n Utility.SavedUtility();\r\nEndProcedure\r\n";
    let (_directory, _source, mut session) = fixture(body, false);
    let at = u32::try_from(body.rfind("SavedUtility").unwrap()).unwrap();
    let before = session.summary();
    let path = owner(CALLER);
    let request = InputRequest::default();
    assert!(matches!(
        session.prepare_symbol_hover_metadata(
            &path,
            SessionRevision(0),
            at,
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryError::Request(_))
    ));
    let broken = u32::try_from(body.find('😀').unwrap() + 1).unwrap();
    assert_eq!(
        session.prepare_symbol_hover_metadata(
            &path,
            session.revision(),
            broken,
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryError::InvalidOffset { offset: broken })
    );
    let cancellation = QueryCancellation::default();
    cancellation.cancel();
    assert_eq!(
        session.prepare_symbol_hover_metadata(
            &path,
            session.revision(),
            at,
            &request,
            &cancellation
        ),
        Err(QueryError::Cancelled)
    );
    assert_eq!(session.summary(), before);
}
