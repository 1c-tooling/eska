//! Checker query results share native parse identity, explicit syntax scope and locale-independent JSON.

use std::fs;

use super::{report, run, support::query_project as fixture};

/// All four declared-query projects produce exact UTF-16/byte findings while preserving native source files.
#[test]
fn query_text_checker_all_four_profiles_match_locales_and_keep_syntax_only_coverage() {
    for kind in fixture::TYPES {
        let project = fixture::project(kind);
        assert_ne!(project.source, project.neighbor);
        fs::write(
            project.root.join("src").join(project.source.as_path()),
            fixture::BAD,
        )
        .unwrap();
        let before = fixture::snapshot(&project);
        let mut machine = Vec::new();
        for lang in ["ru-RU", "en-US"] {
            let output = run(
                &project.root,
                lang,
                &["--profile", "syntax", "--format", "json"],
            );
            let code = if kind == "configuration" { 1 } else { 2 };
            let value = report(&output, code);
            let queries = &value["data"]["report"];
            assert_eq!(queries["query_coverage"]["queries"], 2);
            assert_eq!(queries["query_coverage"]["checked"], 2);
            assert_eq!(queries["query_coverage"]["syntax_complete"], true);
            assert_eq!(queries["query_coverage"]["metadata_complete"], false);
            assert_eq!(queries["query_coverage"]["consumer_complete"], false);
            let file = queries["query_files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|file| file["source"] == project.source.as_str())
                .unwrap();
            assert_eq!(file["scope"], "syntax_only");
            assert_eq!(file["position_encoding"], "utf-16");
            assert!(!file["findings"].as_array().unwrap().is_empty());
            assert_eq!(queries["resources"]["query_text_bytes"], 0);
            for finding in file["findings"].as_array().unwrap() {
                assert_eq!(finding["code"], "QueryParseError");
                let start = usize::try_from(finding["start_byte"].as_u64().unwrap()).unwrap();
                let end = usize::try_from(finding["end_byte"].as_u64().unwrap()).unwrap();
                let coordinates = eska::project_core::source_format::TextCoordinates::new(
                    fixture::BAD,
                    &[start, end],
                )
                .unwrap();
                assert_eq!(
                    finding["range"],
                    serde_json::to_value(coordinates.range(start..end).unwrap()).unwrap()
                );
            }
            let human = run(&project.root, lang, &["--profile", "syntax"]);
            let text = String::from_utf8(human.stdout).unwrap();
            assert!(text.contains(if lang == "ru-RU" {
                "Только синтаксис"
            } else {
                "Syntax only"
            }));
            machine.push(value);
        }
        assert_eq!(machine[0], machine[1]);
        assert_eq!(fixture::snapshot(&project), before);
    }
}

/// Missing query files and disabled parse rules stay incomplete rather than checked empty successes.
#[test]
fn query_text_checker_missing_inputs_disabled_rules_and_semantic_context_are_incomplete() {
    let project = fixture::project("configuration");
    fs::remove_file(project.root.join("src").join(project.source.as_path())).unwrap();
    let missing = report(
        &run(
            &project.root,
            "en-US",
            &["--profile", "syntax", "--format", "json"],
        ),
        2,
    );
    assert_eq!(
        missing["data"]["report"]["query_coverage"]["unavailable"],
        1
    );
    assert_eq!(
        missing["data"]["report"]["query_coverage"]["syntax_complete"],
        false
    );
    fs::write(
        project.root.join("src").join(project.source.as_path()),
        "SELECT 1",
    )
    .unwrap();
    let semantic = report(
        &run(
            &project.root,
            "en-US",
            &["--profile", "semantic", "--format", "json"],
        ),
        2,
    );
    assert_eq!(semantic["data"]["report"]["query_coverage"]["checked"], 2);
    assert_eq!(
        semantic["data"]["report"]["query_coverage"]["consumer_complete"],
        false
    );
    fs::write(
        project.root.join("bsl-analyzer.toml"),
        "[diagnostics.parameters]\nQueryParseError=false\n",
    )
    .unwrap();
    let disabled = report(
        &run(
            &project.root,
            "en-US",
            &["--profile", "syntax", "--format", "json"],
        ),
        2,
    );
    assert_eq!(disabled["data"]["report"]["query_coverage"]["unchecked"], 2);
    assert_eq!(
        disabled["data"]["report"]["query_coverage"]["syntax_complete"],
        false
    );
}
