//! Batch coverage, disk immutability and exact saved IDE parity use actual shared queries.

mod metadata;

use std::fs;

use crate::{Locale, QueryCancellation};

use super::*;

#[path = "../../tests/support/project.rs"]
mod support;

/// Reuse the library's one test-directory module rather than compiling it twice.
fn project(
    body: &str,
    binary: bool,
) -> (
    crate::session::tests::support::TestDirectory,
    std::path::PathBuf,
) {
    let directory = crate::session::tests::support::TestDirectory::new();
    let root = support::project(&directory.0, body, binary);
    (directory, root)
}

/// A minimal complete configuration checks byte-identical disk input through both consumers.
#[test]
fn saved_batch_and_resident_syntax_diagnostics_match_in_both_locales() {
    let (_directory, root) = project("\u{feff}Процедура Ошибка(\r\n", false);
    let source = root.join("src");
    let path = SourcePath::parse("CommonModules/Tools/Ext/Module.bsl").unwrap();
    let before = fs::read(source.join(path.as_path())).unwrap();
    let mut session = AnalysisSession::open(&source).unwrap();
    let owners = session.owner_paths().cloned().collect::<Vec<_>>();
    for owner in owners {
        session
            .load_saved(&owner, session.revision(), &InputRequest::default())
            .unwrap();
    }
    session
        .load_module_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let mut json = Vec::new();
    for locale in [Locale::Ru, Locale::En] {
        let config = DiagnosticsConfig {
            locale,
            only_enabled: Some(vec![DiagnosticCode::ParseError]),
            ..DiagnosticsConfig::default()
        };
        let expected = session
            .module_diagnostics(
                &path,
                session.revision(),
                &config,
                &QueryCancellation::default(),
            )
            .unwrap();
        let report = check_syntax(
            SourceReader::open(&source).unwrap(),
            &config,
            &BatchOptions::default(),
        )
        .unwrap();
        assert_eq!(report.status, BatchStatus::Findings);
        assert!(report.coverage.syntax_complete && report.coverage.metadata_models_complete);
        assert_eq!(report.files[0].source_sha256, Some(expected.source_sha256));
        assert_eq!(report.files[0].findings.len(), expected.value.len());
        for (actual, expected) in report.files[0].findings.iter().zip(expected.value) {
            assert_eq!(actual.code, expected.code.as_str());
            assert_eq!(actual.start_byte, u32::from(expected.range.start()));
            assert_eq!(actual.end_byte, u32::from(expected.range.end()));
            assert_eq!(actual.message, expected.message);
        }
        json.push(serde_json::to_value(report).unwrap());
    }
    assert_eq!(json[0], json[1]);
    assert_eq!(fs::read(source.join(path.as_path())).unwrap(), before);
    session
        .release_module_saved(&path, session.revision())
        .unwrap();
    assert_eq!(session.module_summary().loaded_bytes, 0);
    assert_eq!(session.module(&path).unwrap().state, ModuleState::Pending);
    assert!(matches!(
        session.module_diagnostics(
            &path,
            session.revision(),
            &DiagnosticsConfig::default(),
            &QueryCancellation::default()
        ),
        Err(QueryError::ModuleUnavailable { .. })
    ));
}

/// The canonical enclosing owner binds an inline command's separate body for both consumers.
#[test]
fn inline_command_syntax_matches_the_resident_session() {
    let directory = crate::session::tests::support::TestDirectory::new();
    let root = support::inline_command_project(&directory.0, "Процедура Ошибка(\r\n");
    let source = root.join("src-with-command");
    let path = SourcePath::parse("Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl").unwrap();
    let mut session = AnalysisSession::open(&source).unwrap();
    assert_eq!(
        session.module_owner(&path).unwrap().as_str(),
        "Catalogs/Sample/object.yaml"
    );
    session
        .load_module_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::ParseError]),
        ..DiagnosticsConfig::default()
    };
    let expected = session
        .module_diagnostics(
            &path,
            session.revision(),
            &config,
            &QueryCancellation::default(),
        )
        .unwrap();
    let report = check_syntax(
        SourceReader::open(&source).unwrap(),
        &config,
        &BatchOptions::default(),
    )
    .unwrap();
    assert_eq!(report.coverage.checked_modules, 2);
    assert!(report.coverage.syntax_complete);
    assert!(report.coverage.metadata_models_complete);
    let actual = report
        .files
        .iter()
        .find(|file| file.source == path)
        .unwrap();
    assert_eq!(actual.source_sha256, Some(expected.source_sha256));
    assert!(!actual.findings.is_empty());
    assert_eq!(actual.findings.len(), expected.value.len());
}

