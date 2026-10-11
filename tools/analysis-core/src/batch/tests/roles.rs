//! Role dependencies are the same saved inputs for independent syntax and resident semantic checks.

use super::*;

/// Native rights readiness changes metadata coverage without erasing BSL findings or their positions.
#[test]
fn role_resources_keep_saved_batch_and_resident_profiles_in_agreement() {
    let (directory, root) = project(super::semantic::BODY, false);
    crate::test_role_project::add_roles(&root, &directory.0.join("native"));
    let source = root.join("src-with-roles");
    let resource = source.join("Roles/Reader/Ext/Rights.xml");
    let saved = fs::read(&resource).unwrap();
    let module = SourcePath::parse("CommonModules/Tools/Ext/Module.bsl").unwrap();
    for invalid in [false, true] {
        if invalid {
            fs::write(&resource, "<NotRights/>").unwrap();
        }
        let mut previous = None;
        for locale in [Locale::Ru, Locale::En] {
            let config = DiagnosticsConfig {
                locale,
                only_enabled: Some(vec![DiagnosticCode::ParseError, DiagnosticCode::SelfAssign]),
                ..DiagnosticsConfig::default()
            };
            let mut session = AnalysisSession::open(&source).unwrap();
            session
                .load_semantic_inputs(session.revision(), &InputRequest::default())
                .unwrap();
            let expected = session
                .module_diagnostics(
                    &module,
                    session.revision(),
                    &config,
                    &QueryCancellation::default(),
                )
                .unwrap();
            let semantic = check_semantic(
                SourceReader::open(&source).unwrap(),
                &config,
                &BatchOptions::default(),
            )
            .unwrap();
            let findings = expected
                .value
                .iter()
                .map(|finding| report::finding(&module, finding, super::semantic::BODY).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(semantic.files[0].findings, findings);
            assert_eq!(findings.len(), 1);
            assert_eq!(semantic.coverage.semantic_profile_complete, Some(!invalid));
            assert_eq!(
                semantic.status,
                if invalid {
                    BatchStatus::Incomplete
                } else {
                    BatchStatus::Findings
                }
            );
            let syntax = check_syntax(
                SourceReader::open(&source).unwrap(),
                &config,
                &BatchOptions::default(),
            )
            .unwrap();
            assert_eq!(syntax.coverage.metadata_models_complete, !invalid);
            assert!(syntax.files[0].findings.is_empty());
            assert_eq!(syntax.metadata_issues, semantic.metadata_issues);
            assert_eq!(
                semantic.resources.metadata_source_bytes,
                session.resources().metadata_source_bytes
            );
            let current = serde_json::to_value(semantic).unwrap();
            if let Some(previous) = &previous {
                assert_eq!(&current, previous);
            }
            previous = Some(current);
        }
    }
    fs::write(&resource, &saved).unwrap();
    assert_eq!(fs::read(resource).unwrap(), saved);
}
