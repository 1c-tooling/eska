//! Public standalone CLI tests own temporary files under the configured sibling playground.

#[path = "../../../tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
use support::TestDir;

/// An absent analyzer is deliberate: formatting must not invoke or require it.
fn run(root: &Path, locale: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska-fmt"))
        .current_dir(root)
        .args(["--lang", locale])
        .args(args)
        .env("ESKA_BSL_ANALYZER", root.join("missing-analyzer"))
        .output()
        .unwrap()
}

/// JSON is parsed only after asserting the documented process exit, including non-success check results.
fn checked(output: &Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&output.stdout)))
}

/// Check/diff do not write, both locales share JSON, and successful application is idempotent.
#[test]
fn check_preview_apply_and_locale_parity() {
    let directory = TestDir::new();
    let path = directory.0.join("Module.bsl");
    let before = "\u{feff}процедура Test()\r\nА=1;\r\nконецпроцедуры";
    fs::write(&path, before).unwrap();
    let ru = checked(
        &run(
            &directory.0,
            "ru",
            &["Module.bsl", "--check", "--format", "json"],
        ),
        1,
    );
    let en = checked(
        &run(
            &directory.0,
            "en",
            &["Module.bsl", "--check", "--format", "json"],
        ),
        1,
    );
    assert_eq!(ru, en);
    assert_eq!(ru["files"][0]["status"], "planned");
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
    let diff = checked(
        &run(
            &directory.0,
            "ru",
            &["Module.bsl", "--diff", "--format", "json"],
        ),
        0,
    );
    assert!(
        diff["files"][0]["diff"]
            .as_str()
            .unwrap()
            .contains("@@ -1,")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
    let applied = checked(
        &run(&directory.0, "ru", &["Module.bsl", "--format", "json"]),
        0,
    );
    assert_eq!(applied["files"][0]["status"], "applied");
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("\r\n\tА = 1;"));
    assert!(after.starts_with('\u{feff}'));
    let repeated = checked(
        &run(
            &directory.0,
            "en",
            &["Module.bsl", "--check", "--format", "json"],
        ),
        0,
    );
    assert_eq!(repeated["files"][0]["changed"], false);
}

