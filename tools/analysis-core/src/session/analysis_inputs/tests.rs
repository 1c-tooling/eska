//! Shared preparation preserves current authority and performs no implicit saved refresh.

use super::*;
use crate::session::tests::{owner, support::TestDirectory};

/// Fully supported inputs isolate authority and cancellation from intentional binary fixtures.
fn fixture() -> (TestDirectory, std::path::PathBuf) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Saved body.\n", false);
    (directory, root.join("src"))
}

/// Preparation skips ready snapshots even after their saved files disappear, and guards all reads.
#[test]
fn ready_inputs_are_not_refreshed_and_stale_preparation_cannot_read() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let initial = session.revision();
    let path = owner("CommonModules/Tools/Ext/Module.bsl");
    let request = InputRequest::default();
    session
        .apply_module_overlay(&path, initial, 7, "// Current editor.\n", &request)
        .unwrap();
    let before = session.summary();
    assert!(matches!(
        session.load_semantic_inputs(initial, &request),
        Err(SessionRequestError::StaleRevision { .. })
    ));
    assert_eq!(session.summary(), before);
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(session.summary().inputs.metadata_input_complete);
    assert!(session.module_summary().module_input_complete);
    let revision = session.revision();
    let resources = session.resources();
    let inputs = session.module(&path).unwrap().clone();
    for path in session.owner_paths().chain(session.module_paths()) {
        std::fs::remove_file(root.join(path.as_path())).unwrap();
    }
    session.load_semantic_inputs(revision, &request).unwrap();
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
    assert_eq!(session.module(&path), Some(&inputs));
    assert_eq!(
        inputs.origin,
        InputOrigin::Overlay {
            document_version: 7
        }
    );
}

/// Cancelled saved reads retry; failed overlays remain the current unavailable state.
#[test]
fn preparation_retries_saved_cancellation_without_replacing_editor_failures() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let metadata = owner("CommonModules/Tools/object.yaml");
    let module = owner("CommonModules/Tools/Ext/Module.bsl");
    let request = InputRequest::default();
    let cancelled = InputRequest::default();
    cancelled.cancellation.cancel();
    session
        .load_saved(&metadata, session.revision(), &cancelled)
        .unwrap();
    session
        .load_module_saved(&module, session.revision(), &cancelled)
        .unwrap();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(session.summary().inputs.metadata_input_complete);
    assert!(session.module_summary().module_input_complete);
    session
        .apply_overlay(
            &metadata,
            session.revision(),
            3,
            "unavailable editor",
            &cancelled,
        )
        .unwrap();
    session
        .apply_module_overlay(
            &module,
            session.revision(),
            4,
            "unavailable editor",
            &cancelled,
        )
        .unwrap();
    let revision = session.revision();
    session.load_semantic_inputs(revision, &request).unwrap();
    assert_eq!(session.revision(), revision);
    assert_eq!(
        session.owner(&metadata).unwrap().origin,
        InputOrigin::Overlay {
            document_version: 3
        }
    );
    assert!(matches!(
        session.owner(&metadata).unwrap().state,
        ModelState::Unavailable(InputFailure::Cancelled)
    ));
    assert_eq!(
        session.module(&module).unwrap().origin,
        InputOrigin::Overlay {
            document_version: 4
        }
    );
    assert!(matches!(
        session.module(&module).unwrap().state,
        ModuleState::Unavailable(ModuleFailure::Input(InputFailure::Cancelled))
    ));
}

/// An already stopped preparation keeps all unvisited declarations pending without source allocation.
#[test]
fn cancelled_preparation_keeps_pending_inputs_and_revision() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let before = session.summary();
    let request = InputRequest::default();
    request.cancellation.cancel();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert_eq!(session.summary(), before);
    assert_eq!(session.resources().total_source_bytes, 0);
    assert!(
        session
            .selected_root
            .modules
            .values()
            .all(|entry| entry.model.state == ModuleState::Pending)
    );
}
