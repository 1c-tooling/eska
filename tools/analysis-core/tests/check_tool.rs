//! Actual process contracts distinguish findings, incomplete input and environment errors in both locales.

mod check_forms;
mod check_metadata;
mod check_roles;
mod check_semantic;
mod support;

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use serde_json::Value;

const GOOD: &str =
    "\u{feff}// Проверяет синтаксис.\r\nПроцедура Проверка() Экспорт\r\nКонецПроцедуры\r\n";

/// Empty PATH proves that checking and project discovery do not invoke another installed tool.
fn run(root: &Path, lang: &str, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska-check"))
        .current_dir(root)
        .env("PATH", "")
        .env("ESKA_LANG", "en-US")
        .args(["--lang", lang])
        .args(arguments)
        .output()
        .unwrap()
}

/// Normal structured results write exactly one JSON envelope and no stderr chatter.
fn report(output: &Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["tool"], "eska-check");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("\"message\""));
    value
}

/// Help/version succeed, parsing errors return exit 3 with locale-independent error schemas.
#[test]
fn help_version_and_required_profile_contract() {
    let (_directory, root) = support::project(GOOD, false);
    let mut errors = vec![];
    for lang in ["ru-RU", "en-US"] {
        let help = run(&root, lang, &["--help"]);
        assert!(help.status.success());
        assert!(help.stderr.is_empty());
        let text = String::from_utf8(help.stdout).unwrap();
        assert!(text.contains(if lang == "ru-RU" {
            "Проверка сохранённых модулей"
        } else {
            "Check saved"
        }));
        assert!(text.contains("--profile") && text.contains("--timeout-seconds"));
        let version = run(&root, lang, &["--version"]);
        assert!(version.status.success());
        assert!(
            String::from_utf8(version.stdout)
                .unwrap()
                .contains(env!("CARGO_PKG_VERSION"))
        );
        let error = run(&root, lang, &["--format", "json"]);
        assert_eq!(error.status.code(), Some(3));
        assert!(!error.stderr.is_empty());
        let value: Value = serde_json::from_slice(&error.stdout).unwrap();
        assert_eq!(value["status"], "error");
        assert_eq!(value["error"]["code"], "arguments_invalid");
        errors.push(value);
    }
    assert_eq!(errors[0], errors[1]);
}

/// Complete supported syntax returns zero and leaves the exact BOM/CRLF source bytes unchanged.
#[test]
fn complete_syntax_result_is_read_only_and_locale_independent() {
    let (_directory, root) = support::project(GOOD, false);
    let source = root.join("src/CommonModules/Tools/Ext/Module.bsl");
    let before = fs::read(&source).unwrap();
    let mut reports = vec![];
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "syntax", "--format", "json"]),
            0,
        );
        assert_eq!(value["status"], "passed");
        assert_eq!(value["data"]["report"]["coverage"]["syntax_complete"], true);
        assert_eq!(
            value["data"]["report"]["coverage"]["metadata_models_complete"],
            true
        );
        assert_eq!(
            value["data"]["report"]["files"][0]["findings"],
            serde_json::json!([])
        );
        reports.push(value);
        let human = run(&root, lang, &["--profile", "syntax"]);
        assert!(human.status.success());
        assert!(
            String::from_utf8(human.stdout)
                .unwrap()
                .contains(if lang == "ru-RU" {
                    "Синтаксис проверен"
                } else {
                    "Syntax checked"
                })
        );
    }
    assert_eq!(reports[0], reports[1]);
    assert_eq!(fs::read(source).unwrap(), before);
    assert!(!root.join(".eska").exists());
}

/// Real syntax findings keep Unicode source coordinates in JSON and localized text in human output.
#[test]
fn syntax_findings_have_exit_one_and_stable_unicode_coordinates() {
    let (_directory, root) = support::project("\u{feff}Процедура Ошибка(\r\n", false);
    let mut reports = vec![];
    let mut humans = vec![];
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "syntax", "--format", "json"]),
            1,
        );
        assert_eq!(value["status"], "findings");
        let finding = &value["data"]["report"]["files"][0]["findings"][0];
        assert_eq!(finding["code"], "ParseError");
        assert!(finding["start_byte"].is_number() && finding["start_column"].is_number());
        assert_eq!(value["data"]["position_encoding"], "unicode-scalar");
        reports.push(value);
        let human = run(&root, lang, &["--profile", "syntax"]);
        assert_eq!(human.status.code(), Some(1));
        assert!(human.stderr.is_empty());
        humans.push(String::from_utf8(human.stdout).unwrap());
    }
    assert_eq!(reports[0], reports[1]);
    assert_ne!(humans[0], humans[1]);
    assert!(
        humans
            .iter()
            .all(|text| text.contains("CommonModules/Tools/Ext/Module.bsl:"))
    );
}

