//! Current canonical query rules share metadata authority, visibility and the source reserve.

use crate::{
    AnalysisScope, AnalysisSession, DiagnosticCode, DiagnosticsConfig, Locale,
    QueryAnalysisRequestError, QueryCancellation, QueryError, SessionOptions, SessionRequestError,
    test_query_analysis_project as fixture, test_support::TestDirectory,
};
use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    project_core::source_format::SourceReader,
};
use std::fs;

/// A fresh canonical host retains no BSL or query bytes before explicit input preparation.
fn host(project: &fixture::Project) -> AnalysisSession {
    AnalysisSession::open(&project.root.join("src")).unwrap()
}

/// Locale changes human prose while native rule identity and exact source ranges remain fixed.
fn assert_rule_locales(
    session: &AnalysisSession,
    source: &eska::project_core::source_format::SourcePath,
    code: DiagnosticCode,
) {
    let request = InputRequest::default();
    let mut positions = vec![];
    for locale in [Locale::Ru, Locale::En] {
        let config = DiagnosticsConfig {
            locale,
            only_enabled: Some(vec![code]),
            ..DiagnosticsConfig::default()
        };
        let result = session
            .query_analysis_diagnostics(
                source,
                session.revision(),
                &config,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        assert!(
            result
                .result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code),
            "{code:?}: {:?}",
            result.result.diagnostics
        );
        assert_eq!(result.result.enabled_rules, vec![code]);
        if locale == Locale::En && code != DiagnosticCode::QueryParseError {
            assert!(
                result
                    .result
                    .diagnostics
                    .iter()
                    .all(|d| d.message.is_ascii())
            );
        }
        positions.push(
            result
                .result
                .diagnostics
                .iter()
                .map(|d| (d.code, d.range))
                .collect::<Vec<_>>(),
        );
        assert_eq!(result.root, session.selected_root_id());
        assert_eq!(result.revision, session.revision());
    }
    assert_eq!(positions[0], positions[1]);
}

/// Repeated current requests retain the same explicit source/configuration/cancellation contract.
fn current(
    session: &AnalysisSession,
    project: &fixture::Project,
    config: &DiagnosticsConfig,
    request: &InputRequest,
) -> crate::QueryAnalysisDiagnostics {
    session
        .query_analysis_diagnostics(
            &project.source,
            session.revision(),
            config,
            request,
            &QueryCancellation::default(),
        )
        .unwrap()
}

/// Rule matrices keep a positive diagnostic and a valid control, rather than testing only empty results.
#[test]
fn canonical_query_rules_cover_metadata_and_quality_without_reading_bsl() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let before = fixture::snapshot(&project);
    let mut session = host(&project);
    let request = InputRequest::default();
    session
        .prepare_query_analysis(session.revision(), &request)
        .unwrap();
    let cases = [
        (
            "SELECT Data.Missing AS Value FROM Catalog.Items AS Data",
            DiagnosticCode::UnknownFieldInQuery,
        ),
        (
            "SELECT Data.Ref AS Value FROM Catalog.Missing AS Data",
            DiagnosticCode::QueryToMissingMetadata,
        ),
        (
            "SELECT CurrentField AS Value FROM Catalog.Items AS A INNER JOIN Catalog.Items AS B ON A.Ref=B.Ref",
            DiagnosticCode::AmbiguousFieldInQuery,
        ),
        (
            "SELECT A.Ref AS Value FROM Catalog.Items AS A INNER JOIN Catalog.Items AS A ON A.Ref=A.Ref",
            DiagnosticCode::DuplicateAliasInQuery,
        ),
        ("SELECT FROM", DiagnosticCode::QueryParseError),
    ];
    for (index, (text, code)) in cases.into_iter().enumerate() {
        session
            .apply_query_overlay(
                &project.source,
                session.revision(),
                i64::try_from(index).unwrap() + 1,
                text,
                &request,
            )
            .unwrap();
        assert_rule_locales(&session, &project.source, code);
        let syntax = session
            .query_syntax_diagnostics(
                &project.source,
                session.revision(),
                &DiagnosticsConfig::default(),
                &request,
            )
            .unwrap();
        assert!(
            syntax
                .result
                .diagnostics
                .iter()
                .all(|d| d.code == DiagnosticCode::QueryParseError)
        );
    }
    assert_eq!(session.module_summary().ready, 0);
    assert_eq!(session.resources().module_text_bytes, 0);
    assert_eq!(session.query_summary().ready, 1);
    assert_eq!(fixture::snapshot(&project), before);
}

