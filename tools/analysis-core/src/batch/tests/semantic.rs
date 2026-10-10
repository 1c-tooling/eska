//! Configured diagnostics query the same retained saved bytes as a resident IDE consumer.

use super::*;

#[path = "../../../tests/support/semantic_project.rs"]
mod fixture;

const BODY: &str = "\u{feff}Процедура Проверка()\r\n\tЗначение = 1;\r\n\tЗначение = Значение;\r\nКонецПроцедуры\r\n";

/// A non-syntax rule and its exact localized findings agree with the shared resident session.
#[test]
fn semantic_batch_matches_resident_rules_and_preserves_saved_bytes() {
    let (_directory, root) = project(BODY, false);
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
    let mut reports = vec![];
    for locale in [Locale::Ru, Locale::En] {
        let config = DiagnosticsConfig {
            locale,
            only_enabled: Some(vec![DiagnosticCode::ParseError, DiagnosticCode::SelfAssign]),
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
        let actual = check_semantic(
            SourceReader::open(&source).unwrap(),
            &config,
            &BatchOptions::default(),
        )
        .unwrap();
        assert_eq!(actual.profile, BatchProfile::Semantic);
        assert!(actual.profile_enabled);
        assert_eq!(actual.status, BatchStatus::Findings);
        assert_eq!(actual.coverage.semantic_profile_complete, Some(true));
        assert!(actual.coverage.syntax_complete);
        assert_eq!(actual.resources.module_text_bytes, before.len() as u64);
        let findings = expected
            .value
            .iter()
            .map(|diagnostic| report::finding(&path, diagnostic, BODY).unwrap())
            .collect::<Vec<_>>();
        assert!(!findings.is_empty());
        assert_eq!(actual.files[0].findings, findings);
        assert_eq!(actual.files[0].source_sha256, Some(expected.source_sha256));
        reports.push(serde_json::to_value(actual).unwrap());
        let syntax = check_syntax(
            SourceReader::open(&source).unwrap(),
            &config,
            &BatchOptions::default(),
        )
        .unwrap();
        assert_eq!(syntax.status, BatchStatus::Passed);
        assert_eq!(syntax.coverage.semantic_profile_complete, None);
        assert!(syntax.files[0].findings.is_empty());
    }
    assert_eq!(reports[0], reports[1]);
    assert_eq!(fs::read(source.join(path.as_path())).unwrap(), before);
}

/// The caller sorts first, so interleaved load/query/release would miss its exported dependency.
#[test]
fn semantic_batch_loads_all_declared_bodies_before_cross_module_queries() {
    let directory = crate::session::tests::support::TestDirectory::new();
    let root = support::project(
        &directory.0,
        "Procedure Run()\n ZDependency.Available();\n ZDependency.Missing();\nEndProcedure\n",
        false,
    );
    fixture::add_dependency(
        &root,
        &directory.0.join("native"),
        "Procedure Available() Export\nEndProcedure\n",
    );
    let source = root.join("src-dependent");
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::UnresolvedMethodCall]),
        ..DiagnosticsConfig::default()
    };
    let actual = check_semantic(
        SourceReader::open(&source).unwrap(),
        &config,
        &BatchOptions::default(),
    )
    .unwrap();
    assert_eq!(actual.status, BatchStatus::Findings);
    assert_eq!(actual.coverage.semantic_profile_complete, Some(true));
    assert!(!actual.coverage.syntax_complete);
    assert_eq!(actual.coverage.checked_modules, 2);
    assert_eq!(
        actual.files[0].source.as_path(),
        std::path::Path::new("CommonModules/Tools/Ext/Module.bsl")
    );
    assert_eq!(actual.files[0].findings.len(), 1);
    assert!(actual.files[0].findings[0].message.contains("Missing"));
    assert!(!actual.files[0].findings[0].message.contains("Available"));

    fs::remove_file(source.join("CommonModules/ZDependency/Ext/Module.bsl")).unwrap();
    let missing = check_semantic(
        SourceReader::open(&source).unwrap(),
        &config,
        &BatchOptions::default(),
    )
    .unwrap();
    assert_eq!(missing.status, BatchStatus::Incomplete);
    assert_eq!(missing.coverage.semantic_profile_complete, Some(false));
    assert_eq!(missing.coverage.unavailable_modules, 1);
    assert!(missing.files[0].findings.is_empty());
}

