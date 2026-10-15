//! Real host queries share metadata/BSL revisions and source quotas without native file-ID wrappers.

use std::{fs, path::PathBuf};

use super::*;
use crate::{
    AnalysisScope, DiagnosticCode, Locale, QueryTextState, query_creation_fixture as fixture,
    test_support::TestDirectory,
};
use eska::{
    analysis_core::{InputFailure, InputOrigin},
    project_core::source_format::import_designer_queries,
};

/// Every native type owns its isolated source tree; scope admission is explicit for local syntax.
fn host(
    kind: &str,
    options: crate::SessionOptions,
) -> (TestDirectory, PathBuf, SourcePath, AnalysisSession) {
    let directory = TestDirectory::new();
    let native = fixture::native(&directory.0, kind);
    let root = directory.0.join("source");
    import_designer_queries(&native.input, &root).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let native_resource = SourcePath::parse(native.schema.to_str().unwrap()).unwrap();
    let source = reader
        .query_paths()
        .find(|path| reader.declared_query(path).unwrap().native_resource == &native_resource)
        .unwrap()
        .clone();
    let session =
        AnalysisSession::from_reader_with_scope(reader, options, AnalysisScope::LocalSyntax)
            .unwrap();
    (directory, root, source, session)
}

/// Exact byte text survives disk deletion and maps native parse errors independently of locale.
#[test]
fn all_four_hosts_share_revision_exact_input_and_native_syntax_without_metadata_or_file_ids() {
    for kind in fixture::TYPES {
        let (_directory, root, source, mut session) = host(kind, crate::SessionOptions::default());
        assert_eq!(session.query_paths().count(), 2);
        assert_eq!(session.query_summary().pending, 2);
        assert_eq!(session.resources().query_text_bytes, 0);
        let files = session.selected_root.module_sources.clone();
        let text = "\u{feff}// Я😀\r\nВЫБРАТЬ Таблица. ИЗ Справочник.Нет КАК Таблица\r\n";
        session
            .apply_query_overlay(
                &source,
                session.revision(),
                4,
                text,
                &InputRequest::default(),
            )
            .unwrap();
        let input_revision = session.revision();
        let metadata = session.owner_paths().next().unwrap().clone();
        session
            .load_saved(&metadata, session.revision(), &InputRequest::default())
            .unwrap();
        assert_ne!(session.revision(), input_revision);
        fs::remove_file(root.join(source.as_path())).unwrap();
        let mut positions = Vec::new();
        for locale in [Locale::Ru, Locale::En] {
            let config = DiagnosticsConfig {
                locale,
                ..DiagnosticsConfig::default()
            };
            let report = session
                .query_syntax_diagnostics(
                    &source,
                    session.revision(),
                    &config,
                    &InputRequest::default(),
                )
                .unwrap();
            assert_eq!(report.revision, session.revision());
            assert_eq!(report.result.input_revision, input_revision);
            assert_eq!(report.result.scope, crate::QuerySyntaxScope::SyntaxOnly);
            assert!(!report.result.diagnostics.is_empty());
            assert!(
                report
                    .result
                    .diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.code == DiagnosticCode::QueryParseError)
            );
            if locale == Locale::En {
                assert!(
                    report
                        .result
                        .diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.message == "Syntax error")
                );
            }
            positions.push(
                report
                    .result
                    .diagnostics
                    .iter()
                    .map(|diagnostic| (diagnostic.code, diagnostic.range))
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(positions[0], positions[1]);
        assert_eq!(session.selected_root.module_sources, files);
        assert_eq!(session.resources().query_text_bytes, text.len() as u64);
        let input = session.query_input(&source).unwrap();
        assert_eq!(
            input.origin,
            InputOrigin::Overlay {
                document_version: 4
            }
        );
        let QueryTextState::Ready(current) = &input.state else {
            panic!("current overlay must remain readable");
        };
        assert_eq!(current.as_ref(), text);
    }
}