/// A current metadata failure cannot be interpreted as a proven missing field or older saved value.
#[test]
fn canonical_query_dirty_metadata_stale_clock_and_close_preserve_current_authority() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = host(&project);
    let request = InputRequest::default();
    let text = "SELECT Data.CurrentField AS Value FROM Catalog.Items AS Data";
    session
        .apply_query_overlay(&project.source, session.revision(), 1, text, &request)
        .unwrap();
    session
        .prepare_query_analysis(session.revision(), &request)
        .unwrap();
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::UnknownFieldInQuery]),
        ..DiagnosticsConfig::default()
    };
    let query_revision = session.query_input(&project.source).unwrap().revision;
    let saved =
        fs::read_to_string(project.root.join("src").join(project.catalog.as_path())).unwrap();
    let valid = current(&session, &project, &config, &request);
    assert!(valid.result.diagnostics.is_empty());
    let stale = session.revision();
    session
        .apply_overlay(
            &project.catalog,
            stale,
            1,
            &saved.replace("CurrentField", "DirtyField"),
            &request,
        )
        .unwrap();
    assert!(matches!(
        session.query_analysis_diagnostics(
            &project.source,
            stale,
            &config,
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryAnalysisRequestError::Request(
            SessionRequestError::StaleRevision { .. }
        ))
    ));
    let dirty = current(&session, &project, &config, &request);
    assert_eq!(dirty.result.input_revision, query_revision);
    assert_eq!(
        dirty.result.diagnostics[0].code,
        DiagnosticCode::UnknownFieldInQuery
    );
    session
        .apply_overlay(
            &project.catalog,
            session.revision(),
            2,
            "broken: [",
            &request,
        )
        .unwrap();
    session
        .prepare_query_analysis(session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.query_analysis_diagnostics(
            &project.source,
            session.revision(),
            &config,
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryAnalysisRequestError::Request(
            SessionRequestError::CompositionInputIncomplete(_)
        ))
    ));
    session
        .close_overlay(&project.catalog, session.revision(), &request)
        .unwrap();
    let restored = current(&session, &project, &config, &request);
    assert!(restored.result.diagnostics.is_empty());
    assert_eq!(
        session.query_input(&project.source).unwrap().origin,
        InputOrigin::Overlay {
            document_version: 1
        }
    );
    assert_saved_watch(&mut session, &project, &saved, &config, &request);
}

/// Saved metadata changes invalidate the old contribution before the same query can run again.
fn assert_saved_watch(
    session: &mut AnalysisSession,
    project: &fixture::Project,
    saved: &str,
    config: &DiagnosticsConfig,
    request: &InputRequest,
) {
    fs::write(
        project.root.join("src").join(project.catalog.as_path()),
        saved.replace("CurrentField", "WatchedField"),
    )
    .unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&project.catalog), session.revision())
        .unwrap();
    assert!(
        session
            .query_analysis_diagnostics(
                &project.source,
                session.revision(),
                config,
                request,
                &QueryCancellation::default()
            )
            .is_err()
    );
    session
        .prepare_query_analysis(session.revision(), request)
        .unwrap();
    assert_eq!(
        session
            .query_analysis_diagnostics(
                &project.source,
                session.revision(),
                config,
                request,
                &QueryCancellation::default()
            )
            .unwrap()
            .result
            .diagnostics
            .len(),
        1
    );
}

