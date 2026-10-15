//! Current diagnostics reuse captured YAML coordinates and never recover stale models from disk.

use std::fs;

use eska::{
    analysis_core::InputRequest,
    project_core::source_format::{Child, Section, SourceReader, TextPosition, XmlFragment},
};

use super::*;
use crate::session::tests::{fixture, owner};

/// Count Unicode independently of the stored source map; fixtures use LF or CRLF.
fn position(text: &str, byte: usize) -> TextPosition {
    let prefix = &text[..byte];
    let line_start = prefix.rfind('\n').map_or(0, |at| at + 1);
    TextPosition {
        line: u32::try_from(prefix.bytes().filter(|&byte| byte == b'\n').count()).unwrap(),
        character: u32::try_from(prefix[line_start..].encode_utf16().count()).unwrap(),
    }
}

/// Both endpoints refer to the same verbatim text, including a non-BMP prefix.
fn coordinates(finding: &MetadataDiagnostic, text: &str) {
    assert_eq!(
        finding.range.start,
        position(text, finding.byte_range.start)
    );
    assert_eq!(finding.range.end, position(text, finding.byte_range.end));
}

/// Adapter findings use the exact editor hash/range even after the saved owner disappears.
#[test]
fn adapter_diagnostics_follow_editor_authority_and_clear_after_correction() {
    let (_directory, root) = fixture();
    let path = owner("CommonModules/Tools/object.yaml");
    let saved = fs::read_to_string(root.join(path.as_path())).unwrap();
    let draft = format!(
        "# Я😀 current diagnostic\r\n{}",
        crate::test_property::server_value(&saved, "Future")
    );
    let mut session = AnalysisSession::open(&root).unwrap();
    let cancellation = QueryCancellation::default();
    assert_eq!(
        session.metadata_diagnostics(&path, session.revision(), &cancellation),
        Err(MetadataDiagnosticsError::Pending)
    );
    session
        .apply_overlay(
            &path,
            session.revision(),
            7,
            &draft,
            &InputRequest::default(),
        )
        .unwrap();
    let report = session
        .metadata_diagnostics(&path, session.revision(), &cancellation)
        .unwrap();
    assert_eq!(
        report.origin,
        InputOrigin::Overlay {
            document_version: 7
        }
    );
    assert!(
        report.input_complete && !report.main_model_complete && !report.descriptor_models_complete
    );
    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert_eq!(
        finding.issue,
        MetadataDiagnosticIssue::Adapter(AdapterIssue::InvalidProperty {
            name: "Server".into()
        })
    );
    assert!(draft[finding.byte_range.clone()].contains("Future"));
    coordinates(finding, &draft);
    let revision = session.revision();
    let resources = session.resources();
    fs::remove_file(root.join(path.as_path())).unwrap();
    assert_eq!(
        session
            .metadata_diagnostics(&path, revision, &cancellation)
            .unwrap(),
        report
    );
    assert_eq!(session.resources(), resources);
    assert_eq!(session.revision(), revision);
    session
        .apply_overlay(&path, revision, 8, &saved, &InputRequest::default())
        .unwrap();
    assert!(
        session
            .metadata_diagnostics(&path, session.revision(), &cancellation)
            .unwrap()
            .findings
            .is_empty()
    );
    assert!(matches!(
        session.metadata_diagnostics(&path, revision, &cancellation),
        Err(MetadataDiagnosticsError::Request(_))
    ));
    session
        .apply_overlay(
            &path,
            session.revision(),
            9,
            "broken: [\n",
            &InputRequest::default(),
        )
        .unwrap();
    assert!(matches!(
        session.metadata_diagnostics(&path, session.revision(), &cancellation),
        Err(MetadataDiagnosticsError::Unavailable(_))
    ));
    fs::write(root.join(path.as_path()), saved).unwrap();
    session
        .close_overlay(&path, session.revision(), &InputRequest::default())
        .unwrap();
    cancellation.cancel();
    assert_eq!(
        session.metadata_diagnostics(&path, session.revision(), &cancellation),
        Err(MetadataDiagnosticsError::Cancelled)
    );
    assert!(
        session
            .metadata_diagnostics(&path, session.revision(), &QueryCancellation::default())
            .unwrap()
            .findings
            .is_empty()
    );
}

