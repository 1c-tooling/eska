//! Direct namespaces require current flags before their declared body may contribute exports.

use std::{collections::BTreeSet, fs, path::PathBuf};

use super::*;
use crate::{
    Locale,
    session::tests::{owner, support::TestDirectory},
    test_common_module_project as project,
};

const CALLER: &str = "CommonModules/Tools/Ext/Module.bsl";
const OWN: &str = "CommonModules/Tools/object.yaml";
const TARGET: &str = "CommonModules/Utility/object.yaml";
const MODULE: &str = "CommonModules/Utility/Ext/Module.bsl";
const BODY: &str = "\u{feff}// Я😀\r\nProcedure Run()\r\n Value = Utility.;\r\nEndProcedure\r\n";

/// Retain directory ownership while current YAML, BSL and source-reserve assertions run.
fn fixture(body: &str) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    project::add_common_modules(&root, &directory.0.join("native"));
    let source = root.join("src-with-common-modules");
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_module_saved(&owner(CALLER), session.revision(), &InputRequest::default())
        .unwrap();
    (directory, source, session)
}

/// Prepare only the dependency at the cursor, then run the distinct native read-only completion query.
fn complete(session: &mut AnalysisSession, body: &str, locale: Locale) -> BTreeSet<String> {
    complete_at(session, CALLER, body, locale)
}

