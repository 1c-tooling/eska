use std::{fs, path::Path, process::Command};

use serde_json::Value;

use crate::support::TestDir;

mod migration;

/// Create a minimal versioned Designer XML configuration project.
fn project(version: &str) -> TestDir {
    let fixture = TestDir::new();
    fs::create_dir(fixture.0.join("src")).expect("source");
    fs::write(
        fixture.0.join("eska.toml"),
        "[project]\ntype = 'configuration'\n",
    )
    .expect("config");
    fs::write(
        fixture.0.join("src/Configuration.xml"),
        format!("\u{feff}<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration><Properties><Name>Пример</Name><Version>{version}</Version><Comment/></Properties></Configuration></MetaDataObject>\r\n"),
    )
    .expect("descriptor");
    fixture
}

/// Create a workspace with independently versioned report and processing members.
fn workspace(report_version: &str, processing_version: &str) -> TestDir {
    let fixture = TestDir::new();
    let report = fixture.0.join("src/sales-report");
    let processing = fixture.0.join("src/import-orders");
    fs::create_dir_all(&report).expect("report member");
    fs::create_dir_all(&processing).expect("processing member");
    fs::write(
        fixture.0.join("eska.toml"),
        "[workspace]\nmembers = ['src/sales-report', 'src/import-orders']\n",
    )
    .expect("workspace config");
    fs::write(
        report.join("eska.toml"),
        "[project]\nname = 'sales-report'\ntype = 'report'\nsource = '.'\n",
    )
    .expect("report config");
    fs::write(
        processing.join("eska.toml"),
        "[project]\nname = 'import-orders'\ntype = 'processing'\nsource = '.'\n",
    )
    .expect("processing config");
    fs::write(
        report.join("SalesReport.xml"),
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><ExternalReport><Properties><Name>SalesReport</Name></Properties></ExternalReport></MetaDataObject>",
    )
    .expect("report descriptor");
    fs::write(
        processing.join("ImportOrders.xml"),
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><ExternalDataProcessor><Properties><Name>ImportOrders</Name></Properties></ExternalDataProcessor></MetaDataObject>",
    )
    .expect("processing descriptor");
    for (root, name, value) in [
        (&report, "SalesReport", report_version),
        (&processing, "ImportOrders", processing_version),
    ] {
        let ext = root.join(name).join("Ext");
        fs::create_dir_all(&ext).unwrap();
        fs::write(
            ext.join("ObjectModule.bsl"),
            format!(
                "\u{feff}Функция Версия() Экспорт\r\n\tВозврат \"{value}\";\r\nКонецФункции\r\n"
            ),
        )
        .unwrap();
    }
    fixture
}

/// Run the localized CLI from a project directory.
fn eska(root: &Path, locale: &str, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(root)
        .env_remove("ESKA_LANG")
        .args(["--lang", locale])
        .args(arguments)
        .output()
        .expect("run eska")
}

#[test]
fn version_json_is_stable_locale_independent_and_read_only() {
    let fixture = project("1.0.2.01");
    let path = fixture.0.join("src/Configuration.xml");
    let before = fs::read(&path).expect("before");
    let mut documents = Vec::new();
    for locale in ["ru", "en"] {
        let output = eska(&fixture.0, locale, &["version", "--format", "json"]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        documents.push(serde_json::from_slice::<Value>(&output.stdout).expect("JSON"));
    }
    assert_eq!(documents[0], documents[1]);
    assert_eq!(documents[0]["schema_version"], 1);
    assert_eq!(documents[0]["version"], "1.0.2.01");
    assert_eq!(documents[0]["descriptor"], "src/Configuration.xml");
    assert_eq!(documents[0].as_object().unwrap().len(), 3);
    assert_eq!(fs::read(path).unwrap(), before);
}

/// The new profile has a locale-independent v2 document with an explicit scheme.
#[test]
fn three_part_json_identifies_its_scheme_and_retains_exact_values() {
    let fixture = project("1.2.3");
    for locale in ["ru", "en"] {
        let read = eska(&fixture.0, locale, &["version", "--format", "json"]);
        assert!(read.status.success(), "{read:?}");
        assert!(read.stderr.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&read.stdout).unwrap(),
            serde_json::json!({
                "schema_version": 2, "version": "1.2.3", "version_scheme": "x.y.z", "descriptor": "src/Configuration.xml"
            })
        );
    }
    for (kind, expected) in [("patch", "1.2.4"), ("minor", "1.3.0"), ("major", "2.0.0")] {
        for locale in ["ru", "en"] {
            let fixture = project("1.2.3");
            let output = eska(
                &fixture.0,
                locale,
                &["version", "bump", kind, "--format", "json"],
            );
            assert!(output.status.success(), "{output:?}");
            assert!(output.stderr.is_empty());
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stdout).unwrap(),
                serde_json::json!({
                    "schema_version": 2, "operation": "bump", "bump": kind,
                    "previous_version": "1.2.3", "current_version": expected,
                    "version_scheme": "x.y.z", "descriptor": "src/Configuration.xml"
                })
            );
        }
    }
}

