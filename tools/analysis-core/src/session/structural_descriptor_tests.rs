//! Structural descriptor coverage is independent of DCS payloads and unsupported query consumers.

use std::{fs, path::PathBuf};

use eska::{
    analysis_core::{InputOrigin, InputRequest},
    project_core::source_format::SourceReader,
};

use super::tests::{owner, support::TestDirectory};
use crate::{
    AnalysisScope, AnalysisSession, DiagnosticCode, DiagnosticsConfig, ModelState,
    QueryCancellation, SessionOptions,
    batch::{BatchOptions, check_semantic},
};

const LANGUAGE: &str = "Languages/Locale/object.yaml";
const TEMPLATE: &str = "Reports/Demo/Templates/Schema/template.yaml";
const RESOURCE: &str = "Reports/Demo/Templates/Schema/Template.xml";
#[path = "../../tests/support/structural_project.rs"]
mod fixture;

/// The shared fixture never touches another test's project, source or native input.
fn fixture() -> (TestDirectory, PathBuf) {
    let directory = TestDirectory::new();
    let root = fixture::project(&directory.0);
    fixture::settings(&root);
    (directory, root.join("src"))
}

/// The semantic profile here consists of explicitly selected native BSL rules, not DCS analysis.
fn rules() -> DiagnosticsConfig {
    DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::ParseError]),
        ..DiagnosticsConfig::default()
    }
}

/// Loaded structural leaves never populate a canonical kernel slot, and local admission stays source-only.
#[test]
fn language_template_current_models_are_structural_only_in_canonical_scope() {
    let (_directory, source) = fixture();
    let mut session = AnalysisSession::open(&source).unwrap();
    let request = InputRequest::default();
    for path in [LANGUAGE, TEMPLATE, "CommonTemplates/Global/template.yaml"] {
        let path = owner(path);
        assert!(matches!(
            session.owner(&path).unwrap().state,
            ModelState::Pending
        ));
        session
            .load_saved(&path, session.revision(), &request)
            .unwrap();
        assert_eq!(
            session.owner(&path).unwrap().state,
            ModelState::StructureOnly
        );
        assert!(
            session
                .resolve(&path, session.revision())
                .unwrap()
                .is_none()
        );
        let checked = session
            .metadata_diagnostics(&path, session.revision(), &QueryCancellation::default())
            .unwrap();
        assert!(
            checked.input_complete
                && checked.main_model_complete
                && checked.descriptor_models_complete
        );
    }
    let language = owner(LANGUAGE);
    let saved = fs::read_to_string(source.join(LANGUAGE)).unwrap();
    session
        .apply_overlay(&language, session.revision(), 4, &saved, &request)
        .unwrap();
    assert_eq!(
        session.owner(&language).unwrap().origin,
        InputOrigin::Overlay {
            document_version: 4
        }
    );
    fs::remove_file(source.join(LANGUAGE)).unwrap();
    session
        .close_overlay(&language, session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.owner(&language).unwrap().state,
        ModelState::Unavailable(_)
    ));
    fs::write(source.join(LANGUAGE), saved).unwrap();
    session
        .load_saved(&language, session.revision(), &request)
        .unwrap();
    assert_eq!(
        session.owner(&language).unwrap().state,
        ModelState::StructureOnly
    );
    let mut local = AnalysisSession::from_reader_with_scope(
        SourceReader::open(&source).unwrap(),
        SessionOptions::default(),
        AnalysisScope::LocalSyntax,
    )
    .unwrap();
    for path in [LANGUAGE, TEMPLATE] {
        let path = owner(path);
        local.load_saved(&path, local.revision(), &request).unwrap();
        assert_eq!(local.owner(&path).unwrap().state, ModelState::SourceOnly);
    }
    assert!(!local.summary().main_models_complete && !local.summary().descriptor_models_complete);
}

