//! Actual native metadata and BSL share distinct root identities and topology-selected current inputs.

use std::fs;

use eska::{analysis_core::InputRequest, project_core::source_format::SourceReader};
use project_model::ProjectConfig;

use crate::{
    AnalysisSession, Locale, ModelState, QueryCancellation, SessionOptions,
    composition::{SourceKey, admit},
    test_composition_project as fixture,
    test_support::TestDirectory,
};

/// Readers arrive out of order, while admission and native IDs retain their declared topology order.
fn session(project: &fixture::CompositionProject) -> AnalysisSession {
    assert_eq!(project.selected_accepted_sha256.len(), 64);
    fixture::parse_only(project);
    let config = ProjectConfig::load_from_file(&project.settings).unwrap();
    let readers = ["sibling", "b", "base", "a"]
        .map(|name| SourceReader::open(&project.roots[name].join("src")).unwrap())
        .into();
    let composition = admit(
        &project.roots["b"],
        &config,
        readers,
        &project.roots["b"].join("src"),
    )
    .unwrap();
    AnalysisSession::from_composition(composition, SessionOptions::default()).unwrap()
}

/// Equal paths retain independent editor versions while failures reserve no stale source bytes.
#[test]
fn composition_foreign_overlays_share_quota_clock_and_no_saved_fallback() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0);
    let mut session = session(&project);
    let request = InputRequest::default();
    let selected = session.selected_source(&project.module);
    let foreign = SourceKey {
        root: session.root_ids().find(|id| id.index() == 1).unwrap(),
        source: project.module,
    };
    let text = "Procedure Current() Export\nEndProcedure\n";
    session.options.max_retained_source_bytes = text.len() as u64;
    session
        .apply_module_overlay_at(&foreign, session.revision(), 4, text, &request)
        .unwrap();
    let current = session.revision();
    session
        .apply_module_overlay_at(&selected, current, 4, text, &request)
        .unwrap();
    assert_ne!(session.revision(), current);
    assert!(matches!(
        session.module_at(&selected).unwrap().state,
        crate::ModuleState::Unavailable(crate::ModuleFailure::Input(
            eska::analysis_core::InputFailure::SourceBudgetExceeded { .. }
        ))
    ));
    assert!(matches!(
        session.module_at(&foreign).unwrap().state,
        crate::ModuleState::Ready { .. }
    ));
    assert_eq!(session.resources().module_text_bytes, text.len() as u64);
    let revision = session.revision();
    let resources = session.resources();
    assert!(matches!(
        session.apply_module_overlay_at(&foreign, revision, 4, "", &request),
        Err(crate::SessionRequestError::StaleDocumentVersion { .. })
    ));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
    let cancelled = InputRequest::default();
    cancelled.cancellation.cancel();
    session
        .apply_module_overlay_at(&foreign, revision, 5, text, &cancelled)
        .unwrap();
    assert!(matches!(
        session.module_at(&foreign).unwrap().state,
        crate::ModuleState::Unavailable(crate::ModuleFailure::Input(
            eska::analysis_core::InputFailure::Cancelled
        ))
    ));
    assert_eq!(session.resources().module_text_bytes, 0);
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.module_at(&foreign).unwrap().state,
        crate::ModuleState::Unavailable(_)
    ));
    session
        .close_module_overlay_at(&foreign, session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.module_at(&foreign).unwrap().state,
        crate::ModuleState::Ready { .. }
    ));
}

