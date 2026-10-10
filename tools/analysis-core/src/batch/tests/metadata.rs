//! Batch metadata findings retain the resident query's exact identities and captured coordinates.

use eska::project_core::source_format::{Child, Namespace, Section, XmlFragment};

use super::*;
use crate::{MetadataDiagnosticIssue, session::tests::fixture};

/// Main, nested and unsupported findings use the same current saved query without claiming full coverage.
#[test]
fn batch_metadata_findings_match_resident_queries() {
    for (path, nested) in [
        ("CommonModules/Tools/object.yaml", false),
        ("Catalogs/Sample/object.yaml", true),
        ("Roles/Reader/object.yaml", false),
    ] {
        let (_directory, source) = fixture();
        let path = SourcePath::parse(path).unwrap();
        let file = source.join(path.as_path());
        let before = fs::read_to_string(&file).unwrap();
        let text = if nested {
            let mut owner = SourceReader::open(&source)
                .unwrap()
                .read_owner(&path)
                .unwrap()
                .source;
            owner.nested[0]
                .descriptor
                .object
                .sections
                .push(Section::Children(vec![Child::Preserved {
                    fragment: XmlFragment {
                        xml: "<future:NestedChild/>".into(),
                        namespaces: vec![Namespace {
                            prefix: Some("future".into()),
                            uri: "urn:future".into(),
                        }],
                    },
                }]));
            serde_json::to_string(&owner).unwrap()
        } else if path.as_str().starts_with("CommonModules/") {
            before.replace("<Server>true</Server>", "<Server>Future</Server>")
        } else {
            before
        };
        let text = format!("# Я😀 batch metadata\r\n{text}");
        fs::write(&file, &text).unwrap();
        let mut session = AnalysisSession::open(&source).unwrap();
        session
            .load_saved(&path, session.revision(), &InputRequest::default())
            .unwrap();
        let expected = session
            .metadata_diagnostics(&path, session.revision(), &QueryCancellation::default())
            .unwrap();
        assert_eq!(expected.findings.len(), 1);
        if nested {
            assert!(expected.main_model_complete && !expected.descriptor_models_complete);
            assert_ne!(expected.findings[0].native_path, path);
            assert!(matches!(
                expected.findings[0].issue,
                MetadataDiagnosticIssue::Parse(_)
            ));
        }
        let report = check_syntax(
            SourceReader::open(&source).unwrap(),
            &DiagnosticsConfig::default(),
            &BatchOptions::default(),
        )
        .unwrap();
        assert_eq!(report.status, BatchStatus::Incomplete);
        let issue = report
            .metadata_issues
            .iter()
            .find(|issue| issue.source == path)
            .unwrap();
        assert_eq!(issue.source_sha256, Some(expected.source_sha256));
        assert_eq!(issue.diagnostics.as_ref().unwrap(), &expected.findings);
        assert_eq!(fs::read_to_string(file).unwrap(), text);
    }
}

/// Stopping a detailed walk keeps the existing incomplete owner and never manufactures an empty clean result.
#[test]
fn cancelled_metadata_report_preserves_incomplete_owner_without_findings() {
    let (_directory, source) = fixture();
    let path = SourcePath::parse("Roles/Reader/object.yaml").unwrap();
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let cancellation = QueryCancellation::default();
    cancellation.cancel();
    let report = super::super::report::finish(
        &session,
        "manifest".into(),
        vec![],
        Some(StopReason::Cancelled),
        &DiagnosticsConfig::default(),
        &cancellation,
        BatchProfile::Syntax,
    )
    .unwrap();
    assert_eq!(report.status, BatchStatus::Incomplete);
    let issue = report
        .metadata_issues
        .iter()
        .find(|issue| issue.source == path)
        .unwrap();
    assert_eq!(issue.code, "metadata_projection_unsupported");
    assert!(issue.source_sha256.is_some() && issue.diagnostics.is_none());
    assert_eq!(report.stop, Some(StopReason::Cancelled));
}
