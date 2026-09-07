use std::{fs, path::Path, process::Command};

use serde_json::Value;

use crate::support::TestDir;

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
    assert_eq!(fs::read(path).unwrap(), before);
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