/// Unknown preserved children get their own captured spans, without a duplicate incomplete-adapter finding.
#[test]
fn recoverable_parse_issues_have_current_coordinates_without_duplicate_adapter_findings() {
    let (_directory, root) = fixture();
    let path = owner("CommonModules/Tools/object.yaml");
    let reader = SourceReader::open(&root).unwrap();
    let mut source = reader.read_owner(&path).unwrap().source;
    source
        .document
        .descriptor
        .object
        .sections
        .push(Section::Children(vec![Child::Preserved {
            fragment: XmlFragment {
                xml: "<future:Child/>".into(),
                namespaces: vec![eska::project_core::source_format::Namespace {
                    prefix: Some("future".into()),
                    uri: "urn:future".into(),
                }],
            },
        }]));
    let draft = format!(
        "# Я😀 recoverable child\r\n{}",
        serde_json::to_string(&source).unwrap()
    );
    let mut session = AnalysisSession::open(&root).unwrap();
    session
        .apply_overlay(
            &path,
            session.revision(),
            7,
            &draft,
            &InputRequest::default(),
        )
        .unwrap();
    let report = session
        .metadata_diagnostics(&path, session.revision(), &QueryCancellation::default())
        .unwrap();
    assert_eq!(report.findings.len(), 1);
    assert!(!report.input_complete && !report.main_model_complete);
    let finding = &report.findings[0];
    assert!(matches!(
        finding.issue,
        MetadataDiagnosticIssue::Parse(ParseIssue::UnknownKind { .. })
    ));
    assert!(draft[finding.byte_range.clone()].contains("future:Child"));
    coordinates(finding, &draft);
}

/// A nested issue belongs to its native descriptor while the top-level canonical model remains available.
#[test]
fn nested_parse_issue_preserves_native_identity_and_does_not_invalidate_ready_main_model() {
    let (_directory, root) = fixture();
    let path = owner("Catalogs/Sample/object.yaml");
    let reader = SourceReader::open(&root).unwrap();
    let mut source = reader.read_owner(&path).unwrap().source;
    let nested = source.nested.first_mut().unwrap();
    let native = nested.path.clone();
    nested
        .descriptor
        .object
        .sections
        .push(Section::Children(vec![Child::Preserved {
            fragment: XmlFragment {
                xml: "<future:NestedChild/>".into(),
                namespaces: vec![eska::project_core::source_format::Namespace {
                    prefix: Some("future".into()),
                    uri: "urn:future".into(),
                }],
            },
        }]));
    let draft = format!(
        "# Я😀 nested issue\r\n{}",
        serde_json::to_string(&source).unwrap()
    );
    let mut session = AnalysisSession::open(&root).unwrap();
    session
        .apply_overlay(
            &path,
            session.revision(),
            7,
            &draft,
            &InputRequest::default(),
        )
        .unwrap();
    let report = session
        .metadata_diagnostics(&path, session.revision(), &QueryCancellation::default())
        .unwrap();
    assert_eq!(report.findings.len(), 1);
    assert!(
        report.main_model_complete && !report.input_complete && !report.descriptor_models_complete
    );
    let finding = &report.findings[0];
    assert_eq!(finding.native_path, native);
    assert!(matches!(
        finding.issue,
        MetadataDiagnosticIssue::Parse(ParseIssue::UnknownKind { .. })
    ));
    assert!(draft[finding.byte_range.clone()].contains("NestedChild"));
    coordinates(finding, &draft);
}

/// Unsupported projection is explicit at the actual root span rather than a fabricated empty clean report.
#[test]
fn unsupported_metadata_kind_preserves_its_real_root_location() {
    let (_directory, root) = crate::session::tests::fixture_with_unmodeled_family();
    let path = owner("SessionParameters/Reader/object.yaml");
    let mut session = AnalysisSession::open(&root).unwrap();
    session
        .load_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let result = session
        .metadata_diagnostics(&path, session.revision(), &QueryCancellation::default())
        .unwrap();
    assert!(result.input_complete && !result.main_model_complete);
    assert_eq!(result.findings.len(), 1);
    assert_eq!(
        result.findings[0].issue,
        MetadataDiagnosticIssue::UnsupportedKind(MetadataKind::SessionParameter)
    );
    let saved = fs::read_to_string(root.join(path.as_path())).unwrap();
    assert!(saved[result.findings[0].byte_range.clone()].contains("Reader"));
}