/// A stale host clock, old document version or unknown path cannot mutate current query or byte usage.
#[test]
fn validation_refusals_preserve_global_clock_query_hash_and_other_inputs() {
    let (_directory, _, source, mut session) =
        host("configuration", crate::SessionOptions::default());
    let old = session.revision();
    session
        .apply_query_overlay(&source, old, 5, "SELECT 1", &InputRequest::default())
        .unwrap();
    let expected = session.query_input(&source).unwrap().clone();
    let resources = session.resources();
    let current = session.revision();
    for result in [
        session.apply_query_overlay(&source, old, 6, "SELECT 2", &InputRequest::default()),
        session.apply_query_overlay(&source, current, 5, "SELECT 2", &InputRequest::default()),
        session.load_query_saved(&source, current, &InputRequest::default()),
        session.release_query_saved(&source, current),
        session.load_query_saved(
            &SourcePath::parse("Undeclared.sdbl").unwrap(),
            current,
            &InputRequest::default(),
        ),
    ] {
        assert!(result.is_err());
    }
    assert_eq!(session.revision(), current);
    assert_eq!(session.query_input(&source), Some(&expected));
    assert_eq!(session.resources(), resources);
    assert!(matches!(
        session.query_syntax_diagnostics(
            &source,
            old,
            &DiagnosticsConfig::default(),
            &InputRequest::default()
        ),
        Err(QuerySyntaxRequestError::Request(
            SessionRequestError::StaleRevision { .. }
        ))
    ));
    session.revision = SessionRevision(u64::MAX);
    assert_eq!(
        session.close_query_overlay(&source, session.revision(), &InputRequest::default()),
        Err(SessionRequestError::RevisionExhausted)
    );
    assert_eq!(session.query_input(&source), Some(&expected));
}

/// Query replacement reuses its reserve, while a BSL body and metadata YAML consume the same quota.
#[test]
fn shared_quota_failures_clear_failed_editor_authority_without_source_fallback() {
    let (_directory, root, source, mut session) =
        host("configuration", crate::SessionOptions::default());
    let owner = session
        .owner_paths()
        .find(|path| path.as_str() == "configuration.yaml")
        .unwrap()
        .clone();
    let owner_bytes = fs::metadata(root.join(owner.as_path())).unwrap().len();
    session.options.max_retained_source_bytes = owner_bytes + 8;
    session
        .load_saved(&owner, session.revision(), &InputRequest::default())
        .unwrap();
    let module = session.module_paths().next().unwrap().clone();
    session
        .apply_module_overlay(
            &module,
            session.revision(),
            1,
            "body",
            &InputRequest::default(),
        )
        .unwrap();
    session
        .apply_query_overlay(
            &source,
            session.revision(),
            1,
            "text",
            &InputRequest::default(),
        )
        .unwrap();
    assert_eq!(session.resources().total_source_bytes, owner_bytes + 8);
    session
        .apply_query_overlay(
            &source,
            session.revision(),
            2,
            "large",
            &InputRequest::default(),
        )
        .unwrap();
    let input = session.query_input(&source).unwrap();
    assert!(matches!(
        input.state,
        QueryTextState::Unavailable(InputFailure::SourceBudgetExceeded { .. })
    ));
    assert_eq!(
        input.origin,
        InputOrigin::Overlay {
            document_version: 2
        }
    );
    assert!(input.source_sha256.is_none());
    assert_eq!(session.resources().query_text_bytes, 0);
    assert_eq!(session.resources().module_text_bytes, 4);
    assert!(matches!(
        session.query_syntax_diagnostics(
            &source,
            session.revision(),
            &DiagnosticsConfig::default(),
            &InputRequest::default()
        ),
        Err(QuerySyntaxRequestError::Syntax(
            QuerySyntaxError::Unavailable(_)
        ))
    ));
    assert!(matches!(
        session.load_query_saved(&source, session.revision(), &InputRequest::default()),
        Err(SessionRequestError::OverlayActive {
            document_version: 2
        })
    ));
    let cancelled = InputRequest::default();
    cancelled.cancellation.cancel();
    session
        .apply_query_overlay(&source, session.revision(), 3, "x", &cancelled)
        .unwrap();
    assert_eq!(
        session.query_input(&source).unwrap().state,
        QueryTextState::Unavailable(InputFailure::Cancelled)
    );
    assert_eq!(session.resources().query_text_bytes, 0);
}

