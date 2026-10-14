//! Explicit source admission shares current-input authority without creating metadata semantics.

use std::fs;

use eska::{
    analysis_core::{InputRequest, MetadataInputs},
    project_core::{ProjectType, source_format::SourceReader},
};
use ide_db::RootDatabaseImpl;

use super::tests::support::TestDirectory;
use super::*;
use crate::{DiagnosticCode, DiagnosticsConfig, ModuleState, QueryCancellation, QueryError};

#[path = "../../tests/support/local_syntax_project.rs"]
mod fixture;

/// Salsa execution names prove local diagnostics do not compute a metadata configuration or resolver.
#[test]
fn all_four_local_scopes_check_current_source_without_canonical_metadata() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::BAD, false);
        let database = RootDatabaseImpl::new_with_salsa_events();
        let mut session = AnalysisSession::with_inputs(
            MetadataInputs::open(&root.join("src")).unwrap(),
            database,
            SessionOptions::default(),
            AnalysisScope::LocalSyntax,
        )
        .unwrap();
        assert!(session.database.all_config_paths().is_empty());
        let request = InputRequest::default();
        let owners = session.owner_paths().cloned().collect::<Vec<_>>();
        for owner in owners {
            session
                .load_saved(&owner, session.revision(), &request)
                .unwrap();
            assert_eq!(session.owner(&owner).unwrap().state, ModelState::SourceOnly);
            assert!(
                session
                    .resolve(&owner, session.revision())
                    .unwrap()
                    .is_none()
            );
            let diagnostics = session
                .metadata_diagnostics(&owner, session.revision(), &QueryCancellation::default())
                .unwrap();
            assert!(
                diagnostics.input_complete
                    && !diagnostics.main_model_complete
                    && !diagnostics.descriptor_models_complete
            );
        }
        session
            .load_module_saved(&module, session.revision(), &request)
            .unwrap();
        let result = session
            .module_syntax_diagnostics(
                &module,
                session.revision(),
                &DiagnosticsConfig::default(),
                &QueryCancellation::default(),
            )
            .unwrap();
        assert!(!result.value.is_empty());
        assert!(
            result
                .value
                .iter()
                .all(|item| item.code == DiagnosticCode::ParseError)
        );
        assert!(!result.metadata_models_complete);
        assert!(!session.summary().main_models_complete);
        assert!(!session.summary().descriptor_models_complete);
        assert!(session.summary().source_only > 0);
        assert!(
            !session
                .module_document_symbols(&module, session.revision(), &QueryCancellation::default())
                .unwrap()
                .value
                .is_empty()
        );
        let executions = session
            .database
            .salsa_event_report()
            .unwrap()
            .into_iter()
            .filter(|row| row.execute > 0)
            .map(|row| row.name)
            .collect::<Vec<_>>();
        assert!(
            executions
                .iter()
                .all(|name| !name.contains("load_configuration")
                    && !name.contains("module_metadata")
                    && !name.contains("resolve_metadata_object")),
            "{executions:?}"
        );
    }
}

/// Default constructors still reject uncomposed artifacts; explicit syntax cannot invoke semantic preparation.
#[test]
fn canonical_guards_and_cf_defaults_are_preserved_without_syntax_downgrades() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::GOOD, false);
        let source = root.join("src");
        let original = AnalysisSession::open(&source);
        if kind == ProjectType::Extension {
            assert!(
                matches!(original, Err(SessionError::UnsupportedProjectType(actual)) if actual == kind)
            );
        } else {
            assert_eq!(
                original.unwrap().scope(),
                AnalysisScope::canonical_for(kind)
            );
        }
        let mut session = AnalysisSession::from_reader_with_scope(
            SourceReader::open(&source).unwrap(),
            SessionOptions::default(),
            AnalysisScope::LocalSyntax,
        )
        .unwrap();
        let revision = session.revision();
        assert_eq!(
            session.load_semantic_inputs(revision, &InputRequest::default()),
            Err(SessionRequestError::ContextRequired)
        );
        assert_eq!(
            session.prepare_completion_metadata(
                &module,
                revision,
                0,
                &InputRequest::default(),
                &QueryCancellation::default()
            ),
            Err(QueryError::Request(SessionRequestError::ContextRequired))
        );
        assert_eq!(
            session.module_diagnostics(
                &module,
                revision,
                &DiagnosticsConfig::default(),
                &QueryCancellation::default()
            ),
            Err(QueryError::Request(SessionRequestError::ContextRequired))
        );
        assert_eq!(
            session.module_completions(
                &module,
                revision,
                0,
                crate::Locale::Ru,
                &QueryCancellation::default()
            ),
            Err(QueryError::Request(SessionRequestError::ContextRequired))
        );
        assert_eq!(session.revision(), revision);
        assert_eq!(session.resources().total_source_bytes, 0);
        assert_eq!(session.module(&module).unwrap().state, ModuleState::Pending);
    }
}