/// A command declared inside its owner YAML remains in the syntax sweep without a command XML file.
#[test]
fn inline_object_command_bodies_are_checked_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = support::inline_command_project(&directory.0, "Процедура Ошибка(\r\n");
    let path = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";
    let before = fs::read(root.join("src-with-command").join(path)).unwrap();
    let mut reports = vec![];
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "syntax", "--format", "json"]),
            1,
        );
        let batch = &value["data"]["report"];
        assert_eq!(batch["coverage"]["modules"], 2);
        assert_eq!(batch["coverage"]["checked_modules"], 2);
        assert_eq!(batch["coverage"]["syntax_complete"], true);
        assert_eq!(batch["coverage"]["metadata_models_complete"], true);
        let command = batch["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["source"] == path)
            .unwrap();
        assert_eq!(command["state"]["kind"], "checked");
        assert!(!command["findings"].as_array().unwrap().is_empty());
        reports.push(value);
    }
    assert_eq!(reports[0], reports[1]);
    assert_eq!(
        fs::read(root.join("src-with-command").join(path)).unwrap(),
        before
    );
}

/// Binary, removed and bounded-out bodies cannot become a clean zero-diagnostic success.
#[test]
fn unread_input_and_deadline_return_exit_two() {
    let (_directory, root) = support::project("binary", true);
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "syntax", "--format", "json"]),
            2,
        );
        assert_eq!(value["status"], "incomplete");
        assert_eq!(value["data"]["report"]["coverage"]["binary_modules"], 1);
    }
    let (_directory, root) = support::project(GOOD, false);
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(
                &root,
                lang,
                &[
                    "--profile",
                    "syntax",
                    "--format",
                    "json",
                    "--timeout-seconds",
                    "0",
                ],
            ),
            2,
        );
        assert_eq!(value["data"]["report"]["stop"], "deadline_exceeded");
        assert_eq!(value["data"]["report"]["coverage"]["unchecked_modules"], 1);
        assert!(
            value["data"]["report"]["metadata_issues"]
                .as_array()
                .unwrap()
                .iter()
                .all(|issue| issue.as_object().unwrap().contains_key("findings")
                    && issue["findings"].is_null())
        );
        let value = report(
            &run(
                &root,
                lang,
                &[
                    "--profile",
                    "syntax",
                    "--format",
                    "json",
                    "--max-document-bytes",
                    "1",
                ],
            ),
            2,
        );
        assert_eq!(
            value["data"]["report"]["coverage"]["unavailable_modules"],
            1
        );
        assert!(
            value["data"]["report"]["metadata_issues"]
                .as_array()
                .unwrap()
                .iter()
                .all(|issue| issue["findings"].is_null())
        );
    }
    fs::remove_file(root.join("src/CommonModules/Tools/Ext/Module.bsl")).unwrap();
    let value = report(
        &run(&root, "en-US", &["--profile", "syntax", "--format", "json"]),
        2,
    );
    assert_eq!(
        value["data"]["report"]["files"][0]["state"]["code"],
        "source_file_missing"
    );
}

