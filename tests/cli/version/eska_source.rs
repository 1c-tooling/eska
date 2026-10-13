//! Real CLI processes use one ESKA object-module authority with independently supplied trust.

use super::*;
use ::eska::project_core::source_format::{NativeUuid, accept_source, import_designer_queries};

/// An absent body is visible in preview and created only by an independently authorized write.
#[test]
fn eska_cli_explicitly_creates_a_declared_missing_module() {
    for kind in ["processing", "report"] {
        let (fixture, pin) = project(kind, None, false);
        let path = fixture.0.join("src/ObjectModule.bsl");
        let map = fs::read(fixture.0.join("src/source.json")).unwrap();
        for locale in ["ru", "en"] {
            let preview = eska(
                &fixture.0,
                locale,
                &[
                    "version",
                    "migrate",
                    "--initial-version",
                    "1.2.3",
                    "--accepted-base-sha256",
                    &pin,
                    "--format",
                    "json",
                ],
            );
            assert!(preview.status.success(), "{preview:?}");
            let document: Value = serde_json::from_slice(&preview.stdout).unwrap();
            assert_eq!(document["creates_module"], true);
            assert_eq!(document["applied"], false);
            assert_eq!(document["source"], "src/ObjectModule.bsl");
            assert_eq!(document["edits"][0]["byte_start"], 0);
            assert!(!path.exists());
        }
        let output = eska(
            &fixture.0,
            "en",
            &[
                "version",
                "migrate",
                "--initial-version",
                "1.2.3",
                "--accepted-base-sha256",
                &pin,
                "--write",
                "--format",
                "json",
            ],
        );
        assert!(output.status.success(), "{output:?}");
        let document: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(document["creates_module"], true);
        assert_eq!(document["applied"], true);
        assert!(path.is_file());
        assert_eq!(fs::read(fixture.0.join("src/source.json")).unwrap(), map);
        let repeated = eska(
            &fixture.0,
            "ru",
            &[
                "version",
                "migrate",
                "--accepted-base-sha256",
                &pin,
                "--write",
                "--format",
                "json",
            ],
        );
        assert!(repeated.status.success(), "{repeated:?}");
        let document: Value = serde_json::from_slice(&repeated.stdout).unwrap();
        assert_eq!(document["creates_module"], false);
        assert_eq!(document["applied"], false);
    }
}

/// Import an owned external project with an existing text module and optional root freeze.
fn project(kind: &str, module: Option<&str>, frozen: bool) -> (TestDir, String) {
    let fixture = TestDir::new();
    let tag = if kind == "processing" {
        "ExternalDataProcessor"
    } else {
        "ExternalReport"
    };
    fs::create_dir_all(fixture.0.join("native/Исходный.v1/Ext")).unwrap();
    fs::write(fixture.0.join("native/Исходный.v1.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><{tag} uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Пример</Name></Properties></{tag}></MetaDataObject>")).unwrap();
    if let Some(module) = module {
        fs::write(
            fixture.0.join("native/Исходный.v1/Ext/ObjectModule.bsl"),
            module,
        )
        .unwrap();
    }
    import_designer_queries(&fixture.0.join("native"), &fixture.0.join("src")).unwrap();
    fs::create_dir(fixture.0.join(".eska")).unwrap();
    let freeze = if frozen {
        vec![NativeUuid::parse("10000000-0000-4000-8000-000000000001").unwrap()]
    } else {
        vec![]
    };
    let pin = accept_source(
        &fixture.0.join("src"),
        &fixture.0.join(".eska/accepted"),
        &freeze,
    )
    .unwrap();
    fs::write(fixture.0.join("eska.toml"), format!("[project]\ntype='{kind}'\nsource_format='eska'\n[eska]\naccepted_base='.eska/accepted'\n")).unwrap();
    (fixture, pin.sha256)
}