/// Preset selection is manifest-owned and gives identical preview, apply and inspection in ru/en.
#[test]
fn eska_preset_is_shared_by_check_apply_and_config_dump() {
    let directory = TestDir::new();
    fs::create_dir_all(directory.0.join("src/Ext")).unwrap();
    fs::write(
        directory.0.join("eska.toml"),
        "[project]\ntype='configuration'\n[format]\nprofile='eska'\nprofile_version=1\n",
    )
    .unwrap();
    fs::write(directory.0.join("src/Configuration.xml"), "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Demo</Name></Properties><ChildObjects/></Configuration></MetaDataObject>").unwrap();
    fs::write(
        directory.0.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
    let relative = "src/Ext/ManagedApplicationModule.bsl";
    let path = directory.0.join(relative);
    let before = "\u{feff}Процедура Тест()\r\nДанные.Вставить(\"А\",1);\r\nДанные.Вставить(\"Длинное\",2);\r\nЕсли Условие Тогда\r\nПервое();\r\nВторое();\r\nКонецЕсли;\r\nПока Условие Цикл\r\n\r\nМетод(\r\n1,\r\n2);\r\n\r\nКонецЦикла;\r\nВозврат;\r\nКонецПроцедуры";
    fs::write(&path, before).unwrap();
    let args = [relative, "--dump-config", "--format", "json"];
    let ru = checked(&run(&directory.0, "ru", &args), 0);
    let en = checked(&run(&directory.0, "en", &args), 0);
    assert_eq!(ru, en);
    let policy = &ru["files"][0]["effective"];
    assert_eq!(policy["profile"], "eska");
    assert_eq!(policy["options"]["column_limit"], 150);
    assert_eq!(policy["origins"]["align_call_arguments"], "profile:eska@1");
    assert_eq!(policy["options"]["separate_object_initialization"], true);
    assert_eq!(
        policy["origins"]["separate_object_initialization"],
        "profile:eska@1"
    );
    for key in [
        "blank_lines_around_blocks",
        "blank_lines_inside_multistatement_blocks",
        "blank_lines_before_return",
    ] {
        assert_eq!(policy["options"][key], 1);
        assert_eq!(policy["origins"][key], "profile:eska@1");
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
    let args = [relative, "--check", "--format", "json"];
    let ru = checked(&run(&directory.0, "ru", &args), 1);
    let en = checked(&run(&directory.0, "en", &args), 1);
    assert_eq!(ru, en);
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
    checked(&run(&directory.0, "ru", &[relative, "--format", "json"]), 0);
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.starts_with("\u{feff}Процедура Тест()\r\n\r\n"));
    assert!(after.contains("Данные.Вставить(\"А\"      , 1);\r\n"));
    assert!(after.contains("\r\n\r\n\tЕсли Условие Тогда\r\n\r\n\t\tПервое();\r\n\t\tВторое();\r\n\r\n\tКонецЕсли;\r\n\r\n"));
    assert!(after.contains("\tПока Условие Цикл\r\n\t\tМетод(\r\n\t\t\t1,\r\n\t\t\t2\r\n\t\t);\r\n\tКонецЦикла;\r\n\r\n\tВозврат;\r\n\r\nКонецПроцедуры\r\n"));
    assert!(!after.replace("\r\n", "").contains('\n'));
    checked(&run(&directory.0, "en", &args), 0);
}

/// Removing gaps and collapsing calls reaches a stable checked result in either locale.
#[test]
fn compact_alignment_is_idempotent_through_the_cli() {
    let directory = TestDir::new();
    let path = directory.0.join("Module.bsl");
    let before = "\u{feff}Call(\r\n\"A\",1);\r\n\r\nCall(\"Long\",2);\r\n";
    let style = r#"{"align_call_arguments":true,"max_empty_lines":0}"#;
    let args = [
        "Module.bsl",
        "--style",
        style,
        "--check",
        "--format",
        "json",
    ];
    for locale in ["ru", "en"] {
        fs::write(&path, before).unwrap();
        let preview = checked(&run(&directory.0, locale, &args), 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        assert_eq!(preview, checked(&run(&directory.0, "en", &args), 1));
        checked(
            &run(
                &directory.0,
                locale,
                &["Module.bsl", "--style", style, "--format", "json"],
            ),
            0,
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "\u{feff}Call(\"A\"   , 1);\r\nCall(\"Long\", 2);\r\n"
        );
        checked(&run(&directory.0, locale, &args), 0);
    }
}

/// A later syntax failure cannot leave an earlier valid module formatted.
#[test]
fn complete_preflight_keeps_all_sources_on_failure() {
    let directory = TestDir::new();
    fs::write(directory.0.join("A.bsl"), "А=1;").unwrap();
    fs::write(directory.0.join("Z.bsl"), "Если Тогда").unwrap();
    let result = checked(&run(&directory.0, "en", &["--format", "json"]), 2);
    assert_eq!(result["files"][0]["status"], "not_applied");
    assert_eq!(result["files"][1]["status"], "failed");
    assert_eq!(
        fs::read_to_string(directory.0.join("A.bsl")).unwrap(),
        "А=1;"
    );
    assert_eq!(
        fs::read_to_string(directory.0.join("Z.bsl")).unwrap(),
        "Если Тогда"
    );
}

/// Output expansion is rejected during planning, preserving every file with identical ru/en results.
#[test]
fn excessive_output_keeps_the_complete_batch_unchanged() {
    let directory = TestDir::new();
    let large = format!(
        "{}{}{}",
        "If A Then\n".repeat(127),
        "B=1;\n".repeat(4_200),
        "EndIf;\n".repeat(127)
    );
    fs::write(directory.0.join("A.bsl"), "A=1;").unwrap();
    fs::write(directory.0.join("Z.bsl"), &large).unwrap();
    let args = [
        "--style",
        r#"{"indent_style":"space","indent_width":32}"#,
        "--format",
        "json",
    ];
    let ru = checked(&run(&directory.0, "ru", &args), 2);
    let en = checked(&run(&directory.0, "en", &args), 2);
    assert_eq!(ru, en);
    assert_eq!(ru["error"]["code"], "output_too_large");
    assert_eq!(ru["files"][0]["status"], "not_applied");
    assert_eq!(ru["files"][1]["status"], "failed");
    assert_eq!(
        fs::read_to_string(directory.0.join("A.bsl")).unwrap(),
        "A=1;"
    );
    assert_eq!(
        fs::read_to_string(directory.0.join("Z.bsl")).unwrap(),
        large
    );
    assert!(!directory.0.join(".eska").exists());
}

/// A native query lease in another process is visible to the standalone formatter and released by drop.
#[test]
fn shared_source_lease_blocks_the_cli() {
    let directory = TestDir::new();
    let path = directory.0.join("Module.bsl");
    fs::write(&path, "А=1;").unwrap();
    let lease = eska_project_core::project::source_lock::SourceLease::acquire(
        &directory.0,
        std::slice::from_ref(&path),
    )
    .unwrap();
    let blocked = checked(
        &run(&directory.0, "ru", &["Module.bsl", "--format", "json"]),
        2,
    );
    assert_eq!(blocked["error"]["code"], "source_busy");
    assert_eq!(fs::read_to_string(&path).unwrap(), "А=1;");
    drop(lease);
    checked(
        &run(&directory.0, "ru", &["Module.bsl", "--format", "json"]),
        0,
    );
    assert_eq!(fs::read_to_string(path).unwrap(), "А = 1;\n");
}

/// Effective style inspection does not parse invalid source; operation overrides remain highest priority.
#[test]
fn dump_config_and_style_validation() {
    let directory = TestDir::new();
    fs::write(directory.0.join("Text.sdbl"), "broken").unwrap();
    let result = checked(
        &run(
            &directory.0,
            "ru",
            &[
                "Text.sdbl",
                "--dump-config",
                "--style",
                "{\"column_limit\":80}",
                "--format",
                "json",
            ],
        ),
        0,
    );
    assert_eq!(
        result["files"][0]["effective"]["options"]["column_limit"],
        80
    );
    assert_eq!(
        result["files"][0]["effective"]["origins"]["column_limit"],
        "operation"
    );
    let rejected = checked(
        &run(
            &directory.0,
            "en",
            &[
                "Text.sdbl",
                "--dump-config",
                "--style",
                "{\"unknown\":1}",
                "--format",
                "json",
            ],
        ),
        2,
    );
    assert_eq!(rejected["error"]["code"], "invalid_options");
}

/// Exclusion and policy inspection do not decode source bytes or require a complete metadata export.
#[test]
fn excluded_sources_are_not_read_or_parsed() {
    let directory = TestDir::new();
    fs::create_dir(directory.0.join("src")).unwrap();
    fs::write(
        directory.0.join("eska.toml"),
        "[project]\ntype='configuration'\n[format]\nexclude=['src/Generated.bsl']\n",
    )
    .unwrap();
    fs::write(directory.0.join("src/Generated.bsl"), [0xff, 0xfe, 0x00]).unwrap();
    let result = checked(
        &run(&directory.0, "en", &["--check", "--format", "json"]),
        0,
    );
    assert_eq!(result["files"][0]["status"], "excluded");
    assert!(result["files"][0]["source_sha256"].is_null());
    assert_eq!(
        fs::read(directory.0.join("src/Generated.bsl")).unwrap(),
        [0xff, 0xfe, 0x00]
    );
    assert!(!directory.0.join(".eska").exists());
}

/// The machine entry has no locale or file lookup and uses the real library algorithm.
#[test]
fn pure_text_protocol_checks_hash_and_rejects_unknown_input() {
    let directory = TestDir::new();
    let request = eska_project_core::formatting::protocol::Request {
        schema_version: 1,
        language: eska_project_core::formatting::Language::Sdbl,
        text: "select 1 as A".into(),
        options: eska_project_core::formatting::Options::default(),
        range: None,
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_eska-fmt"))
        .arg("__format-text")
        .current_dir(&directory.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let output = checked(&child.wait_with_output().unwrap(), 0);
    assert_eq!(
        output,
        serde_json::to_value(eska_fmt::format(&request).unwrap()).unwrap()
    );
    let contract = run(&directory.0, "en", &["--help"]);
    assert!(contract.status.success());
    assert!(String::from_utf8_lossy(&contract.stdout).contains("without an analyzer"));
    assert_eq!(json!(1), output["schema_version"]);
    let invalid = eska_project_core::formatting::protocol::Request {
        schema_version: 2,
        ..request
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_eska-fmt"))
        .arg("__format-text")
        .current_dir(&directory.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&invalid).unwrap())
        .unwrap();
    assert_eq!(
        checked(&child.wait_with_output().unwrap(), 2)["error"]["code"],
        "incompatible_protocol"
    );
}

/// Real Designer ownership enforces support, and explicit legacy migration preserves a restorable manifest.
#[test]
fn project_support_and_manifest_migration() {
    let directory = TestDir::new();
    fs::create_dir_all(directory.0.join("src/Ext")).unwrap();
    let manifest = "[project]\ntype='configuration'\n[tools.bsl-analyzer.formatting]\nuse_tabs=false\nindent_size=2\n";
    fs::write(directory.0.join("eska.toml"), manifest).unwrap();
    let uuid = "11111111-1111-1111-1111-111111111111";
    fs::write(directory.0.join("src/Configuration.xml"),format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='{uuid}'><Properties><Name>Demo</Name></Properties><ChildObjects/></Configuration></MetaDataObject>")).unwrap();
    fs::write(
        directory.0.join("src/Ext/ManagedApplicationModule.bsl"),
        "Процедура Тест()\nА=1;\nКонецПроцедуры",
    )
    .unwrap();
    let rules = format!(
        "{{6,0,1,{uuid},0,{uuid},\"1\",\"Vendor\",\"Test\",2,0,0,{uuid},{uuid},0,0,22222222-2222-2222-2222-222222222222,22222222-2222-2222-2222-222222222222,0,0,0,1,0,0,0,1,0,1,0,1,1,1,1}}"
    );
    fs::write(directory.0.join("src/Ext/ParentConfigurations.bin"), rules).unwrap();
    assert_eq!(
        checked(&run(&directory.0, "ru", &["--format", "json"]), 0)["files"][0]["skip_reason"],
        "source_read_only"
    );
    fs::write(
        directory.0.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
    checked(
        &run(&directory.0, "en", &["--check", "--format", "json"]),
        1,
    );
    let preview = checked(
        &run(
            &directory.0,
            "ru",
            &["--migrate-config", "--format", "json"],
        ),
        0,
    );
    assert_eq!(preview["migration"]["changed"], true);
    assert_eq!(
        fs::read_to_string(directory.0.join("eska.toml")).unwrap(),
        manifest
    );
    let applied = checked(
        &run(
            &directory.0,
            "en",
            &["--apply-migration", "--format", "json"],
        ),
        0,
    );
    let backup = applied["migration"]["backup"].as_str().unwrap();
    assert_eq!(fs::read_to_string(backup).unwrap(), manifest);
    checked(&run(&directory.0, "ru", &["--format", "json"]), 0);
    assert!(
        fs::read_to_string(directory.0.join("src/Ext/ManagedApplicationModule.bsl"))
            .unwrap()
            .contains("\n  А = 1;")
    );
    assert_eq!(
        checked(
            &run(
                &directory.0,
                "en",
                &["--apply-migration", "--format", "json"]
            ),
            0
        )["migration"]["changed"],
        false
    );
}

/// The explicit query option formats only the static literal and leaves surrounding unformatted BSL intact.
#[test]
fn query_at_uses_the_checked_owner_source_map() {
    let directory = TestDir::new();
    let before = "q=\"select 1 as A\"; // parent\nX=  1;\n";
    fs::write(directory.0.join("Module.bsl"), before).unwrap();
    checked(
        &run(
            &directory.0,
            "ru",
            &["Module.bsl", "--query-at", "3", "--format", "json"],
        ),
        0,
    );
    let after = fs::read_to_string(directory.0.join("Module.bsl")).unwrap();
    assert!(after.starts_with("q=\"SELECT"));
    assert!(after.ends_with("; // parent\nX=  1;\n"));
    assert_eq!(
        checked(
            &run(
                &directory.0,
                "en",
                &[
                    "Module.bsl",
                    "--query-at",
                    "3",
                    "--check",
                    "--format",
                    "json"
                ]
            ),
            0
        )["files"][0]["changed"],
        false
    );
}

/// Export only owned metadata fixtures; locked modules deliberately contain syntax the formatter cannot parse.
fn supported_fixture(directory: &TestDir) -> Vec<(String, String)> {
    fs::create_dir_all(directory.0.join("src/Ext")).unwrap();
    fs::write(
        directory.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    let root = "11111111-1111-1111-1111-111111111111";
    let ids = [
        "22222222-2222-2222-2222-222222222222",
        "33333333-3333-3333-3333-333333333333",
        "44444444-4444-4444-4444-444444444444",
    ];
    fs::write(directory.0.join("src/Configuration.xml"), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='{root}'><Properties><Name>Demo</Name></Properties><ChildObjects><CommonModule>Locked</CommonModule><CommonModule>Supported</CommonModule><CommonModule>Own</CommonModule></ChildObjects></Configuration></MetaDataObject>")).unwrap();
    let mut sources = Vec::new();
    for (name, id) in ["Locked", "Supported", "Own"].into_iter().zip(ids) {
        let module = format!("src/CommonModules/{name}/Ext/Module.bsl");
        fs::create_dir_all(directory.0.join(&module).parent().unwrap()).unwrap();
        fs::write(directory.0.join(format!("src/CommonModules/{name}.xml")), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><CommonModule uuid='{id}'><Properties><Name>{name}</Name></Properties></CommonModule></MetaDataObject>")).unwrap();
        let text = if name == "Locked" {
            "invalid \"unterminated"
        } else {
            "Procedure Run()\nA=1;\nQ=\"select 1 as A,2 as B\";\nEndProcedure\n"
        };
        fs::write(directory.0.join(&module), text).unwrap();
        sources.push((module, text.to_owned()));
    }
    let mut rules = vec![
        "6",
        "0",
        "1",
        root,
        "0",
        root,
        "\"1\"",
        "\"Vendor\"",
        "\"Demo\"",
        "2",
    ];
    rules.extend(["0", "0", ids[0], ids[0], "1", "0", ids[1], ids[1], "0", "0"]);
    rules.extend(["0"; 13]);
    fs::write(
        directory.0.join("src/Ext/ParentConfigurations.bin"),
        format!("{{{}}}", rules.join(",")),
    )
    .unwrap();
    sources
}

/// Global CI/check/diff operations skip supplier modules before parsing and still process unrestricted code.
#[test]
fn bulk_cli_never_formats_locked_or_editable_with_support_modules() {
    let directory = TestDir::new();
    let sources = supported_fixture(&directory);
    for mode in ["--check", "--diff"] {
        let args = [mode, "--format", "json"];
        let ru = checked(
            &run(&directory.0, "ru", &args),
            i32::from(mode == "--check"),
        );
        let en = checked(
            &run(&directory.0, "en", &args),
            i32::from(mode == "--check"),
        );
        assert_eq!(ru, en);
        assert_eq!(ru["files"][0]["skip_reason"], "source_read_only");
        assert_eq!(ru["files"][2]["skip_reason"], "selection_required");
        assert!(ru["files"][0]["source_sha256"].is_null());
        assert!(ru["files"][2]["source_sha256"].is_null());
        for (path, text) in &sources {
            assert_eq!(fs::read_to_string(directory.0.join(path)).unwrap(), *text);
        }
    }
    let result = checked(&run(&directory.0, "en", &["--format", "json"]), 0);
    assert_eq!(result["files"][1]["status"], "applied");
    for index in [0, 1] {
        assert_eq!(
            fs::read_to_string(directory.0.join(&sources[index].0)).unwrap(),
            sources[index].1
        );
    }
    assert!(
        fs::read_to_string(directory.0.join(&sources[2].0))
            .unwrap()
            .contains("\tA = 1;")
    );
    for (index, code) in [(0, "source_read_only"), (1, "selection_required")] {
        for locale in ["ru", "en"] {
            let output = run(
                &directory.0,
                locale,
                &[&sources[index].0, "--format", "json"],
            );
            assert_eq!(checked(&output, 2)["error"]["code"], code);
        }
    }
}

/// A query is an explicit source fragment, while a controlled method prohibits even that narrow operation.
#[test]
fn query_formatting_respects_supplier_selection_and_control_annotations() {
    let directory = TestDir::new();
    let sources = supported_fixture(&directory);
    let (path, before) = &sources[1];
    let offset = before.find("select").unwrap().to_string();
    checked(
        &run(
            &directory.0,
            "en",
            &[path, "--query-at", &offset, "--format", "json"],
        ),
        0,
    );
    let after = fs::read_to_string(directory.0.join(path)).unwrap();
    assert!(after.contains("A=1;"));
    assert!(after.contains("SELECT"));
    let controlled = format!("&ChangeAndValidate(\"Run\")\n{before}");
    fs::write(directory.0.join(path), &controlled).unwrap();
    let offset = controlled.find("select").unwrap().to_string();
    for locale in ["ru", "en"] {
        let result = checked(
            &run(
                &directory.0,
                locale,
                &[path, "--query-at", &offset, "--format", "json"],
            ),
            2,
        );
        assert_eq!(result["error"]["code"], "change_control_protected");
        assert_eq!(
            fs::read_to_string(directory.0.join(path)).unwrap(),
            controlled
        );
    }
}
