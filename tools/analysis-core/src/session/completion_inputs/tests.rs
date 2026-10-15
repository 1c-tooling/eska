//! Cold completion prepares exact current dependencies without widening source coverage.

use std::{collections::BTreeSet, fs, path::PathBuf};

use eska::analysis_core::{Cancellation, InputState};

use super::*;
use crate::{
    Locale, MetadataResourceState, ModelState,
    session::tests::{owner, support::TestDirectory},
    test_predefined_project,
};

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const OWN: &str = "CommonModules/Tools/object.yaml";
const TARGET: &str = "Catalogs/Items/object.yaml";
const RESOURCE: &str = "Catalogs/Items/Ext/Predefined.xml";
const BODY: &str =
    "\u{feff}// Я😀\r\nProcedure Run()\r\n Value = Catalogs.Items.Saved;\r\nEndProcedure\r\n";

/// Import one owned fixture and retain its guard through all source-state assertions.
fn fixture(body: &str) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    test_predefined_project::add_predefined(&root, &directory.0.join("native"));
    let source = root.join("src-with-predefined");
    let session = AnalysisSession::open(&source).unwrap();
    (directory, source, session)
}

/// Query the shared read-only API after the caller explicitly prepared its current dependencies.
fn labels(session: &AnalysisSession, offset: u32, locale: Locale) -> BTreeSet<String> {
    session
        .module_completions(
            &owner(MODULE),
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

/// Observe controller readiness, not projection readiness: a pending resource can hide ready YAML.
fn input_ready(session: &AnalysisSession, path: &str) -> bool {
    matches!(
        session
            .selected_root
            .inputs
            .owner(&owner(path))
            .unwrap()
            .state,
        InputState::Ready(_)
    )
}

/// A first direct object request needs its own YAML and the target's YAML/resource only.
#[test]
fn cold_direct_completion_loads_only_own_and_target_inputs_in_both_locales() {
    for access in ["Catalogs.Items.", "Справочники.Items.", "cAtAlOgS.iTeMs."] {
        let body = BODY.replace("Catalogs.Items.", access);
        let (_directory, source, mut session) = fixture(&body);
        let module = owner(MODULE);
        let offset = u32::try_from(body.find("Saved").unwrap()).unwrap();
        let request = InputRequest::default();
        session
            .load_module_saved(&module, session.revision(), &request)
            .unwrap();
        let before = session.resources();
        assert!(!labels(&session, offset, Locale::En).contains("Saved"));
        assert_eq!(session.resources(), before);
        session
            .prepare_completion_metadata(
                &module,
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        for locale in [Locale::En, Locale::Ru] {
            assert!(labels(&session, offset, locale).contains("Saved"));
        }
        assert!(input_ready(&session, OWN));
        assert!(input_ready(&session, TARGET));
        assert!(matches!(
            session.owner(&owner(TARGET)).unwrap().state,
            ModelState::Ready(_)
        ));
        for path in session.selected_root.inputs.owner_paths() {
            if path.as_str() != OWN && path.as_str() != TARGET {
                assert!(
                    matches!(
                        session.selected_root.inputs.owner(path).unwrap().state,
                        InputState::Pending
                    ),
                    "{path:?}"
                );
            }
        }
        for path in session.resource_paths() {
            let input = session.metadata_resource(path).unwrap();
            assert_eq!(
                matches!(input.state, MetadataResourceState::Ready { .. }),
                path.as_str() == RESOURCE,
                "{path:?}"
            );
        }
        // The same target owns a declared nested dialog; direct manager access must leave it pending.
        assert_eq!(
            session
                .metadata_resource(&owner("Catalogs/Items/Forms/Shared/Ext/Form.xml"))
                .unwrap()
                .state,
            MetadataResourceState::Pending
        );
        let expected = [MODULE, OWN, TARGET, RESOURCE]
            .into_iter()
            .map(|path| fs::metadata(source.join(path)).unwrap().len())
            .sum::<u64>();
        assert_eq!(session.resources().total_source_bytes, expected);
        let summary = session.summary();
        session
            .prepare_completion_metadata(
                &module,
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        assert_eq!(session.summary(), summary);
    }
}

/// Native syntax and root ownership reject false direct dependencies before any target I/O.
#[test]
fn collection_metadata_string_comment_and_shadowed_roots_do_not_load_object_payloads() {
    for body in [
        "Procedure Run()\n Value = Catalogs.$0;\nEndProcedure\n",
        "Procedure Run()\n Value = Metadata.Catalogs.$0;\nEndProcedure\n",
        "Procedure Run(Catalogs)\n Value = Catalogs.Items.$0;\nEndProcedure\n",
        "Procedure Run()\n Value = \"Catalogs.Items.$0\";\nEndProcedure\n",
        "Procedure Run()\n // Catalogs.Items.$0\nEndProcedure\n",
    ] {
        let offset = u32::try_from(body.find("$0").unwrap()).unwrap();
        let body = body.replace("$0", "");
        let (_directory, _source, mut session) = fixture(&body);
        let module = owner(MODULE);
        let request = InputRequest::default();
        session
            .load_module_saved(&module, session.revision(), &request)
            .unwrap();
        session
            .prepare_completion_metadata(
                &module,
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        assert!(input_ready(&session, OWN), "{body}");
        assert!(!input_ready(&session, TARGET), "{body}");
        assert!(
            session
                .resource_paths()
                .all(|path| session.metadata_resource(path).unwrap().state
                    == MetadataResourceState::Pending),
            "{body}"
        );
    }
}

/// Rejected clocks, byte boundaries and cancellation cannot publish additional metadata input.
#[test]
fn preparation_preflight_does_not_mutate_on_stale_invalid_or_cancelled_requests() {
    let (_directory, _source, mut session) = fixture(BODY);
    let module = owner(MODULE);
    let stale = session.revision();
    let request = InputRequest::default();
    session.load_module_saved(&module, stale, &request).unwrap();
    let offset = u32::try_from(BODY.find("Saved").unwrap()).unwrap();
    let before = session.summary();
    assert!(matches!(
        session.prepare_completion_metadata(
            &module,
            stale,
            offset,
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryError::Request(
            SessionRequestError::StaleRevision { .. }
        ))
    ));
    assert_eq!(
        session.prepare_completion_metadata(
            &module,
            session.revision(),
            1,
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryError::InvalidOffset { offset: 1 })
    );
    let cancellation = QueryCancellation::default();
    cancellation.cancel();
    assert_eq!(
        session.prepare_completion_metadata(
            &module,
            session.revision(),
            offset,
            &request,
            &cancellation
        ),
        Err(QueryError::Cancelled)
    );
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let request = InputRequest {
        cancellation,
        ..InputRequest::default()
    };
    assert_eq!(
        session.prepare_completion_metadata(
            &module,
            session.revision(),
            offset,
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryError::Cancelled)
    );
    assert_eq!(session.summary(), before);
    assert!(session.resource_paths().all(
        |path| session.metadata_resource(path).unwrap().state == MetadataResourceState::Pending
    ));
}

/// Pending projection does not authorize rereading ready YAML or replacing a current resource overlay.
#[test]
fn targeted_preparation_preserves_current_overlay_and_cached_yaml_when_a_resource_changes() {
    let (_directory, source, mut session) = fixture(BODY);
    let module = owner(MODULE);
    let resource = owner(RESOURCE);
    let target = owner(TARGET);
    let request = InputRequest::default();
    let cancellation = QueryCancellation::default();
    let offset = u32::try_from(BODY.find("Saved").unwrap()).unwrap();
    let saved_yaml = fs::read_to_string(source.join(TARGET)).unwrap();
    session
        .load_module_saved(&module, session.revision(), &request)
        .unwrap();
    session
        .prepare_completion_metadata(&module, session.revision(), offset, &request, &cancellation)
        .unwrap();
    let yaml_hash = session
        .selected_root
        .inputs
        .owner(&target)
        .unwrap()
        .source_sha256
        .clone();
    for (version, text, has_current) in [
        (1, test_predefined_project::data("Current"), true),
        (2, "<NotPredefinedData/>".to_owned(), false),
    ] {
        session
            .apply_resource_overlay(&resource, session.revision(), version, &text, &request)
            .unwrap();
        let before = session.summary();
        session
            .prepare_completion_metadata(
                &module,
                session.revision(),
                offset,
                &request,
                &cancellation,
            )
            .unwrap();
        assert_eq!(session.summary(), before);
        let items = labels(&session, offset, Locale::En);
        assert_eq!(items.contains("Current"), has_current);
        assert!(!items.contains("Saved"));
        assert!(matches!(
            session.metadata_resource(&resource).unwrap().origin,
            InputOrigin::Overlay { .. }
        ));
        assert_eq!(
            session
                .selected_root
                .inputs
                .owner(&target)
                .unwrap()
                .source_sha256,
            yaml_hash
        );
    }
    session
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    assert!(labels(&session, offset, Locale::En).contains("Saved"));
    // No watcher invalidated this YAML. A pending resource must keep that ready saved snapshot.
    fs::remove_file(source.join(TARGET)).unwrap();
    fs::write(
        source.join(RESOURCE),
        test_predefined_project::data("DiskCurrent"),
    )
    .unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&resource), session.revision())
        .unwrap();
    assert_eq!(session.owner(&target).unwrap().state, ModelState::Pending);
    session
        .prepare_completion_metadata(&module, session.revision(), offset, &request, &cancellation)
        .unwrap();
    assert!(input_ready(&session, TARGET));
    assert_eq!(
        session
            .selected_root
            .inputs
            .owner(&target)
            .unwrap()
            .source_sha256,
        yaml_hash
    );
    assert!(labels(&session, offset, Locale::En).contains("DiskCurrent"));
    session
        .apply_overlay(&target, session.revision(), 1, "broken: [", &request)
        .unwrap();
    let before = session.summary();
    session
        .prepare_completion_metadata(&module, session.revision(), offset, &request, &cancellation)
        .unwrap();
    assert_eq!(session.summary(), before);
    assert!(!labels(&session, offset, Locale::En).contains("DiskCurrent"));
    fs::write(source.join(TARGET), saved_yaml).unwrap();
    session
        .close_overlay(&target, session.revision(), &request)
        .unwrap();
    assert!(labels(&session, offset, Locale::En).contains("DiskCurrent"));
}

/// The same budget and saved-failure policy govern explicit metadata reads and cold completion.
#[test]
fn targeted_preparation_preserves_quota_failures_until_explicit_invalidation() {
    let (_directory, source, _unused_session) = fixture(BODY);
    let yaml = fs::read_to_string(source.join(TARGET)).unwrap();
    fs::write(
        source.join(TARGET),
        format!("# {}\n{yaml}", "reservation".repeat(400)),
    )
    .unwrap();
    let limit = [MODULE, OWN, TARGET]
        .into_iter()
        .map(|path| fs::metadata(source.join(path)).unwrap().len())
        .sum::<u64>();
    let mut session = AnalysisSession::open_with_options(
        &source,
        crate::SessionOptions {
            max_retained_source_bytes: limit,
            ..crate::SessionOptions::default()
        },
    )
    .unwrap();
    let module = owner(MODULE);
    let resource = owner(RESOURCE);
    let request = InputRequest::default();
    let cancellation = QueryCancellation::default();
    let offset = u32::try_from(BODY.find("Saved").unwrap()).unwrap();
    session
        .load_module_saved(&module, session.revision(), &request)
        .unwrap();
    session
        .prepare_completion_metadata(&module, session.revision(), offset, &request, &cancellation)
        .unwrap();
    assert_eq!(session.resources().total_source_bytes, limit);
    assert_eq!(
        session.metadata_resource(&resource).unwrap().state,
        MetadataResourceState::Unavailable(InputFailure::SourceBudgetExceeded { limit })
    );
    assert!(!labels(&session, offset, Locale::En).contains("Saved"));
    let before = session.summary();
    session
        .prepare_completion_metadata(&module, session.revision(), offset, &request, &cancellation)
        .unwrap();
    assert_eq!(session.summary(), before);
    session
        .apply_overlay(&owner(TARGET), session.revision(), 1, &yaml, &request)
        .unwrap();
    assert!(session.resources().total_source_bytes < limit);
    // Releasing space does not refresh a failed resource implicitly; its own event is required.
    session
        .prepare_completion_metadata(&module, session.revision(), offset, &request, &cancellation)
        .unwrap();
    assert_eq!(
        session.metadata_resource(&resource).unwrap().state,
        MetadataResourceState::Unavailable(InputFailure::SourceBudgetExceeded { limit })
    );
    session
        .invalidate_saved_inputs(std::slice::from_ref(&resource), session.revision())
        .unwrap();
    session
        .prepare_completion_metadata(&module, session.revision(), offset, &request, &cancellation)
        .unwrap();
    assert!(labels(&session, offset, Locale::En).contains("Saved"));
}

/// A current form body prepares its own dialog while native context decides a different manager target.
#[test]
fn form_completion_prepares_own_dialog_and_respects_native_client_server_context() {
    for (directive, admitted) in [("AtClient", false), ("AtServer", true)] {
        let (_directory, source, mut session) = fixture(BODY);
        let form_module = owner("Catalogs/Items/Forms/Shared/Ext/Form/Module.bsl");
        let body = format!(
            "&{directive}\nProcedure Run()\n Value = Catalogs.Neighbor.NeighborItem;\nEndProcedure\n"
        );
        let offset = u32::try_from(body.find("NeighborItem").unwrap()).unwrap();
        fs::write(source.join(form_module.as_path()), body).unwrap();
        let request = InputRequest::default();
        session
            .load_module_saved(&form_module, session.revision(), &request)
            .unwrap();
        session
            .prepare_completion_metadata(
                &form_module,
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        let input = session
            .metadata_resource(&owner("Catalogs/Items/Forms/Shared/Ext/Form.xml"))
            .unwrap();
        assert!(matches!(input.state, MetadataResourceState::Ready(_)));
        assert!(matches!(
            session.owner(&owner(TARGET)).unwrap().forms[0].state,
            ModelState::Ready(_)
        ));
        assert_eq!(
            input_ready(&session, "Catalogs/Neighbor/object.yaml"),
            admitted
        );
        let result = session
            .module_completions(
                &form_module,
                session.revision(),
                offset,
                Locale::En,
                &QueryCancellation::default(),
            )
            .unwrap();
        assert_eq!(
            result.value.iter().any(|item| item.label == "NeighborItem"),
            admitted
        );
    }
}

/// Only saved transient cancellation is retryable; an editor-owned cancelled resource cannot fall back.
#[test]
fn targeted_preparation_retries_cancelled_saved_inputs_but_preserves_cancelled_overlays() {
    let (_directory, _source, mut session) = fixture(BODY);
    let module = owner(MODULE);
    let target = owner(TARGET);
    let resource = owner(RESOURCE);
    let request = InputRequest::default();
    let cancellation = QueryCancellation::default();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let stopped = InputRequest {
        cancellation: cancelled,
        ..InputRequest::default()
    };
    let offset = u32::try_from(BODY.find("Saved").unwrap()).unwrap();
    session
        .load_module_saved(&module, session.revision(), &request)
        .unwrap();
    session
        .load_saved(&target, session.revision(), &stopped)
        .unwrap();
    session
        .load_resource_saved(&resource, session.revision(), &stopped)
        .unwrap();
    assert!(matches!(
        session.selected_root.inputs.owner(&target).unwrap().state,
        InputState::Unavailable(InputFailure::Cancelled)
    ));
    session
        .prepare_completion_metadata(&module, session.revision(), offset, &request, &cancellation)
        .unwrap();
    assert!(labels(&session, offset, Locale::En).contains("Saved"));
    session
        .apply_resource_overlay(
            &resource,
            session.revision(),
            1,
            &test_predefined_project::data("Current"),
            &stopped,
        )
        .unwrap();
    let before = session.summary();
    session
        .prepare_completion_metadata(&module, session.revision(), offset, &request, &cancellation)
        .unwrap();
    assert_eq!(session.summary(), before);
    assert_eq!(
        session.metadata_resource(&resource).unwrap().state,
        MetadataResourceState::Unavailable(InputFailure::Cancelled)
    );
    assert!(!labels(&session, offset, Locale::En).contains("Saved"));
    session
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    assert!(labels(&session, offset, Locale::En).contains("Saved"));
}