/// Preview/read JSON stays locale-independent; explicit migration and bump publish only BSL edits.
#[test]
fn eska_version_commands_read_migrate_and_bump_in_both_locales() {
    for kind in ["processing", "report"] {
        for value in ["1.2.3", "01.02.003.0004"] {
            let module =
                format!("\u{feff}Функция Версия()\r\nВозврат \"{value}\";\r\nКонецФункции\r\n");
            let (fixture, pin) = project(kind, Some(&module), false);
            let path = fixture.0.join("src/ObjectModule.bsl");
            let mut previews = Vec::new();
            for locale in ["ru", "en"] {
                let output = eska(
                    &fixture.0,
                    locale,
                    &[
                        "version",
                        "migrate",
                        "--accepted-base-sha256",
                        &pin,
                        "--format",
                        "json",
                    ],
                );
                assert!(output.status.success(), "{output:?}");
                let preview: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(preview["schema_version"], 1);
                assert_eq!(preview["source"], "src/ObjectModule.bsl");
                assert_eq!(preview["creates_module"], false);
                assert_eq!(preview["applied"], false);
                assert_eq!(preview["edits"][0]["kind"], "export_function");
                previews.push(preview);
                assert_eq!(fs::read_to_string(&path).unwrap(), module);
            }
            assert_eq!(previews[0], previews[1]);
            let output = eska(
                &fixture.0,
                "ru",
                &[
                    "version",
                    "migrate",
                    "--accepted-base-sha256",
                    &pin,
                    "--write",
                    "--format",
                    "json",
                ],
            );
            assert!(output.status.success(), "{output:?}");
            for locale in ["ru", "en"] {
                let output = eska(&fixture.0, locale, &["version", "--format", "json"]);
                assert!(output.status.success(), "{output:?}");
                let info: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(info["version"], value);
                assert_eq!(info["descriptor"], "src/ObjectModule.bsl");
                let help = eska(&fixture.0, locale, &["version", "bump", "--help"]);
                assert!(String::from_utf8_lossy(&help.stdout).contains("--accepted-base-sha256"));
            }
            let output = Command::new(env!("CARGO_BIN_EXE_eska"))
                .current_dir(&fixture.0)
                .env_remove("ESKA_LANG")
                .env("ESKA_ACCEPTED_BASE_SHA256", &pin)
                .args([
                    "--lang", "en", "version", "bump", "patch", "--format", "json",
                ])
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let info: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(info["descriptor"], "src/ObjectModule.bsl");
            assert_eq!(
                info["current_version"],
                if value == "1.2.3" {
                    "1.2.4"
                } else {
                    "01.02.004.0001"
                }
            );
        }
    }
}

/// Missing/wrong trust and inherited freeze refuse before writing; errors never depend on locale.
#[test]
fn eska_version_trust_and_freeze_failures_preserve_working_bytes() {
    for frozen in [false, true] {
        let module = "Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\r\n";
        let (fixture, pin) = project("report", Some(module), frozen);
        for (expected, explicit) in [
            ("accepted_base_pin_required", None),
            ("accepted_base_pin_invalid", Some("wrong".to_owned())),
            ("source_baseline_untrusted", Some("0".repeat(64))),
        ]
        .into_iter()
        .chain(frozen.then_some(("source_edit_frozen", Some(pin))))
        {
            let mut errors = Vec::new();
            for locale in ["ru", "en"] {
                let mut args = vec!["version", "migrate", "--write", "--format", "json"];
                if let Some(pin) = &explicit {
                    args.extend(["--accepted-base-sha256", pin]);
                }
                let output = Command::new(env!("CARGO_BIN_EXE_eska"))
                    .current_dir(&fixture.0)
                    .env_remove("ESKA_LANG")
                    .env_remove("ESKA_ACCEPTED_BASE_SHA256")
                    .args(["--lang", locale])
                    .args(args)
                    .output()
                    .unwrap();
                assert_eq!(output.status.code(), Some(1), "{output:?}");
                let error: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(error["error"]["code"], expected);
                assert!(!String::from_utf8_lossy(&output.stderr).contains("version-fix-"));
                assert_eq!(
                    fs::read_to_string(fixture.0.join("src/ObjectModule.bsl")).unwrap(),
                    module
                );
                errors.push(error);
            }
            assert_eq!(errors[0], errors[1]);
        }
    }
}