/// Completion uses the declared physical source path independently of native module conventions.
fn complete_at(
    session: &mut AnalysisSession,
    caller: &str,
    body: &str,
    locale: Locale,
) -> BTreeSet<String> {
    let offset = u32::try_from(body.find(".;").unwrap() + 1).unwrap();
    session
        .prepare_completion_metadata(
            &owner(caller),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    session
        .module_completions(
            &owner(caller),
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

/// One named namespace loads two owners and two bodies, excludes private exports and never scans its neighbor.
#[test]
fn cold_common_module_completion_loads_only_its_current_declared_owner_and_body() {
    for body in [BODY.to_owned(), BODY.replace("Utility.", "uTiLiTy.")] {
        let (_directory, source, mut session) = fixture(&body);
        for locale in [Locale::En, Locale::Ru] {
            let labels = complete(&mut session, &body, locale);
            assert!(labels.contains("SavedUtility"));
            assert!(!labels.contains("PrivateUtility"));
            assert!(!labels.contains("NeighborUtility"));
        }
        for path in session.selected_root.inputs.owner_paths() {
            assert_eq!(
                matches!(
                    session.selected_root.inputs.owner(path).unwrap().state,
                    InputState::Ready(_)
                ),
                matches!(path.as_str(), OWN | TARGET),
                "{path:?}"
            );
        }
        for path in session.module_paths() {
            assert_eq!(
                matches!(
                    session.module(path).unwrap().state,
                    ModuleState::Ready { .. }
                ),
                matches!(path.as_str(), CALLER | MODULE),
                "{path:?}"
            );
        }
        let bytes = [OWN, TARGET, CALLER, MODULE]
            .into_iter()
            .map(|path| fs::metadata(source.join(path)).unwrap().len())
            .sum::<u64>();
        assert_eq!(session.resources().total_source_bytes, bytes);
        let before = session.summary();
        complete(&mut session, &body, Locale::En);
        assert_eq!(session.summary(), before);
    }
}

/// Current YAML gates body preparation and keeps controlling exports even after that body is already ready.
#[test]
fn current_common_module_flags_gate_loading_and_remove_inaccessible_or_invalid_exports() {
    let (_directory, source, mut session) = fixture(BODY);
    let saved = fs::read_to_string(source.join(TARGET)).unwrap();
    let denied = crate::test_property::boolean_value(
        &saved,
        eska::project_core::source_format::PropertyName::ServerCall,
        false,
    );
    assert_ne!(saved, denied);
    let request = InputRequest::default();
    session
        .apply_overlay(&owner(TARGET), session.revision(), 1, &denied, &request)
        .unwrap();
    assert!(!complete(&mut session, BODY, Locale::En).contains("SavedUtility"));
    assert_eq!(
        session.module(&owner(MODULE)).unwrap().state,
        ModuleState::Pending
    );
    session
        .apply_overlay(&owner(TARGET), session.revision(), 2, &saved, &request)
        .unwrap();
    assert!(complete(&mut session, BODY, Locale::En).contains("SavedUtility"));
    session
        .apply_module_overlay(
            &owner(MODULE),
            session.revision(),
            1,
            project::CURRENT,
            &request,
        )
        .unwrap();
    assert!(complete(&mut session, BODY, Locale::En).contains("CurrentUtility"));
    for (version, text) in [(3, denied.as_str()), (4, "???")] {
        session
            .apply_overlay(&owner(TARGET), session.revision(), version, text, &request)
            .unwrap();
        let before = session.module(&owner(MODULE)).unwrap().clone();
        let labels = complete(&mut session, BODY, Locale::En);
        assert!(!labels.contains("SavedUtility"));
        assert!(!labels.contains("CurrentUtility"));
        assert_eq!(session.module(&owner(MODULE)).unwrap(), &before);
    }
    session
        .close_overlay(&owner(TARGET), session.revision(), &request)
        .unwrap();
    assert!(complete(&mut session, BODY, Locale::Ru).contains("CurrentUtility"));
    session
        .close_module_overlay(&owner(MODULE), session.revision(), &request)
        .unwrap();
    assert!(complete(&mut session, BODY, Locale::Ru).contains("SavedUtility"));
}

/// Shadowing rejects the namespace before YAML loading, and continues rejecting exports from an already loaded body.
#[test]
fn common_module_preparation_preserves_local_name_ownership_before_and_after_loading() {
    let (_directory, _source, mut session) = fixture(BODY);
    assert!(complete(&mut session, BODY, Locale::En).contains("SavedUtility"));
    let shadow = "Procedure Run(Utility)\n Value = Utility.;\nEndProcedure\n";
    session
        .apply_module_overlay(
            &owner(CALLER),
            session.revision(),
            1,
            shadow,
            &InputRequest::default(),
        )
        .unwrap();
    let before = session.summary();
    assert!(!complete(&mut session, shadow, Locale::En).contains("SavedUtility"));
    assert_eq!(session.summary(), before);
    let (_directory, _source, mut session) = fixture(shadow);
    assert!(!complete(&mut session, shadow, Locale::En).contains("SavedUtility"));
    assert_eq!(
        session
            .selected_root
            .inputs
            .owner(&owner(TARGET))
            .unwrap()
            .state,
        InputState::Pending
    );
    assert_eq!(
        session.module(&owner(MODULE)).unwrap().state,
        ModuleState::Pending
    );
}

/// A saved body failure is cached until watched invalidation; binary and absent bodies never gain text fallback.
#[test]
fn common_module_preparation_keeps_missing_binary_and_bodyless_dependencies_explicit() {
    let (_directory, source, mut session) = fixture(BODY);
    fs::remove_file(source.join(MODULE)).unwrap();
    assert!(!complete(&mut session, BODY, Locale::En).contains("SavedUtility"));
    assert!(matches!(
        session.module(&owner(MODULE)).unwrap().state,
        ModuleState::Unavailable(_)
    ));
    let before = session.summary();
    fs::write(source.join(MODULE), project::SAVED).unwrap();
    complete(&mut session, BODY, Locale::En);
    assert_eq!(session.summary(), before);
    session
        .invalidate_saved_inputs(&[owner(MODULE)], session.revision())
        .unwrap();
    assert!(complete(&mut session, BODY, Locale::En).contains("SavedUtility"));
    for target in ["Binary", "Bodyless"] {
        let body = BODY.replace("Utility.", &format!("{target}."));
        let (_directory, _source, mut session) = fixture(&body);
        assert!(!complete(&mut session, &body, Locale::En).contains("SavedUtility"));
        assert_eq!(
            session
                .module(&owner("CommonModules/Binary/Ext/Module.bin"))
                .unwrap()
                .state,
            ModuleState::Binary
        );
        assert_eq!(
            session.module(&owner(MODULE)).unwrap().state,
            ModuleState::Pending
        );
    }
}

/// Relocated common bodies retain kernel scope, exported completions and physical navigation targets.
#[test]
fn relocated_common_bodies_complete_and_navigate_to_current_source_paths() {
    migrated_common_bodies_complete_and_navigate(false);
}

/// Named property envelopes feed the same lazy completion and native resolver as legacy descriptors.
#[test]
fn readable_common_properties_complete_and_navigate_to_current_source_paths() {
    migrated_common_bodies_complete_and_navigate(true);
}

/// Both explicit migrations retain the same module identity, scopes, overlays and source paths.
fn migrated_common_bodies_complete_and_navigate(readable: bool) {
    let (directory, legacy, _) = fixture(BODY);
    let source = directory.0.join("relocated-common");
    let plan = if readable {
        eska::project_core::source_format::prepare_readable_layout(&legacy)
    } else {
        eska::project_core::source_format::prepare_resource_layout(&legacy)
    }
    .unwrap();
    plan.publish(&source).unwrap();
    let mut session = AnalysisSession::open(&source).unwrap();
    let caller = "CommonModules/Tools/Module.bsl";
    let target = owner("CommonModules/Utility/Module.bsl");
    let request = InputRequest::default();
    session
        .load_module_saved(&owner(caller), session.revision(), &request)
        .unwrap();
    for locale in [Locale::En, Locale::Ru] {
        assert!(complete_at(&mut session, caller, BODY, locale).contains("SavedUtility"));
    }
    if readable {
        let path = source.join(TARGET);
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("\"ServerCall\": true"));
        let denied = saved.replace("\"ServerCall\": true", "\"ServerCall\": false");
        session
            .apply_overlay(&owner(TARGET), session.revision(), 1, &denied, &request)
            .unwrap();
        assert!(!complete_at(&mut session, caller, BODY, Locale::En).contains("SavedUtility"));
        session
            .apply_overlay(&owner(TARGET), session.revision(), 2, &saved, &request)
            .unwrap();
        assert!(complete_at(&mut session, caller, BODY, Locale::Ru).contains("SavedUtility"));
    }
    session
        .apply_module_overlay(&target, session.revision(), 1, project::CURRENT, &request)
        .unwrap();
    assert!(complete_at(&mut session, caller, BODY, Locale::En).contains("CurrentUtility"));
    let body = "Procedure Run()\nUtility.CurrentUtility();\nEndProcedure\n";
    session
        .apply_module_overlay(&owner(caller), session.revision(), 1, body, &request)
        .unwrap();
    let offset = u32::try_from(body.find("CurrentUtility").unwrap() + 1).unwrap();
    let result = session
        .module_definition(
            &owner(caller),
            session.revision(),
            offset,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .unwrap();
    assert_eq!(result.source, target);
    assert!(
        session
            .module_paths()
            .all(|path| !path.as_str().contains("/Ext/"))
    );
    assert!(
        session
            .module(&owner("CommonModules/Neighbor/Module.bsl"))
            .is_some_and(|input| input.state == ModuleState::Pending)
    );
}