/// Stopped/disabled queries and shared source exhaustion cannot publish a checked empty result.
#[test]
fn canonical_query_rule_preflight_cancellation_and_quota_are_explicit() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = host(&project);
    let request = InputRequest::default();
    let disabled = DiagnosticsConfig {
        only_enabled: Some(vec![]),
        ..DiagnosticsConfig::default()
    };
    let initial = session.revision();
    assert_eq!(
        session
            .query_analysis_diagnostics(
                &project.source,
                initial,
                &disabled,
                &request,
                &QueryCancellation::default()
            )
            .unwrap_err(),
        QueryAnalysisRequestError::Disabled
    );
    assert_eq!(session.revision(), initial);
    let cancelled = QueryCancellation::default();
    cancelled.cancel();
    assert_eq!(
        session
            .query_analysis_diagnostics(
                &project.source,
                initial,
                &DiagnosticsConfig::default(),
                &request,
                &cancelled
            )
            .unwrap_err(),
        QueryAnalysisRequestError::Native(QueryError::Cancelled)
    );
    let text = "SELECT 1 AS Value";
    session.options.max_retained_source_bytes = text.len() as u64;
    session
        .apply_query_overlay(&project.source, session.revision(), 1, text, &request)
        .unwrap();
    session
        .prepare_query_analysis(session.revision(), &request)
        .unwrap();
    assert!(session.resources().total_source_bytes <= text.len() as u64);
    assert!(matches!(
        session.query_analysis_diagnostics(
            &project.source,
            session.revision(),
            &DiagnosticsConfig::default(),
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryAnalysisRequestError::Request(
            SessionRequestError::CompositionInputIncomplete(_)
        ))
    ));
    let cancelled_input = InputRequest::default();
    cancelled_input.cancellation.cancel();
    session
        .apply_query_overlay(
            &project.source,
            session.revision(),
            2,
            text,
            &cancelled_input,
        )
        .unwrap();
    assert_eq!(session.resources().query_text_bytes, 0);
    assert!(matches!(
        session.query_input(&project.source).unwrap().state,
        crate::QueryTextState::Unavailable(_)
    ));
}

/// A failed editor replacement remains authoritative until explicit close and saved reload.
#[test]
fn canonical_query_failed_editor_input_never_falls_back_to_saved_text() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = host(&project);
    let request = InputRequest::default();
    let config = DiagnosticsConfig::default();
    session
        .prepare_query_analysis(session.revision(), &request)
        .unwrap();
    let cancelled = InputRequest::default();
    cancelled.cancellation.cancel();
    session
        .apply_query_overlay(
            &project.source,
            session.revision(),
            1,
            "SELECT 1",
            &cancelled,
        )
        .unwrap();
    assert_eq!(
        session
            .query_analysis_diagnostics(
                &project.source,
                session.revision(),
                &config,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap_err(),
        QueryAnalysisRequestError::Input(crate::QuerySyntaxError::Unavailable(
            InputFailure::Cancelled
        ))
    );
    assert_eq!(
        session.query_input(&project.source).unwrap().origin,
        InputOrigin::Overlay {
            document_version: 1
        }
    );
    session
        .close_query_overlay(&project.source, session.revision(), &request)
        .unwrap();
    session
        .load_query_saved(&project.source, session.revision(), &request)
        .unwrap();
    assert_eq!(
        current(&session, &project, &config, &request).result.origin,
        InputOrigin::Saved
    );
}

