//! Semantic process results retain explicit coverage and the same machine schema in both locales.

use super::{fs, report, run, support};
use eska_analysis::DiagnosticCode;

#[path = "../support/semantic_project.rs"]
mod fixture;

const BODY: &str = "\u{feff}Процедура Проверка()\r\n\tЗначение = 1;\r\n\tЗначение = Значение;\r\nКонецПроцедуры\r\n";

/// Select actual native codes without relying on their changing default activation set.
fn settings(root: &std::path::Path, enabled: &[DiagnosticCode]) {
    use std::fmt::Write as _;
    let text = ide::all_diagnostic_codes().fold(
        "[diagnostics.parameters]\n".into(),
        |mut text: String, code| {
            writeln!(text, "{}={}", code.as_str(), enabled.contains(&code)).unwrap();
            text
        },
    );
    fs::write(root.join("bsl-analyzer.toml"), text).unwrap();
}

/// Non-syntax findings, clean selected rules and syntax preserve separate identities and exit codes.
#[test]
fn semantic_profile_runs_configured_rules_with_stable_json_and_localized_summary() {
    let (_directory, root) = support::project(BODY, false);
    settings(
        &root,
        &[DiagnosticCode::SelfAssign, DiagnosticCode::ParseError],
    );
    let before = fs::read(root.join("src/CommonModules/Tools/Ext/Module.bsl")).unwrap();
    let mut values = vec![];
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "semantic", "--format", "json"]),
            1,
        );
        let batch = &value["data"]["report"];
        assert_eq!(batch["profile"], "semantic");
        assert_eq!(batch["coverage"]["semantic_profile_complete"], true);
        assert_eq!(batch["coverage"]["syntax_complete"], true);
        assert_eq!(batch["files"][0]["findings"][0]["code"], "SelfAssign");
        assert_eq!(batch["resources"]["module_text_bytes"], before.len());
        let syntax = report(
            &run(&root, lang, &["--profile", "syntax", "--format", "json"]),
            0,
        );
        assert!(
            syntax["data"]["report"]["coverage"]
                .get("semantic_profile_complete")
                .is_none()
        );
        assert_ne!(
            syntax["data"]["settings_sha256"],
            value["data"]["settings_sha256"]
        );
        let human = run(&root, lang, &["--profile", "semantic"]);
        assert_eq!(human.status.code(), Some(1));
        assert!(human.stderr.is_empty());
        let text = String::from_utf8(human.stdout).unwrap();
        assert!(text.contains(if lang == "ru-RU" {
            "Семантический профиль"
        } else {
            "Semantic profile"
        }));
        let severity = batch["files"][0]["findings"][0]["severity"]
            .as_str()
            .unwrap();
        assert!(text.contains(&format!("SelfAssign ({severity})")));
        values.push(value);
    }
    assert_eq!(values[0], values[1]);
    assert_eq!(
        fs::read(root.join("src/CommonModules/Tools/Ext/Module.bsl")).unwrap(),
        before
    );
    settings(&root, &[DiagnosticCode::UnusedLocalVariable]);
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "semantic", "--format", "json"]),
            0,
        );
        assert_eq!(
            value["data"]["report"]["coverage"]["semantic_profile_complete"],
            true
        );
        assert_eq!(
            value["data"]["report"]["coverage"]["syntax_complete"],
            false
        );
    }
    assert!(!root.join(".eska").exists());
}

/// A resolved exported call and a genuinely missing method are distinguished in the same snapshot.
#[test]
fn semantic_profile_retains_cross_module_dependencies() {
    let (directory, root) = support::project(
        "Procedure Run()\n ZDependency.Available();\n ZDependency.Missing();\nEndProcedure\n",
        false,
    );
    fixture::add_dependency(
        &root,
        &directory.0.join("native"),
        "Procedure Available() Export\nEndProcedure\n",
    );
    settings(&root, &[DiagnosticCode::UnresolvedMethodCall]);
    let mut values = vec![];
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "semantic", "--format", "json"]),
            1,
        );
        let batch = &value["data"]["report"];
        assert_eq!(batch["coverage"]["checked_modules"], 2);
        assert_eq!(batch["coverage"]["semantic_profile_complete"], true);
        assert_eq!(batch["files"][0]["findings"].as_array().unwrap().len(), 1);
        assert_eq!(
            batch["files"][0]["findings"][0]["code"],
            "UnresolvedMethodCall"
        );
        values.push(value);
    }
    assert_eq!(values[0], values[1]);
}

/// Unread bodies, exhausted quotas, stopped analysis and a disabled profile never return exit zero.
#[test]
fn semantic_profile_reports_incomplete_input_in_both_locales() {
    let (_directory, root) = support::project("binary", true);
    settings(&root, &[DiagnosticCode::SelfAssign]);
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "semantic", "--format", "json"]),
            2,
        );
        assert_eq!(
            value["data"]["report"]["coverage"]["semantic_profile_complete"],
            false
        );
        assert_eq!(value["data"]["report"]["coverage"]["binary_modules"], 1);
    }
    let (_directory, root) = support::project(BODY, false);
    settings(&root, &[DiagnosticCode::SelfAssign]);
    for arguments in [
        ["--max-retained-source-bytes", "1"],
        ["--timeout-seconds", "0"],
    ] {
        let mut values = vec![];
        for lang in ["ru-RU", "en-US"] {
            let value = report(
                &run(
                    &root,
                    lang,
                    &[
                        "--profile",
                        "semantic",
                        "--format",
                        "json",
                        arguments[0],
                        arguments[1],
                    ],
                ),
                2,
            );
            assert_eq!(
                value["data"]["report"]["coverage"]["semantic_profile_complete"],
                false
            );
            assert_eq!(
                value["data"]["report"]["resources"]["total_source_bytes"],
                0
            );
            values.push(value);
        }
        assert_eq!(values[0], values[1]);
    }
    settings(&root, &[]);
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "semantic", "--format", "json"]),
            2,
        );
        assert_eq!(value["data"]["report"]["profile_enabled"], false);
        assert_eq!(value["data"]["report"]["coverage"]["unchecked_modules"], 1);
        assert_eq!(value["data"]["report"]["resources"]["module_text_bytes"], 0);
    }

    settings(&root, &[DiagnosticCode::SelfAssign]);
    let path = root.join("src/CommonModules/Tools/object.yaml");
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains("<Server>true</Server>"));
    fs::write(
        &path,
        saved.replacen("<Server>true</Server>", "<Server>Future</Server>", 1),
    )
    .unwrap();
    let mut values = vec![];
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "semantic", "--format", "json"]),
            2,
        );
        let coverage = &value["data"]["report"]["coverage"];
        assert_eq!(coverage["metadata_inputs_complete"], true);
        assert_eq!(coverage["metadata_models_complete"], false);
        assert_eq!(coverage["checked_modules"], 1);
        assert_eq!(coverage["semantic_profile_complete"], false);
        values.push(value);
    }
    assert_eq!(values[0], values[1]);
}