/// Dirty invalid structure clears coverage; saved invalidation cannot replace its newer editor bytes.
#[test]
fn structural_descriptor_dirty_invalid_watch_repair_and_close_keep_current_authority() {
    let (_directory, source) = fixture();
    let mut session = AnalysisSession::open(&source).unwrap();
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(session.summary().main_models_complete && session.summary().descriptor_models_complete);
    let path = owner(TEMPLATE);
    let saved = fs::read_to_string(source.join(TEMPLATE)).unwrap();
    let dirty = saved.replace("DataCompositionSchema", "FutureTemplateType");
    assert_ne!(saved, dirty);
    session
        .apply_overlay(&path, session.revision(), 8, &dirty, &request)
        .unwrap();
    let ModelState::Invalid { error, .. } = &session.owner(&path).unwrap().state else {
        panic!("current invalid field must clear structural coverage");
    };
    assert!(dirty[error.range.clone()].contains("FutureTemplateType"));
    assert!(
        !session.summary().main_models_complete && !session.summary().descriptor_models_complete
    );
    let current = session.revision();
    fs::write(source.join(TEMPLATE), &saved).unwrap();
    assert!(
        session
            .invalidate_saved_inputs(std::slice::from_ref(&path), current)
            .unwrap()
            .is_empty()
    );
    assert_eq!(session.revision(), current);
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Invalid { .. }
    ));
    assert!(session.resolve(&path, current).unwrap().is_none());
    assert!(
        session
            .apply_overlay(&path, current, 8, &saved, &request)
            .is_err()
    );
    assert_eq!(session.revision(), current);
    session
        .apply_overlay(&path, current, 9, &saved, &request)
        .unwrap();
    assert_eq!(
        session.owner(&path).unwrap().state,
        ModelState::StructureOnly
    );
    session
        .close_overlay(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(session.owner(&path).unwrap().origin, InputOrigin::Saved);
    assert!(session.summary().descriptor_models_complete);
    fs::remove_file(source.join(TEMPLATE)).unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&path), session.revision())
        .unwrap();
    assert_eq!(session.owner(&path).unwrap().state, ModelState::Pending);
    session
        .load_saved(&path, session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Unavailable(_)
    ));
    assert!(!session.summary().descriptor_models_complete);
}

/// Descriptor/selected BSL coverage must never imply resource, query syntax or consumer-context checks.
#[test]
fn dcs_and_query_content_are_outside_structural_and_selected_bsl_coverage() {
    for change in [
        "valid",
        "missing-resource",
        "invalid-resource",
        "missing-query",
        "invalid-query",
        "missing-context",
    ] {
        let (_directory, source) = fixture();
        let reader = SourceReader::open(&source).unwrap();
        let query = reader.query_paths().next().unwrap().clone();
        let module = reader.module_paths().next().unwrap().clone();
        match change {
            "missing-resource" => fs::remove_file(source.join(RESOURCE)).unwrap(),
            "invalid-resource" => fs::write(source.join(RESOURCE), b"not reference XML").unwrap(),
            "missing-query" => fs::remove_file(source.join(query.as_path())).unwrap(),
            "invalid-query" => fs::write(source.join(query.as_path()), "SELECT FROM").unwrap(),
            "missing-context" => fs::write(
                source.join(query.as_path()),
                "SELECT Unknown.Field FROM Catalog.Unknown AS Unknown",
            )
            .unwrap(),
            _ => (),
        }
        let report = check_semantic(
            SourceReader::open(&source).unwrap(),
            &rules(),
            &BatchOptions::default(),
        )
        .unwrap();
        assert!(
            report.coverage.metadata_models_complete,
            "{change}: {report:?}"
        );
        assert_eq!(
            report.coverage.semantic_profile_complete,
            Some(true),
            "{change}: {report:?}"
        );
        assert!(report.metadata_issues.is_empty(), "{change}: {report:?}");
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].source, module);
        assert!(report.files.iter().all(|file| file.findings.is_empty()));
    }
}
