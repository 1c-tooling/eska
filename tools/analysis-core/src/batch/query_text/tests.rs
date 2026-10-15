//! Saved IDE and batch inputs use one native query API and explicit stopped/disabled source coverage.

use std::{fs, time::Duration};

use crate::{
    Locale, SessionOptions, query_creation_fixture as fixture, test_support::TestDirectory,
};
use eska::project_core::source_format::{SourceReader, import_designer_queries};

use super::*;
use crate::batch::{BatchOptions, StopReason, check_syntax};

/// A complete owned native source supplies real query declarations for each supported artifact type.
fn source(kind: &str) -> (TestDirectory, std::path::PathBuf, SourcePath) {
    let directory = TestDirectory::new();
    let native = fixture::native(&directory.0, kind);
    let root = directory.0.join("source");
    import_designer_queries(&native.input, &root).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let resource = SourcePath::parse(native.schema.to_str().unwrap()).unwrap();
    let path = reader
        .query_paths()
        .find(|path| reader.declared_query(path).unwrap().native_resource == &resource)
        .unwrap()
        .clone();
    (directory, root, path)
}

/// Native codes/hash/ranges agree for all four artifact types without metadata resolution or source mutation.
#[test]
fn saved_query_batch_and_resident_host_are_exact_and_locale_invariant() {
    for kind in fixture::TYPES {
        let (_directory, root, path) = source(kind);
        let text = "\u{feff}// Я😀 current query\r\nSELECT Data. FROM Catalog.Missing AS Data\r\n";
        fs::write(root.join(path.as_path()), text).unwrap();
        let before = fixture::snapshot(&root);
        let mut machine = Vec::new();
        for locale in [Locale::Ru, Locale::En] {
            let config = DiagnosticsConfig {
                locale,
                ..DiagnosticsConfig::default()
            };
            let reader = SourceReader::open(&root).unwrap();
            let scope = BatchProfile::Syntax.session_scope(&reader).unwrap();
            let mut session =
                AnalysisSession::from_reader_with_scope(reader, SessionOptions::default(), scope)
                    .unwrap();
            session
                .load_query_saved(&path, session.revision(), &InputRequest::default())
                .unwrap();
            let expected = session
                .query_syntax_diagnostics(
                    &path,
                    session.revision(),
                    &config,
                    &InputRequest::default(),
                )
                .unwrap();
            let batch = check_syntax(
                SourceReader::open(&root).unwrap(),
                &config,
                &BatchOptions::default(),
            )
            .unwrap();
            assert_eq!(batch.query_coverage.queries, 2);
            assert!(batch.query_coverage.syntax_complete);
            assert!(
                !batch.query_coverage.metadata_complete && !batch.query_coverage.consumer_complete
            );
            assert_eq!(batch.resources.query_text_bytes, 0);
            let file = batch
                .query_files
                .iter()
                .find(|file| file.source == path)
                .unwrap();
            assert_eq!(
                file.source_sha256.as_ref(),
                Some(&expected.result.source_sha256)
            );
            assert_eq!(file.findings.len(), expected.result.diagnostics.len());
            for (finding, diagnostic) in file.findings.iter().zip(expected.result.diagnostics) {
                assert_eq!(finding.finding.code, diagnostic.code.as_str());
                assert_eq!(
                    finding.finding.start_byte,
                    u32::from(diagnostic.range.start())
                );
                assert_eq!(finding.finding.end_byte, u32::from(diagnostic.range.end()));
                assert_eq!(finding.finding.message, diagnostic.message);
            }
            machine.push(serde_json::to_value(&batch.query_files).unwrap());
        }
        assert_eq!(machine[0], machine[1]);
        assert_eq!(fixture::snapshot(&root), before);
    }
}

/// A source quota, zero deadline, cancelled request or disabled rule never becomes checked clean input.
#[test]
fn query_batch_quota_cancel_deadline_and_rule_disabling_keep_unchecked_counts_honest() {
    let (_directory, root, _) = source("configuration");
    let options = BatchOptions {
        max_retained_source_bytes: 0,
        ..BatchOptions::default()
    };
    let report = check_syntax(
        SourceReader::open(&root).unwrap(),
        &DiagnosticsConfig::default(),
        &options,
    )
    .unwrap();
    assert_eq!(report.status, BatchStatus::Incomplete);
    assert_eq!(report.query_coverage.unavailable, 2);
    assert_eq!(report.resources.query_text_bytes, 0);
    assert!(!report.query_coverage.syntax_complete);
    let stopped = BatchOptions {
        timeout: Duration::ZERO,
        ..BatchOptions::default()
    };
    let report = check_syntax(
        SourceReader::open(&root).unwrap(),
        &DiagnosticsConfig::default(),
        &stopped,
    )
    .unwrap();
    assert_eq!(report.stop, Some(StopReason::DeadlineExceeded));
    assert_eq!(report.query_coverage.unchecked, 2);
    let cancelled = BatchOptions::default();
    cancelled.cancellation.cancel();
    let report = check_syntax(
        SourceReader::open(&root).unwrap(),
        &DiagnosticsConfig::default(),
        &cancelled,
    )
    .unwrap();
    assert_eq!(report.stop, Some(StopReason::Cancelled));
    assert_eq!(report.query_coverage.unchecked, 2);
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::ParseError]),
        ..DiagnosticsConfig::default()
    };
    let report = check_syntax(
        SourceReader::open(&root).unwrap(),
        &config,
        &BatchOptions::default(),
    )
    .unwrap();
    assert_eq!(report.status, BatchStatus::Incomplete);
    assert_eq!(report.query_coverage.unchecked, 2);
    assert!(!report.query_coverage.syntax_complete);
}