/// A mixed workspace upgrades the envelope and identifies every member's scheme.
#[test]
fn mixed_workspace_inherits_scheme_and_respects_member_override() {
    let fixture = workspace("1.2.3", "02.3.4.05");
    let root_config = fixture.0.join("eska.toml");
    let mut input = fs::read_to_string(&root_config).unwrap();
    input.push_str("\n[version]\nscheme = 'x.y.z'\n");
    fs::write(&root_config, input).unwrap();
    let member = fixture.0.join("src/import-orders/eska.toml");
    let mut input = fs::read_to_string(&member).unwrap();
    input.push_str("\n[version]\nscheme = 'x.y.z.build'\n");
    fs::write(&member, input).unwrap();
    let mut documents = Vec::new();
    for locale in ["ru", "en"] {
        let output = eska(&fixture.0, locale, &["version", "--format", "json"]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        documents.push(serde_json::from_slice::<Value>(&output.stdout).unwrap());
    }
    assert_eq!(documents[0], documents[1]);
    assert_eq!(documents[0]["schema_version"], 2);
    assert_eq!(documents[0]["projects"][0]["version_scheme"], "x.y.z");
    assert_eq!(documents[0]["projects"][1]["version_scheme"], "x.y.z.build");
    assert_eq!(documents[0]["projects"][1]["version"], "02.3.4.05");
    let output = eska(
        &fixture.0.join("src/sales-report"),
        "en",
        &["version", "bump", "minor"],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("1.3.0"));
    fs::write(
        &member,
        "[project]\nname = 'import-orders'\ntype = 'processing'\nsource = '.'\n",
    )
    .unwrap();
    let path = fixture
        .0
        .join("src/import-orders/ImportOrders/Ext/ObjectModule.bsl");
    let before = fs::read(&path).unwrap();
    for locale in ["ru", "en"] {
        let output = eska(
            &fixture.0,
            locale,
            &["version", "-p", "import-orders", "bump", "patch"],
        );
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("x.y.z.build"));
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn workspace_version_json_lists_all_members_in_manifest_order() {
    let fixture = workspace("1.2.3.4", "2.3.4.5");
    let report_path = fixture
        .0
        .join("src/sales-report/SalesReport/Ext/ObjectModule.bsl");
    let processing_path = fixture
        .0
        .join("src/import-orders/ImportOrders/Ext/ObjectModule.bsl");
    let before = [
        fs::read(&report_path).expect("report before"),
        fs::read(&processing_path).expect("processing before"),
    ];
    let mut documents = Vec::new();
    for locale in ["ru", "en"] {
        let output = eska(&fixture.0, locale, &["version", "--format", "json"]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        documents.push(serde_json::from_slice::<Value>(&output.stdout).expect("JSON"));
    }
    assert_eq!(documents[0], documents[1]);
    let document = &documents[0];
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["projects"][0]["name"], "sales-report");
    assert_eq!(document["projects"][0]["type"], "report");
    assert_eq!(document["projects"][0]["version"], "1.2.3.4");
    assert_eq!(
        document["projects"][0]["descriptor"],
        "SalesReport/Ext/ObjectModule.bsl"
    );
    assert_eq!(document["projects"][1]["name"], "import-orders");
    assert_eq!(document["projects"][1]["type"], "processing");
    assert_eq!(document["projects"][1]["version"], "2.3.4.5");
    assert_eq!(fs::read(report_path).unwrap(), before[0]);
    assert_eq!(fs::read(processing_path).unwrap(), before[1]);
}

#[test]
fn workspace_version_selects_current_named_or_all_members() {
    let fixture = workspace("1.2.3.4", "2.3.4.5");
    let report = fixture.0.join("src/sales-report");

    for locale in ["ru", "en"] {
        let current = eska(&report, locale, &["version"]);
        assert!(current.status.success(), "{current:?}");
        assert_eq!(String::from_utf8_lossy(&current.stdout).trim(), "1.2.3.4");

        let selected = eska(
            &fixture.0,
            locale,
            &["version", "-p", "import-orders", "--format", "json"],
        );
        assert!(selected.status.success(), "{selected:?}");
        let document: Value = serde_json::from_slice(&selected.stdout).expect("JSON");
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["version"], "2.3.4.5");
        assert_eq!(document["descriptor"], "ImportOrders/Ext/ObjectModule.bsl");
        assert!(document.get("projects").is_none());
    }

    let all = eska(&report, "en", &["version", "--workspace"]);
    assert!(all.status.success(), "{all:?}");
    let output = String::from_utf8_lossy(&all.stdout);
    assert!(output.contains("sales-report: 1.2.3.4"), "{output}");
    assert!(output.contains("import-orders: 2.3.4.5"), "{output}");

    let ordered = eska(
        &fixture.0,
        "en",
        &[
            "version",
            "-p",
            "import-orders",
            "-p",
            "sales-report",
            "--format",
            "json",
        ],
    );
    assert!(ordered.status.success(), "{ordered:?}");
    let document: Value = serde_json::from_slice(&ordered.stdout).expect("JSON");
    assert_eq!(document["projects"][0]["name"], "import-orders");
    assert_eq!(document["projects"][1]["name"], "sales-report");
}

#[test]
fn workspace_bump_requires_and_changes_exactly_one_member() {
    let fixture = workspace("1.2.3.4", "2.3.4.5");
    let report_path = fixture
        .0
        .join("src/sales-report/SalesReport/Ext/ObjectModule.bsl");
    let processing_path = fixture
        .0
        .join("src/import-orders/ImportOrders/Ext/ObjectModule.bsl");
    let report_before = fs::read(&report_path).unwrap();
    let processing_before = fs::read(&processing_path).unwrap();

    for (locale, expected) in [
        ("ru", "выберите ровно один проект"),
        ("en", "Select exactly one project"),
    ] {
        let output = eska(&fixture.0, locale, &["version", "bump", "patch"]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
        assert_eq!(fs::read(&report_path).unwrap(), report_before);
        assert_eq!(fs::read(&processing_path).unwrap(), processing_before);
    }

    let selected = eska(
        &fixture.0,
        "en",
        &[
            "version",
            "-p",
            "sales-report",
            "bump",
            "patch",
            "--format",
            "json",
        ],
    );
    assert!(selected.status.success(), "{selected:?}");
    let document: Value = serde_json::from_slice(&selected.stdout).expect("JSON");
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["descriptor"], "SalesReport/Ext/ObjectModule.bsl");
    assert!(document.get("projects").is_none());
    assert!(
        fs::read_to_string(&report_path)
            .unwrap()
            .contains("Возврат \"1.2.4.1\"")
    );
    assert_eq!(fs::read(&processing_path).unwrap(), processing_before);

    let current = eska(
        &fixture.0.join("src/import-orders"),
        "en",
        &["version", "bump", "minor"],
    );
    assert!(current.status.success(), "{current:?}");
    assert!(
        fs::read_to_string(&processing_path)
            .unwrap()
            .contains("Возврат \"2.4.1.1\"")
    );
}

#[test]
fn workspace_version_rejects_invalid_unknown_and_multiple_bump_selectors() {
    let fixture = workspace("1.2.3.4", "2.3.4.5");
    let report_path = fixture
        .0
        .join("src/sales-report/SalesReport/Ext/ObjectModule.bsl");
    let processing_path = fixture
        .0
        .join("src/import-orders/ImportOrders/Ext/ObjectModule.bsl");
    let report_before = fs::read(&report_path).unwrap();
    let processing_before = fs::read(&processing_path).unwrap();
    for (arguments, expected) in [
        (
            vec!["version", "-p", "Unknown"],
            "Invalid workspace project name",
        ),
        (
            vec!["version", "-p", "missing"],
            "Workspace project \"missing\" was not found",
        ),
        (
            vec!["version", "-p", "sales-report", "-p", "sales-report"],
            "selected more than once",
        ),
        (
            vec![
                "version",
                "-p",
                "sales-report",
                "-p",
                "import-orders",
                "bump",
                "patch",
            ],
            "requires exactly one project",
        ),
        (
            vec!["version", "--workspace", "bump", "patch"],
            "requires exactly one project",
        ),
        (
            vec!["version", "-p", "sales-report", "--workspace"],
            "cannot be used together",
        ),
    ] {
        let output = eska(&fixture.0, "en", &arguments);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr).replace(['\u{2068}', '\u{2069}'], "");
        assert!(stderr.contains(expected), "{output:?}");
    }
    assert_eq!(fs::read(report_path).unwrap(), report_before);
    assert_eq!(fs::read(processing_path).unwrap(), processing_before);

    let standalone = project("1.2.3.4");
    let output = eska(&standalone.0, "en", &["version", "-p", "sales-report"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("only be used inside a workspace"));
}

#[test]
fn bump_json_reports_the_transition_and_updates_the_descriptor() {
    let fixture = project("01.0.2.01");
    let output = eska(
        &fixture.0,
        "ru",
        &["version", "bump", "patch", "--format", "json"],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty());
    let document: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["operation"], "bump");
    assert_eq!(document["bump"], "patch");
    assert_eq!(document["previous_version"], "01.0.2.01");
    assert_eq!(document["current_version"], "01.0.3.01");
    assert_eq!(document["descriptor"], "src/Configuration.xml");
    let xml = fs::read_to_string(fixture.0.join("src/Configuration.xml")).expect("XML");
    assert!(xml.contains("<Version>01.0.3.01</Version>"));
}

