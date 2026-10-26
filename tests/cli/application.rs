//! Existing structural validation accepts the new schema and localizes rejected declarations.

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use crate::support::TestDir;

/// Validate without PATH tools so Git, platform discovery and an external analyzer cannot help.
fn validate(root: &Path, language: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(root)
        .args(["--lang", language])
        .env_remove("ESKA_LANG")
        .env("PATH", "")
        .output()
        .unwrap()
}

/// A member's source belongs to its manifest; directory names are not metadata identities.
fn member(root: &Path, name: &str, kind: &str) {
    let root = root.join(name);
    fs::create_dir_all(root.join("XML с пробелами")).unwrap();
    fs::write(
        root.join("eska.toml"),
        format!("[project]\nname = {name:?}\ntype = {kind:?}\nsource = 'XML с пробелами'\n"),
    )
    .unwrap();
}

#[test]
/// Schema validation performs no writes and does not depend on locale or installed executables.
fn application_declarations_are_validated_without_runtime_dependencies() {
    let fixture = TestDir::new();
    member(&fixture.0, "base", "configuration");
    member(&fixture.0, "ext", "extension");
    member(&fixture.0, "external", "report");
    let input = "[workspace]\n[analysis]\ndefault_application = 'accounting'\n[workspace.members.accounting]\nconfiguration = 'base'\nextensions = ['ext']\nexternal_objects = ['external']\ntarget_platform_version = '8.3.27'\n";
    fs::write(fixture.0.join("eska.toml"), input).unwrap();
    for language in ["ru", "en"] {
        let output = validate(&fixture.0, language);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        assert_eq!(
            fs::read_to_string(fixture.0.join("eska.toml")).unwrap(),
            input
        );
        assert!(!fixture.0.join("bsl-analyzer.toml").exists());
        assert!(!fixture.0.join(".eska").exists());
    }
}

#[test]
/// Each new failure has actionable text in both locales and a failing exit status.
fn invalid_context_relationships_have_localized_errors() {
    let fixture = TestDir::new();
    member(&fixture.0, "base", "configuration");
    member(&fixture.0, "ext", "extension");
    for (fields, russian, english) in [
        (
            "[workspace.members.App]\nconfiguration = 'base'",
            "некорректное имя",
            "Invalid name",
        ),
        (
            "[analysis]\ndefault_application = 'missing'",
            "не объявлено",
            "is not declared",
        ),
        (
            "[analysis]\ntarget_platform_version = '8.3'",
            "Некорректная целевая платформа",
            "Invalid target platform",
        ),
        (
            "[workspace.members.app]\nconfiguration = 'base'\nextensions = ['ext', 'ext']",
            "несколько раз",
            "more than once",
        ),
        (
            "[workspace.members.app]\nconfiguration = 'base'\nextensions = ['ext']\nextension_dependencies = {ext = ['ext']}",
            "содержат цикл",
            "contain a cycle",
        ),
        (
            "[workspace.members.app]\nconfiguration = 'base'\nextensions = ['ext']\nextension_dependencies = {ext = ['missing']}",
            "не входит",
            "is not in",
        ),
        (
            "[workspace.members.app]\nconfiguration = 'missing'",
            "missing",
            "missing",
        ),
        (
            "[workspace.members.app]\nconfiguration = 'ext'",
            "получено extension",
            "found extension",
        ),
        (
            "[analysis]\ntarget_platform_version = '8.3.27'\n[workspace.members.app]\nconfiguration = 'base'",
            "удалите общий",
            "remove the shared",
        ),
    ] {
        let input = format!("[workspace]\n{fields}\n");
        fs::write(fixture.0.join("eska.toml"), &input).unwrap();
        for (language, expected) in [("ru", russian), ("en", english)] {
            let output = validate(&fixture.0, language);
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert_eq!(output.stdout, [] as [u8; 0]);
            let error = String::from_utf8(output.stderr)
                .unwrap()
                .replace(['\u{2068}', '\u{2069}'], "");
            assert!(error.contains(expected), "{language}: {error}");
            assert!(!error.contains('\u{1b}'));
            assert_eq!(
                fs::read_to_string(fixture.0.join("eska.toml")).unwrap(),
                input
            );
        }
    }
}

#[test]
/// Standalone application declarations and member-owned settings cannot be silently ignored.
fn application_settings_reject_the_wrong_manifest_scope() {
    let fixture = TestDir::new();
    fs::create_dir(fixture.0.join("src")).unwrap();
    fs::write(
        fixture.0.join("eska.toml"),
        "[project]\ntype = 'report'\n[workspace.members.app]\nconfiguration = 'base'",
    )
    .unwrap();
    for (language, expected) in [("ru", "[project]"), ("en", "[project]")] {
        let output = validate(&fixture.0, language);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8(output.stderr).unwrap().contains(expected));
    }
    member(&fixture.0, "base", "configuration");
    let path = fixture.0.join("base/eska.toml");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap() + "\n[analysis]\ntarget_platform_version = '8.3.27'\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("eska.toml"),
        "[workspace]\n[workspace.members.app]\nexternal_objects=['base']\n",
    )
    .unwrap();
    for (language, expected) in [
        ("ru", "задаются в корневом манифесте"),
        ("en", "belong in the root workspace manifest"),
    ] {
        let output = validate(&fixture.0, language);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8(output.stderr).unwrap().contains(expected));
    }
}