/// Loading visible contributors never aliases equal paths or reads an unrelated sibling body.
#[test]
fn composition_uses_one_file_id_universe_and_excludes_sibling_inputs() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0);
    let mut session = session(&project);
    let module = project.module.clone();
    let roots = session.root_ids().collect::<Vec<_>>();
    let files = roots
        .iter()
        .map(|id| session.root_inputs(*id).unwrap().modules[&module].file)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(files.len(), 4);
    assert_eq!(session.selected_root_id().index(), 2);
    fs::remove_file(project.roots["sibling"].join("src").join(module.as_path())).unwrap();
    session
        .load_semantic_inputs(session.revision(), &InputRequest::default())
        .unwrap();
    session.require_current_composition().unwrap();
    let offset = u32::try_from(fixture::CALLER.find("Value.\n").unwrap() + "Value.".len()).unwrap();
    let result = session
        .module_completions(
            &module,
            session.revision(),
            offset,
            Locale::En,
            &QueryCancellation::default(),
        )
        .unwrap();
    let labels = result
        .value
        .iter()
        .map(|item| item.label.as_str())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"BaseField"), "{labels:?}");
    assert!(labels.contains(&"AField"), "{labels:?}");
    assert!(!labels.contains(&"SiblingField"));
    let sibling = roots.iter().copied().find(|id| id.index() == 3).unwrap();
    assert_eq!(
        session
            .module_at(&SourceKey {
                root: sibling,
                source: module
            })
            .unwrap()
            .state,
        crate::ModuleState::Pending
    );
}

/// An invalid current adopted descriptor cannot revive the base object as a clean semantic result.
#[test]
fn composition_invalid_adopted_overlay_has_no_base_fallback_and_keeps_shared_revision() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0);
    let mut session = session(&project);
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let owner = project.catalog.clone();
    let root = session.root_ids().find(|id| id.index() == 1).unwrap();
    let key = SourceKey {
        root,
        source: owner.clone(),
    };
    let base_root = session.root_ids().find(|id| id.index() == 0).unwrap();
    let base = session
        .owner_at(&SourceKey {
            root: base_root,
            source: owner,
        })
        .unwrap()
        .clone();
    let before = session.revision();
    session
        .apply_overlay_at(&key, before, 4, "broken: [", &request)
        .unwrap();
    assert_ne!(session.revision(), before);
    assert!(!matches!(
        session.owner_at(&key).unwrap().state,
        ModelState::Ready(_)
    ));
    assert!(
        matches!(session.require_current_composition(), Err(crate::SessionRequestError::CompositionInputIncomplete(source)) if source == key)
    );
    assert_eq!(
        session
            .owner_at(&SourceKey {
                root: base_root,
                source: key.source.clone()
            })
            .unwrap(),
        &base
    );
    let saved =
        fs::read_to_string(project.roots["a"].join("src").join(key.source.as_path())).unwrap();
    session
        .apply_overlay_at(&key, session.revision(), 5, &saved, &request)
        .unwrap();
    session.require_current_composition().unwrap();
}

/// Native UUID links, rather than equal object names, decide whether the base contributes fields.
#[test]
fn composition_current_adoption_uuid_controls_inheritance_without_name_fallback() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0);
    let mut session = session(&project);
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let key = SourceKey {
        root: session.root_ids().find(|id| id.index() == 1).unwrap(),
        source: project.catalog.clone(),
    };
    let saved =
        fs::read_to_string(project.roots["a"].join("src").join(key.source.as_path())).unwrap();
    let changed = saved.replace(
        "30000000-0000-4000-8000-000000000001",
        "30000000-0000-4000-8000-000000000099",
    );
    assert_ne!(saved, changed);
    session
        .apply_overlay_at(&key, session.revision(), 1, &changed, &request)
        .unwrap();
    session.require_current_composition().unwrap();
    let offset = u32::try_from(fixture::CALLER.find("Value.\n").unwrap() + "Value.".len()).unwrap();
    let result = session
        .module_completions(
            &project.module,
            session.revision(),
            offset,
            Locale::En,
            &QueryCancellation::default(),
        )
        .unwrap();
    let labels = result
        .value
        .iter()
        .map(|item| item.label.as_str())
        .collect::<Vec<_>>();
    assert!(!labels.contains(&"BaseField"), "{labels:?}");
    session
        .close_overlay_at(&key, session.revision(), &request)
        .unwrap();
    let restored = session
        .module_completions(
            &project.module,
            session.revision(),
            offset,
            Locale::En,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert!(restored.value.iter().any(|item| item.label == "BaseField"));
}