/// Complete metadata and BSL share one source quota; its refusal has explicit localized coverage.
#[test]
fn source_quota_is_shared_with_metadata_and_visible_in_both_formats() {
    let (_directory, root) = support::project(GOOD, false);
    let source = root.join("src");
    let bytes = fs::read(source.join("configuration.yaml")).unwrap().len()
        + fs::read(source.join("CommonModules/Tools/object.yaml"))
            .unwrap()
            .len();
    let limit = bytes.to_string();
    let mut reports = vec![];
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(
                &root,
                lang,
                &[
                    "--profile",
                    "syntax",
                    "--format",
                    "json",
                    "--max-retained-source-bytes",
                    &limit,
                ],
            ),
            2,
        );
        let batch = &value["data"]["report"];
        assert_eq!(batch["coverage"]["metadata_models_complete"], true);
        assert_eq!(batch["coverage"]["syntax_complete"], false);
        assert_eq!(batch["coverage"]["unavailable_modules"], 1);
        assert_eq!(batch["resources"]["total_source_bytes"], bytes);
        assert_eq!(batch["resources"]["max_source_bytes"], bytes);
        assert_eq!(batch["resources"]["module_text_bytes"], 0);
        assert_eq!(
            batch["files"][0]["state"]["code"],
            "analysis_source_budget_exceeded"
        );
        reports.push(value);
        let human = run(
            &root,
            lang,
            &["--profile", "syntax", "--max-retained-source-bytes", &limit],
        );
        assert_eq!(human.status.code(), Some(2));
        assert!(human.stderr.is_empty());
        assert!(
            String::from_utf8(human.stdout)
                .unwrap()
                .contains(if lang == "ru-RU" {
                    "квоты"
                } else {
                    "quota"
                })
        );
    }
    assert_eq!(reports[0], reports[1]);
}

/// Analyzer settings apply before checking; invalid or unsupported config is a fatal error.
#[test]
fn settings_are_read_without_silent_default_or_scope_fallback() {
    let (_directory, root) = support::project("Процедура Ошибка(\r\n", false);
    fs::write(
        root.join("bsl-analyzer.toml"),
        "[diagnostics.parameters]\nParseError=false\n",
    )
    .unwrap();
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "syntax", "--format", "json"]),
            2,
        );
        assert_eq!(value["data"]["report"]["profile_enabled"], false);
        assert_eq!(value["data"]["report"]["coverage"]["checked_modules"], 0);
        assert!(
            value["data"]["settings_path"]
                .as_str()
                .unwrap()
                .ends_with("bsl-analyzer.toml")
        );
    }
    for (text, code) in [
        ("broken = [", "analysis_config_invalid"),
        ("[source]\nexclude=['src']\n", "analysis_scope_unsupported"),
    ] {
        fs::write(root.join("bsl-analyzer.toml"), text).unwrap();
        let mut errors = vec![];
        for lang in ["ru-RU", "en-US"] {
            let output = run(&root, lang, &["--profile", "syntax", "--format", "json"]);
            assert_eq!(output.status.code(), Some(3));
            assert!(!output.stderr.is_empty());
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["error"]["code"], code);
            errors.push(value);
        }
        assert_eq!(errors[0], errors[1]);
    }
}

/// Workspace selection retains the member name and asks for one explicit root member.
#[test]
fn workspace_member_selection_uses_the_common_project_selector() {
    let (directory, root) = support::project(GOOD, false);
    let workspace = directory.0.join("workspace");
    fs::create_dir(&workspace).unwrap();
    let member = workspace.join("component");
    fs::rename(root, &member).unwrap();
    fs::write(
        workspace.join("eska.toml"),
        "[workspace]\nmembers=['component']\n",
    )
    .unwrap();
    let output = run(
        &workspace,
        "en-US",
        &["--profile", "syntax", "--format", "json"],
    );
    assert_eq!(output.status.code(), Some(3));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "project_selection_failed");
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(
                &workspace,
                lang,
                &[
                    "--profile",
                    "syntax",
                    "--format",
                    "json",
                    "--project",
                    "syntax-fixture",
                ],
            ),
            0,
        );
        assert_eq!(
            value["data"]["scope"],
            serde_json::json!({"kind":"member", "name":"syntax-fixture"})
        );
        let current = report(
            &run(&member, lang, &["--profile", "syntax", "--format", "json"]),
            0,
        );
        assert_eq!(value, current);
    }
}

/// A discovered Designer XML project is rejected explicitly instead of scanning it as ESKA.
#[test]
fn unsupported_source_and_missing_project_are_environment_errors() {
    let (directory, root) = support::project(GOOD, false);
    fs::write(
        root.join("eska.toml"),
        "[project]\nname='native'\ntype='configuration'\n",
    )
    .unwrap();
    for lang in ["ru-RU", "en-US"] {
        let output = run(&root, lang, &["--profile", "syntax", "--format", "json"]);
        assert_eq!(output.status.code(), Some(3));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["code"], "source_format_mismatch");
        let output = run(
            &directory.0,
            lang,
            &["--profile", "syntax", "--format", "json"],
        );
        assert_eq!(output.status.code(), Some(3));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["code"], "project_discovery_failed");
    }
}
