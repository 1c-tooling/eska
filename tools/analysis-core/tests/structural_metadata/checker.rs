//! Checker completeness separates descriptors, selected BSL rules and standalone query syntax.

use super::{fs, report, run, support};
use eska::project_core::source_format::SourceReader;

#[path = "../support/structural_project.rs"]
mod fixture;

const TEMPLATE: &str = "Reports/Demo/Templates/Schema/template.yaml";
const RESOURCE: &str = "Reports/Demo/Templates/Schema/Template.xml";

/// Valid leaves remove unsupported gaps; malformed explicit fields retain incomplete current coverage.
#[test]
fn structural_descriptors_checker_models_and_scoped_bsl_coverage_are_honest() {
    let directory = support::TestDirectory::new();
    let root = fixture::project(&directory.0);
    fixture::settings(&root);
    let source = root.join("src");
    let original = fs::read_to_string(source.join(TEMPLATE)).unwrap();
    let mut clean = Vec::new();
    let mut invalid = Vec::new();
    for lang in ["ru-RU", "en-US"] {
        fs::write(source.join(TEMPLATE), &original).unwrap();
        let value = report(
            &run(&root, lang, &["--profile", "semantic", "--format", "json"]),
            2,
        );
        let checked = &value["data"]["report"];
        assert_eq!(checked["coverage"]["metadata_models_complete"], true);
        assert_eq!(checked["coverage"]["semantic_profile_complete"], true);
        assert_eq!(checked["query_coverage"]["syntax_complete"], false);
        assert!(checked["query_files"].as_array().unwrap().is_empty());
        assert_eq!(checked["query_coverage"]["metadata_complete"], false);
        assert_eq!(checked["query_coverage"]["consumer_complete"], false);
        assert!(checked["metadata_issues"].as_array().unwrap().is_empty());
        clean.push(value);
        fs::write(
            source.join(TEMPLATE),
            original.replace("DataCompositionSchema", "FutureTemplateType"),
        )
        .unwrap();
        let value = report(
            &run(&root, lang, &["--profile", "semantic", "--format", "json"]),
            2,
        );
        let checked = &value["data"]["report"];
        assert_eq!(checked["coverage"]["metadata_models_complete"], false);
        assert_eq!(checked["coverage"]["semantic_profile_complete"], false);
        assert_eq!(
            checked["metadata_issues"][0]["code"],
            "metadata_projection_invalid"
        );
        let human = run(&root, lang, &["--profile", "semantic"]);
        assert_eq!(human.status.code(), Some(2));
        assert!(human.stderr.is_empty());
        assert!(
            String::from_utf8(human.stdout)
                .unwrap()
                .contains("TemplateType")
        );
        invalid.push(value);
    }
    assert_eq!(clean[0], clean[1]);
    assert_eq!(invalid[0], invalid[1]);
}

/// Broken resource/value/context cannot silently become successful query findings or extra BSL files.
#[test]
fn structural_descriptors_checker_never_claims_dcs_or_query_consumer_validation() {
    for change in [
        "missing-resource",
        "invalid-resource",
        "missing-query",
        "invalid-query",
        "missing-context",
    ] {
        let directory = support::TestDirectory::new();
        let root = fixture::project(&directory.0);
        fixture::settings(&root);
        let settings = root.join("bsl-analyzer.toml");
        fs::write(
            &settings,
            fs::read_to_string(&settings)
                .unwrap()
                .replace("QueryParseError=false", "QueryParseError=true"),
        )
        .unwrap();
        let source = root.join("src");
        let reader = SourceReader::open(&source).unwrap();
        let query = reader.query_paths().next().unwrap().clone();
        let module = reader.module_paths().next().unwrap().clone();
        match change {
            "missing-resource" => fs::remove_file(source.join(RESOURCE)).unwrap(),
            "invalid-resource" => {
                fs::write(source.join(RESOURCE), b"invalid reference XML").unwrap();
            }
            "missing-query" => fs::remove_file(source.join(query.as_path())).unwrap(),
            "invalid-query" => fs::write(source.join(query.as_path()), "SELECT FROM").unwrap(),
            _ => fs::write(
                source.join(query.as_path()),
                "SELECT Missing.Field FROM Catalog.Unknown AS Missing",
            )
            .unwrap(),
        }
        let mut values = Vec::new();
        for lang in ["ru-RU", "en-US"] {
            let value = report(
                &run(&root, lang, &["--profile", "semantic", "--format", "json"]),
                2,
            );
            let checked = &value["data"]["report"];
            assert_eq!(checked["coverage"]["metadata_models_complete"], true);
            assert_eq!(checked["coverage"]["semantic_profile_complete"], true);
            assert_eq!(
                checked["query_coverage"]["metadata_complete"],
                change != "missing-query"
            );
            assert_eq!(checked["query_coverage"]["consumer_complete"], false);
            assert_eq!(checked["query_coverage"]["parameters_complete"], false);
            assert_eq!(checked["query_coverage"]["semantic_complete"], false);
            assert_eq!(
                checked["query_coverage"]["syntax_complete"],
                change != "missing-query"
            );
            let queries = checked["query_files"].as_array().unwrap();
            let current = queries
                .iter()
                .find(|file| file["source"] == query.as_str())
                .unwrap();
            assert_eq!(current["scope"], "current_canonical_metadata");
            if change == "missing-query" {
                assert_eq!(current["state"]["kind"], "unavailable");
                assert!(current["source_sha256"].is_null());
            } else if change == "invalid-query" {
                let findings = current["findings"].as_array().unwrap();
                assert!(!findings.is_empty());
                assert!(
                    findings
                        .iter()
                        .all(|finding| finding["code"] == "QueryParseError")
                );
            }
            let files = checked["files"].as_array().unwrap();
            assert_eq!(files.len(), 1);
            assert_eq!(files[0]["source"].as_str().unwrap(), module.as_str());
            assert!(files[0]["findings"].as_array().unwrap().is_empty());
            values.push(value);
        }
        assert_eq!(values[0], values[1]);
    }
}