#[test]
fn human_output_and_nested_help_are_localized() {
    let fixture = project("1.0.2.1");
    for (locale, about, bump_about, changed) in [
        (
            "ru",
            "Показать или изменить версию проекта 1С",
            "Увеличить один компонент версии проекта",
            "Версия проекта изменена с 1.0.2.1 на 1.1.1.1",
        ),
        (
            "en",
            "Show or update the 1C project version",
            "Increment one project version component",
            "Project version changed from 1.0.2.1 to 1.1.1.1",
        ),
    ] {
        let help = eska(&fixture.0, locale, &["version", "--help"]);
        assert!(help.status.success(), "{help:?}");
        assert!(String::from_utf8_lossy(&help.stdout).contains(about));
        let bump_help = eska(&fixture.0, locale, &["version", "bump", "--help"]);
        assert!(bump_help.status.success(), "{bump_help:?}");
        assert!(String::from_utf8_lossy(&bump_help.stdout).contains(bump_about));

        fs::write(
            fixture.0.join("src/Configuration.xml"),
            "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration><Properties><Version>1.0.2.1</Version></Properties></Configuration></MetaDataObject>",
        )
        .expect("reset version");
        let output = eska(&fixture.0, locale, &["version", "bump", "minor"]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout)
            .expect("UTF-8")
            .replace(['\u{2068}', '\u{2069}'], "");
        assert!(stdout.contains(changed), "{stdout}");
        assert!(!stdout.contains('\x1b'));
    }
}

#[test]
fn invalid_project_version_fails_without_modifying_the_descriptor() {
    let fixture = project("1.2.bad.4");
    let path = fixture.0.join("src/Configuration.xml");
    let before = fs::read(&path).expect("before");
    for (locale, expected) in [
        ("ru", "Некорректная версия проекта 1С"),
        ("en", "Invalid 1C project version"),
    ] {
        let output = eska(&fixture.0, locale, &["version", "bump", "patch"]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