/// A parsed but invalid canonical model cannot certify semantics even when its BSL was checked.
#[test]
fn invalid_metadata_keeps_semantic_coverage_incomplete() {
    let (_directory, root) = project(BODY, false);
    let source = root.join("src");
    let path = source.join("CommonModules/Tools/object.yaml");
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains("<Server>true</Server>"));
    fs::write(
        &path,
        saved.replacen("<Server>true</Server>", "<Server>Future</Server>", 1),
    )
    .unwrap();
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::SelfAssign]),
        ..DiagnosticsConfig::default()
    };
    let actual = check_semantic(
        SourceReader::open(&source).unwrap(),
        &config,
        &BatchOptions::default(),
    )
    .unwrap();
    assert_eq!(actual.status, BatchStatus::Incomplete);
    assert!(actual.coverage.metadata_inputs_complete);
    assert!(!actual.coverage.metadata_models_complete);
    assert_eq!(actual.coverage.checked_modules, 1);
    assert_eq!(actual.coverage.semantic_profile_complete, Some(false));
    assert_eq!(
        actual.metadata_issues[0].code,
        "metadata_projection_invalid"
    );
}

/// Binary input, current quota failures, no enabled rules and cancellation cannot pass semantics.
#[test]
fn incomplete_or_disabled_semantic_input_never_passes() {
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::SelfAssign]),
        ..DiagnosticsConfig::default()
    };
    let (_directory, root) = project(BODY, true);
    let binary = check_semantic(
        SourceReader::open(&root.join("src")).unwrap(),
        &config,
        &BatchOptions::default(),
    )
    .unwrap();
    assert_eq!(binary.status, BatchStatus::Incomplete);
    assert_eq!(binary.coverage.binary_modules, 1);
    assert_eq!(binary.coverage.semantic_profile_complete, Some(false));

    let (_directory, root) = project(BODY, false);
    let quota = check_semantic(
        SourceReader::open(&root.join("src")).unwrap(),
        &config,
        &BatchOptions {
            max_retained_source_bytes: 1,
            ..BatchOptions::default()
        },
    )
    .unwrap();
    assert_eq!(quota.status, BatchStatus::Incomplete);
    assert_eq!(quota.coverage.semantic_profile_complete, Some(false));
    assert_eq!(quota.resources.total_source_bytes, 0);
    assert!(
        matches!(&quota.files[0].state, FileState::Unavailable { code, .. } if code == "analysis_source_budget_exceeded")
    );

    let disabled = check_semantic(
        SourceReader::open(&root.join("src")).unwrap(),
        &DiagnosticsConfig {
            only_enabled: Some(vec![]),
            ..DiagnosticsConfig::default()
        },
        &BatchOptions::default(),
    )
    .unwrap();
    assert!(!disabled.profile_enabled);
    assert_eq!(disabled.status, BatchStatus::Incomplete);
    assert_eq!(disabled.coverage.semantic_profile_complete, Some(false));
    assert_eq!(disabled.coverage.unchecked_modules, 1);
    assert_eq!(disabled.resources.module_text_bytes, 0);

    let stopped = check_semantic(
        SourceReader::open(&root.join("src")).unwrap(),
        &config,
        &BatchOptions {
            timeout: Duration::ZERO,
            ..BatchOptions::default()
        },
    )
    .unwrap();
    assert_eq!(stopped.status, BatchStatus::Incomplete);
    assert_eq!(stopped.stop, Some(StopReason::DeadlineExceeded));
    assert_eq!(stopped.coverage.semantic_profile_complete, Some(false));
    assert_eq!(stopped.resources.total_source_bytes, 0);
}
