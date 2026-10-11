//! Direct completion prepares only its native manager body and preserves independent input authority.

use std::{collections::BTreeSet, fs, path::PathBuf};

use eska::analysis_core::Cancellation;

use super::*;
use crate::{
    Locale, MetadataResourceState, SessionOptions,
    session::tests::{owner, support::TestDirectory},
    test_manager_project,
};

const CALLER: &str = "CommonModules/Tools/Ext/Module.bsl";
const MANAGER: &str = "Catalogs/Items/Ext/ManagerModule.bsl";
const BODY: &str =
    "\u{feff}// Я😀\r\nProcedure Run()\r\n Value = Catalogs.Items.;\r\nEndProcedure\r\n";

/// Keep all bodies and resources exclusively inside a unique playground directory.
fn fixture(body: &str) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    test_manager_project::add_managers(&root, &directory.0.join("native"));
    let source = root.join("src-with-managers");
    let session = AnalysisSession::open(&source).unwrap();
    (directory, source, session)
}

/// Compare the actual native export consumer rather than merely observing module readiness.
fn labels(session: &AnalysisSession, body: &str, locale: Locale) -> BTreeSet<String> {
    let offset = u32::try_from(body.find(".;").unwrap() + 1).unwrap();
    session
        .module_completions(
            &owner(CALLER),
            session.revision(),
            offset,
            locale,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .into_iter()
        .map(|item| item.label)
        .collect()
}

/// The public preparation and read-only native query remain distinct calls over one current revision.
fn prepare(session: &mut AnalysisSession, body: &str) {
    let offset = u32::try_from(body.find(".;").unwrap() + 1).unwrap();
    session
        .prepare_completion_metadata(
            &owner(CALLER),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
}

/// A cold direct access loads one saved manager, excludes private methods and leaves neighboring bodies pending.
#[test]
fn cold_direct_manager_completion_loads_only_declared_current_exports() {
    for body in [
        BODY.to_owned(),
        BODY.replace("Catalogs.Items", "сПрАвОчНиКи.iTeMs"),
    ] {
        let (_directory, source, mut session) = fixture(&body);
        session
            .load_module_saved(&owner(CALLER), session.revision(), &InputRequest::default())
            .unwrap();
        assert!(!labels(&session, &body, Locale::En).contains("SavedMethod"));
        prepare(&mut session, &body);
        assert!(matches!(
            session.module(&owner(MANAGER)).unwrap().state,
            ModuleState::Ready { .. }
        ));
        for locale in [Locale::En, Locale::Ru] {
            let labels = labels(&session, &body, locale);
            assert!(labels.contains("SavedMethod"));
            assert!(!labels.contains("HiddenMethod"));
            assert!(labels.contains("Saved"));
        }
        for path in session.module_paths() {
            if path.as_str() != CALLER && path.as_str() != MANAGER {
                assert!(
                    matches!(
                        session.module(path).unwrap().state,
                        ModuleState::Pending | ModuleState::Binary
                    ),
                    "{path:?}"
                );
            }
        }
        assert_eq!(
            session
                .metadata_resource(&owner("Catalogs/Items/Forms/Shared/Ext/Form.xml"))
                .unwrap()
                .state,
            MetadataResourceState::Pending
        );
        let expected = [
            CALLER,
            MANAGER,
            "CommonModules/Tools/object.yaml",
            "Catalogs/Items/object.yaml",
            "Catalogs/Items/Ext/Predefined.xml",
        ]
        .into_iter()
        .map(|path| fs::metadata(source.join(path)).unwrap().len())
        .sum::<u64>();
        assert_eq!(session.resources().total_source_bytes, expected);
        let before = session.summary();
        prepare(&mut session, &body);
        assert_eq!(session.summary(), before);
    }
}

/// Editor text replaces exports, malformed current BSL loses old symbols, and close loads actual saved bytes.
#[test]
fn manager_exports_follow_current_overlay_invalid_body_watch_and_close() {
    let (_directory, source, mut session) = fixture(BODY);
    let caller = owner(CALLER);
    let manager = owner(MANAGER);
    let request = InputRequest::default();
    session
        .load_module_saved(&caller, session.revision(), &request)
        .unwrap();
    prepare(&mut session, BODY);
    for (version, text, current) in [(1, test_manager_project::CURRENT, true), (2, "???", false)] {
        session
            .apply_module_overlay(&manager, session.revision(), version, text, &request)
            .unwrap();
        let input = session.module(&manager).unwrap().clone();
        let before = session.summary();
        session
            .invalidate_saved_inputs(std::slice::from_ref(&manager), session.revision())
            .unwrap();
        prepare(&mut session, BODY);
        assert_eq!(session.summary(), before);
        assert_eq!(session.module(&manager).unwrap(), &input);
        let labels = labels(&session, BODY, Locale::En);
        assert_eq!(labels.contains("CurrentMethod"), current);
        assert!(!labels.contains("SavedMethod"));
        assert!(labels.contains("Saved"));
    }
    fs::write(source.join(MANAGER), test_manager_project::CURRENT).unwrap();
    session
        .close_module_overlay(&manager, session.revision(), &request)
        .unwrap();
    assert!(labels(&session, BODY, Locale::En).contains("CurrentMethod"));
    fs::remove_file(source.join(MANAGER)).unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&manager), session.revision())
        .unwrap();
    prepare(&mut session, BODY);
    assert!(matches!(
        session.module(&manager).unwrap().state,
        ModuleState::Unavailable(ModuleFailure::Input(InputFailure::Source(_)))
    ));
    assert!(!labels(&session, BODY, Locale::En).contains("CurrentMethod"));
    let before = session.summary();
    fs::write(source.join(MANAGER), test_manager_project::SAVED).unwrap();
    prepare(&mut session, BODY);
    assert_eq!(session.summary(), before);
    session
        .invalidate_saved_inputs(std::slice::from_ref(&manager), session.revision())
        .unwrap();
    prepare(&mut session, BODY);
    assert!(labels(&session, BODY, Locale::En).contains("SavedMethod"));
}

