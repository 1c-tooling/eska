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
        checked(&run(&directory.0, "ru", &["--format", "json"]), 2)["error"]["code"],
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