/// Saved watchers skip overlays, preflight the whole batch, and close reads the actual deleted file.
#[test]
fn saved_watch_invalidation_and_close_never_restore_stale_saved_text() {
    let (_directory, root, source, mut session) =
        host("configuration", crate::SessionOptions::default());
    session
        .load_query_saved(&source, session.revision(), &InputRequest::default())
        .unwrap();
    let before = session.revision();
    assert!(
        session
            .invalidate_saved_inputs(
                &[source.clone(), SourcePath::parse("Unknown.sdbl").unwrap()],
                before
            )
            .is_err()
    );
    assert_eq!(session.revision(), before);
    assert_eq!(
        session
            .invalidate_saved_inputs(&[source.clone(), source.clone()], before)
            .unwrap(),
        vec![source.clone()]
    );
    assert_eq!(session.revision().0, before.0 + 1);
    assert_eq!(
        session.query_input(&source).unwrap().state,
        QueryTextState::Pending
    );
    assert_eq!(session.resources().query_text_bytes, 0);
    fs::write(root.join(source.as_path()), "SELECT 8").unwrap();
    session
        .load_query_saved(&source, session.revision(), &InputRequest::default())
        .unwrap();
    session
        .apply_query_overlay(
            &source,
            session.revision(),
            1,
            "SELECT 9",
            &InputRequest::default(),
        )
        .unwrap();
    fs::remove_file(root.join(source.as_path())).unwrap();
    let revision = session.revision();
    assert!(
        session
            .invalidate_saved_inputs(std::slice::from_ref(&source), revision)
            .unwrap()
            .is_empty()
    );
    assert_eq!(session.revision(), revision);
    session
        .close_query_overlay(&source, revision, &InputRequest::default())
        .unwrap();
    assert_eq!(
        session.query_input(&source).unwrap().origin,
        InputOrigin::Saved
    );
    assert!(matches!(
        session.query_input(&source).unwrap().state,
        QueryTextState::Unavailable(InputFailure::Source(_))
    ));
    assert_eq!(session.resources().query_text_bytes, 0);
    session
        .invalidate_saved_inputs(std::slice::from_ref(&source), session.revision())
        .unwrap();
    fs::write(root.join(source.as_path()), "SELECT 10").unwrap();
    session
        .load_query_saved(&source, session.revision(), &InputRequest::default())
        .unwrap();
    let QueryTextState::Ready(text) = &session.query_input(&source).unwrap().state else {
        panic!("new saved bytes must be current");
    };
    assert_eq!(text.as_ref(), "SELECT 10");
}

/// Pending input and cancellation are explicit errors, while disabled rules preserve syntax-only coverage.
#[test]
fn cancellation_and_rule_policy_cannot_publish_false_clean_context() {
    let (_directory, _, source, mut session) = host("report", crate::SessionOptions::default());
    assert!(matches!(
        session.query_syntax_diagnostics(
            &source,
            session.revision(),
            &DiagnosticsConfig::default(),
            &InputRequest::default()
        ),
        Err(QuerySyntaxRequestError::Syntax(QuerySyntaxError::Pending))
    ));
    session
        .apply_query_overlay(
            &source,
            session.revision(),
            1,
            "SELECT FROM",
            &InputRequest::default(),
        )
        .unwrap();
    let request = InputRequest::default();
    request.cancellation.cancel();
    let revision = session.revision();
    assert_eq!(
        session
            .query_syntax_diagnostics(&source, revision, &DiagnosticsConfig::default(), &request)
            .unwrap_err(),
        QuerySyntaxRequestError::Syntax(QuerySyntaxError::Cancelled)
    );
    assert_eq!(session.revision(), revision);
    let config = DiagnosticsConfig {
        disabled: vec![DiagnosticCode::QueryParseError],
        ..DiagnosticsConfig::default()
    };
    let result = session
        .query_syntax_diagnostics(&source, revision, &config, &InputRequest::default())
        .unwrap();
    assert!(!result.result.parse_enabled);
    assert!(result.result.diagnostics.is_empty());
    assert_eq!(result.result.scope, crate::QuerySyntaxScope::SyntaxOnly);
}