/// Saved cancellation can retry, but cancelled editor text and source-budget failures cannot be refreshed implicitly.
#[test]
fn manager_preparation_preserves_cancelled_overlay_and_quota_authority() {
    let (_directory, source, mut session) = fixture(BODY);
    let caller = owner(CALLER);
    let manager = owner(MANAGER);
    let request = InputRequest::default();
    session
        .load_module_saved(&caller, session.revision(), &request)
        .unwrap();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let stopped = InputRequest {
        cancellation: cancelled,
        ..InputRequest::default()
    };
    session
        .load_module_saved(&manager, session.revision(), &stopped)
        .unwrap();
    prepare(&mut session, BODY);
    assert!(labels(&session, BODY, Locale::En).contains("SavedMethod"));
    session
        .apply_module_overlay(
            &manager,
            session.revision(),
            1,
            test_manager_project::CURRENT,
            &stopped,
        )
        .unwrap();
    let before = session.summary();
    prepare(&mut session, BODY);
    assert_eq!(session.summary(), before);
    assert_eq!(
        session.module(&manager).unwrap().state,
        ModuleState::Unavailable(ModuleFailure::Input(InputFailure::Cancelled))
    );
    assert!(!labels(&session, BODY, Locale::En).contains("SavedMethod"));
    session
        .close_module_overlay(&manager, session.revision(), &request)
        .unwrap();
    assert!(labels(&session, BODY, Locale::En).contains("SavedMethod"));
    let mut session = AnalysisSession::open_with_options(
        &source,
        SessionOptions {
            max_loaded_module_bytes: u64::try_from(BODY.len()).unwrap(),
            ..SessionOptions::default()
        },
    )
    .unwrap();
    session
        .load_module_saved(&caller, session.revision(), &request)
        .unwrap();
    prepare(&mut session, BODY);
    assert!(matches!(
        session.module(&manager).unwrap().state,
        ModuleState::Unavailable(ModuleFailure::LoadedTextBudgetExceeded { .. })
    ));
    let before = session.summary();
    prepare(&mut session, BODY);
    assert_eq!(session.summary(), before);
}

/// Native context, shadowing, metadata collections and a declared binary manager do not trigger text fallback.
#[test]
fn manager_preparation_rejects_shadowed_non_object_and_binary_targets() {
    for body in [
        "Procedure Run(Catalogs)\n Value = Catalogs.Items.;\nEndProcedure\n",
        "Procedure Run()\n Value = Catalogs.;\nEndProcedure\n",
        "Procedure Run()\n Value = Metadata.Catalogs.;\nEndProcedure\n",
        "Procedure Run()\n Value = Catalogs.Absent.;\nEndProcedure\n",
    ] {
        let (_directory, _source, mut session) = fixture(body);
        session
            .load_module_saved(&owner(CALLER), session.revision(), &InputRequest::default())
            .unwrap();
        prepare(&mut session, body);
        assert_eq!(
            session.module(&owner(MANAGER)).unwrap().state,
            ModuleState::Pending
        );
        assert_eq!(
            session
                .module(&owner("Catalogs/Absent/Ext/ManagerModule.bin"))
                .unwrap()
                .state,
            ModuleState::Binary
        );
    }
}
