//! Actual unpack/import compounds preserve native metadata identity, version and physical aliases.

use super::{
    fixture::{Fixture, success},
    guarded::wrapper,
};
use crate::query_creation;
use serde_json::Value;
use std::{fs, process::Command};

/// Native process options are explicit and shared between the shell preflight and build companion.
fn platform(command: &mut Command, fixture: &Fixture, native: &query_creation::Native) {
    command
        .env("FAKE_IBCMD_NATIVE_TREE", &native.input)
        .env("FAKE_IBCMD_LOG", fixture.0.0.join("native.log"))
        .args(["--platform-version", "8.3.27.2325", "--ibcmd"])
        .arg(crate::cli::build::fake_ibcmd(&fixture.0));
}

#[test]
/// The same actual native build/source tools handle all four profiles in both locales.
fn native_profiles_preserve_original_name_version_and_own_stage_cleanup() {
    for kind in query_creation::TYPES {
        for locale in ["ru", "en"] {
            let fixture = Fixture::new();
            fixture.project_tool();
            fixture.build_tool();
            let native = query_creation::native(&fixture.0.0, kind);
            let suffix = match kind {
                "configuration" => "cf",
                "extension" => "cfe",
                "processing" => "epf",
                _ => "erf",
            };
            let input = format!("artifact.alias.{suffix}");
            fs::copy(native.input.join(&native.root), fixture.0.0.join(&input)).unwrap();
            let before = query_creation::snapshot(&native.input);
            let mut command = fixture.command(locale);
            command.args([
                "new",
                "different-label",
                "--source-format",
                "eska",
                "--from",
                &input,
                "--workflow",
                "trunk",
                "--no-vcs",
                "--format",
                "json",
            ]);
            platform(&mut command, &fixture, &native);
            let data = success(&command.output().unwrap());
            assert_eq!(data["type"], kind);
            assert_eq!(data["metadata_name"], "NativeName");
            assert_eq!(data["version"], "1.2.3");
            assert_eq!(data["support_approved"], false);
            assert_eq!(data["name"], "different-label");
            let root = fixture.0.0.join("different-label");
            assert_eq!(
                fs::read(root.join("src").join(native.module_source)).unwrap(),
                fs::read(native.input.join(&native.module)).unwrap()
            );
            assert_eq!(query_creation::snapshot(&native.input), before);
            let storage = fixture.0.0.join(".eska/import");
            assert_eq!(
                storage
                    .read_dir()
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect::<Vec<_>>(),
                [std::ffi::OsString::from(".gitignore")]
            );
        }
    }
}

#[test]
/// Native versions remain unchanged and a mismatched member scheme fails before project publication.
fn native_member_scheme_mismatch_retains_only_private_unpacked_input() {
    let fixture = Fixture::new();
    fixture.project_tool();
    fixture.build_tool();
    let native = query_creation::native(&fixture.0.0, "processing");
    fs::copy(
        native.input.join(&native.root),
        fixture.0.0.join("incoming.epf"),
    )
    .unwrap();
    let original = "[workspace]\nmembers=[]\n[version]\nscheme='x.y.z.build'\n";
    fs::write(fixture.0.0.join("eska.toml"), original).unwrap();
    let mut command = fixture.command("en");
    command.args([
        "new",
        "member",
        "--source-format",
        "eska",
        "--from",
        "incoming.epf",
        "--format",
        "json",
    ]);
    platform(&mut command, &fixture, &native);
    let output = command.output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "new-version-scheme-mismatch");
    assert!(!fixture.0.0.join("src/member").exists());
    assert_eq!(
        fs::read_to_string(fixture.0.0.join("eska.toml")).unwrap(),
        original
    );
    let stage = std::path::Path::new(value["diagnostics"]["native_staging"].as_str().unwrap());
    assert!(stage.join("native").is_dir());
}

#[test]
/// A completed project cannot authorize recursive cleanup of intervening native staging data.
fn intervening_native_stage_files_are_retained_after_successful_project_onboarding() {
    let fixture = Fixture::new();
    fixture.guarded_tool();
    fixture.build_tool();
    let native = query_creation::native(&fixture.0.0, "configuration");
    fs::copy(
        native.input.join(&native.root),
        fixture.0.0.join("incoming.cf"),
    )
    .unwrap();
    let mut command = wrapper(&fixture, "en", "stage");
    command.args([
        "new",
        "project",
        "--source-format",
        "eska",
        "--from",
        "incoming.cf",
        "--workflow",
        "trunk",
        "--no-vcs",
        "--format",
        "json",
    ]);
    platform(&mut command, &fixture, &native);
    let output = command.output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "new-native-staging-changed");
    assert_eq!(value["diagnostics"]["onboarding_complete"], true);
    assert_eq!(value["diagnostics"]["source_verified"], true);
    assert!(fixture.0.0.join("project/eska.toml").is_file());
    let stage = std::path::Path::new(value["diagnostics"]["native_staging"].as_str().unwrap());
    assert_eq!(
        fs::read(stage.join("intervening.txt")).unwrap(),
        b"retain staging edit"
    );
}