/// Saved invalidation never replaces editor authority; close reads the actual saved bytes.
#[test]
fn local_editor_authority_and_failure_states_use_the_existing_shared_lifecycle() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::GOOD, false);
        let source = root.join("src");
        let mut session = AnalysisSession::from_reader_with_scope(
            SourceReader::open(&source).unwrap(),
            SessionOptions::default(),
            AnalysisScope::LocalSyntax,
        )
        .unwrap();
        let request = InputRequest::default();
        session
            .apply_module_overlay(&module, session.revision(), 1, fixture::BAD, &request)
            .unwrap();
        let revision = session.revision();
        assert!(
            session
                .invalidate_saved_inputs(std::slice::from_ref(&module), revision)
                .unwrap()
                .is_empty()
        );
        assert_eq!(session.revision(), revision);
        assert!(matches!(
            session.load_module_saved(&module, revision, &request),
            Err(SessionRequestError::OverlayActive {
                document_version: 1
            })
        ));
        let token = QueryCancellation::default();
        assert!(
            !session
                .module_syntax_diagnostics(&module, revision, &DiagnosticsConfig::default(), &token)
                .unwrap()
                .value
                .is_empty()
        );
        session
            .close_module_overlay(&module, revision, &request)
            .unwrap();
        assert!(
            session
                .module_syntax_diagnostics(
                    &module,
                    session.revision(),
                    &DiagnosticsConfig::default(),
                    &token
                )
                .unwrap()
                .value
                .is_empty()
        );
        fs::remove_file(source.join(module.as_path())).unwrap();
        session
            .invalidate_saved_inputs(std::slice::from_ref(&module), session.revision())
            .unwrap();
        session
            .load_module_saved(&module, session.revision(), &request)
            .unwrap();
        assert!(matches!(
            session.module_syntax_diagnostics(
                &module,
                session.revision(),
                &DiagnosticsConfig::default(),
                &token
            ),
            Err(QueryError::ModuleUnavailable { .. })
        ));
    }
}

/// Source-only queries share cooperative cancellation and byte reservation failures.
#[test]
fn local_syntax_cancellation_and_budget_rejection_never_return_empty_success() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::GOOD, false);
        let mut session = AnalysisSession::from_reader_with_scope(
            SourceReader::open(&root.join("src")).unwrap(),
            SessionOptions::default(),
            AnalysisScope::LocalSyntax,
        )
        .unwrap();
        session
            .load_module_saved(&module, session.revision(), &InputRequest::default())
            .unwrap();
        let revision = session.revision();
        let cancelled = QueryCancellation::default();
        cancelled.cancel();
        assert_eq!(
            session.module_syntax_diagnostics(
                &module,
                revision,
                &DiagnosticsConfig::default(),
                &cancelled
            ),
            Err(QueryError::Cancelled)
        );
        assert!(matches!(
            session.module_document_symbols(&module, revision, &cancelled),
            Err(QueryError::Cancelled)
        ));
        assert_eq!(session.revision(), revision);
        assert!(
            session
                .module_syntax_diagnostics(
                    &module,
                    revision,
                    &DiagnosticsConfig::default(),
                    &QueryCancellation::default()
                )
                .unwrap()
                .value
                .is_empty()
        );
        session
            .apply_module_overlay(
                &module,
                revision,
                1,
                fixture::BAD,
                &InputRequest {
                    max_document_bytes: 1,
                    ..InputRequest::default()
                },
            )
            .unwrap();
        assert_eq!(session.resources().module_text_bytes, 0);
        assert!(matches!(
            session.module(&module).unwrap().state,
            ModuleState::Unavailable(_)
        ));
        assert!(matches!(
            session.module_syntax_diagnostics(
                &module,
                session.revision(),
                &DiagnosticsConfig::default(),
                &QueryCancellation::default()
            ),
            Err(QueryError::ModuleUnavailable { .. })
        ));
    }
}