/// External objects and standalone CFE syntax cannot silently invent an owning-base context.
#[test]
fn query_analysis_requires_configuration_while_old_syntax_stays_available() {
    for kind in fixture::TYPES
        .into_iter()
        .filter(|kind| *kind != "configuration")
    {
        let directory = TestDirectory::new();
        let project = fixture::project(&directory.0, kind);
        let reader = SourceReader::open(&project.root.join("src")).unwrap();
        let scope = if kind == "extension" {
            AnalysisScope::LocalSyntax
        } else {
            AnalysisScope::CanonicalExternal
        };
        let mut session =
            AnalysisSession::from_reader_with_scope(reader, SessionOptions::default(), scope)
                .unwrap();
        let request = InputRequest::default();
        session
            .load_query_saved(&project.source, session.revision(), &request)
            .unwrap();
        let required = if kind == "extension" {
            SessionRequestError::ContextRequired
        } else {
            SessionRequestError::ConfigurationContextRequired
        };
        assert_eq!(
            session
                .prepare_query_analysis(session.revision(), &request)
                .unwrap_err(),
            required
        );
        assert_eq!(
            session
                .query_analysis_diagnostics(
                    &project.source,
                    session.revision(),
                    &DiagnosticsConfig::default(),
                    &request,
                    &QueryCancellation::default()
                )
                .unwrap_err(),
            QueryAnalysisRequestError::Request(required)
        );
        assert!(
            session
                .query_syntax_diagnostics(
                    &project.source,
                    session.revision(),
                    &DiagnosticsConfig::default(),
                    &request
                )
                .is_ok()
        );
    }
}

/// An adopted foreign contribution is current input; neither an unrelated sibling nor older base wins.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
#[test]
fn query_analysis_cfe_visibility_current_adoption_and_snapshot_binding() {
    let directory = TestDirectory::new();
    let (project, foreign, catalog) = fixture::composition(&directory.0);
    let selected = SourceReader::open(&project.root.join("src")).unwrap();
    let settings =
        project_model::ProjectConfig::load_from_file(&project.root.join("bsl-analyzer.toml"))
            .unwrap();
    let readers = ["base", "a", "sibling"]
        .map(|name| SourceReader::open(&directory.0.join(name).join("src")).unwrap());
    let roots = std::iter::once(selected).chain(readers).collect();
    let composition =
        crate::composition::admit(&project.root, &settings, roots, &project.root.join("src"))
            .unwrap();
    let mut session =
        AnalysisSession::from_composition(composition, SessionOptions::default()).unwrap();
    let request = InputRequest::default();
    let text = "SELECT Data.BaseField AS Base, Data.AField AS Extension, Data.SiblingField AS Missing FROM Catalog.Items AS Data";
    session
        .apply_query_overlay(&project.source, session.revision(), 1, text, &request)
        .unwrap();
    session
        .prepare_query_analysis(session.revision(), &request)
        .unwrap();
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::UnknownFieldInQuery]),
        ..DiagnosticsConfig::default()
    };
    let result = session
        .query_analysis_diagnostics(
            &project.source,
            session.revision(),
            &config,
            &request,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert_eq!(
        result.result.diagnostics.len(),
        1,
        "{:?}",
        result.result.diagnostics
    );
    let diagnostic = &result.result.diagnostics[0];
    assert!(text[std::ops::Range::<usize>::from(diagnostic.range)].contains("SiblingField"));
    assert_eq!(result.root.index(), 2);
    assert_eq!(
        result.composition_sha256.as_deref(),
        session.composition_sha256()
    );
    assert_eq!(session.resources().module_text_bytes, 0);
    let key = crate::composition::SourceKey {
        root: session.root_ids().find(|id| id.index() == 1).unwrap(),
        source: catalog,
    };
    let saved = fs::read_to_string(foreign.join("src").join(key.source.as_path())).unwrap();
    session
        .apply_overlay_at(&key, session.revision(), 1, "broken: [", &request)
        .unwrap();
    assert!(matches!(
        session.query_analysis_diagnostics(
            &project.source,
            session.revision(),
            &config,
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryAnalysisRequestError::Request(
            SessionRequestError::CompositionInputIncomplete(_)
        ))
    ));
    session
        .apply_overlay_at(&key, session.revision(), 2, &saved, &request)
        .unwrap();
    assert_eq!(
        session
            .query_analysis_diagnostics(
                &project.source,
                session.revision(),
                &config,
                &request,
                &QueryCancellation::default()
            )
            .unwrap()
            .result
            .diagnostics
            .len(),
        1
    );
}