/// Zero deadlines and pre-cancelled callers report the unvisited declarations rather than no errors.
#[test]
fn deadline_cancellation_binary_and_byte_limit_remain_incomplete() {
    let (_directory, root) = project("Процедура Проверка()\nКонецПроцедуры\n", false);
    let source = root.join("src");
    let options = BatchOptions {
        timeout: Duration::ZERO,
        ..BatchOptions::default()
    };
    let report = check_syntax(
        SourceReader::open(&source).unwrap(),
        &DiagnosticsConfig::default(),
        &options,
    )
    .unwrap();
    assert_eq!(report.status, BatchStatus::Incomplete);
    assert_eq!(report.stop, Some(StopReason::DeadlineExceeded));
    assert_eq!(report.coverage.checked_modules, 0);
    assert_eq!(report.coverage.unchecked_modules, 1);
    let options = BatchOptions::default();
    options.cancellation.cancel();
    let report = check_syntax(
        SourceReader::open(&source).unwrap(),
        &DiagnosticsConfig::default(),
        &options,
    )
    .unwrap();
    assert_eq!(report.stop, Some(StopReason::Cancelled));
    let options = BatchOptions {
        max_document_bytes: 1,
        ..BatchOptions::default()
    };
    let report = check_syntax(
        SourceReader::open(&source).unwrap(),
        &DiagnosticsConfig::default(),
        &options,
    )
    .unwrap();
    assert_eq!(report.status, BatchStatus::Incomplete);
    assert_eq!(report.coverage.unavailable_modules, 1);
    let options = BatchOptions {
        max_retained_source_bytes: 0,
        ..BatchOptions::default()
    };
    let report = check_syntax(
        SourceReader::open(&source).unwrap(),
        &DiagnosticsConfig::default(),
        &options,
    )
    .unwrap();
    assert_eq!(report.status, BatchStatus::Incomplete);
    assert_eq!(report.resources.total_source_bytes, 0);
    assert_eq!(report.resources.max_source_bytes, 0);
    assert_eq!(report.coverage.unavailable_modules, 1);
    assert!(
        report
            .metadata_issues
            .iter()
            .all(|issue| issue.code == "analysis_source_budget_exceeded")
    );
    let (_directory, root) = project("binary body", true);
    let report = check_syntax(
        SourceReader::open(&root.join("src")).unwrap(),
        &DiagnosticsConfig::default(),
        &BatchOptions::default(),
    )
    .unwrap();
    assert_eq!(report.status, BatchStatus::Incomplete);
    assert_eq!(report.coverage.binary_modules, 1);
    assert_eq!(report.coverage.checked_modules, 0);
}

/// A transferred reader is not reopened, and releasing saved input cannot discard an editor draft.
#[test]
fn adopted_reader_and_editor_owned_text_keep_their_ownership_contract() {
    let (_directory, root) = project("Процедура Проверка()\nКонецПроцедуры\n", false);
    let source = root.join("src");
    let reader = SourceReader::open(&source).unwrap();
    fs::remove_file(source.join("source.json")).unwrap();
    let mut session = AnalysisSession::from_reader(reader, SessionOptions::default()).unwrap();
    let path = SourcePath::parse("CommonModules/Tools/Ext/Module.bsl").unwrap();
    session
        .apply_module_overlay(
            &path,
            session.revision(),
            1,
            "draft",
            &InputRequest::default(),
        )
        .unwrap();
    let revision = session.revision();
    assert!(matches!(
        session.release_module_saved(&path, revision),
        Err(SessionRequestError::OverlayActive {
            document_version: 1
        })
    ));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.module_summary().loaded_bytes, 5);
}
