//! Real source/native companions prove default preview, explicit exact-plan write and confirmed repeat.

use super::fixture::Fixture;
use crate::query_creation;
use eska::project_core::{
    ProjectName,
    creation::{ImportedProject, import_query_project},
};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

struct NativeImport {
    fixture: Fixture,
    created: ImportedProject,
    native: query_creation::Native,
    artifact: PathBuf,
}

impl NativeImport {
    /// The source candidate/history are offline; only a later child owns native staging/publication.
    fn new(kind: &str) -> Self {
        let fixture = Fixture::new();
        let native = query_creation::native(&fixture.0.0, kind);
        if matches!(kind, "configuration" | "extension") {
            fs::write(native.input.join("ConfigDumpInfo.xml"), "<ConfigDumpInfo/>").unwrap();
        }
        let created = import_query_project(
            &native.input,
            &fixture.0.0.join("project"),
            ProjectName::parse("demo".to_owned()).unwrap(),
            None,
            &[],
        )
        .unwrap();
        let destination = fixture
            .0
            .0
            .join(format!("bin/eska-build{}", std::env::consts::EXE_SUFFIX));
        fs::hard_link(env!("CARGO_BIN_EXE_eska-build"), &destination)
            .or_else(|_| fs::copy(env!("CARGO_BIN_EXE_eska-build"), &destination).map(|_| ()))
            .unwrap();
        let descriptor = native.input.join(&native.root);
        fs::write(
            &descriptor,
            fs::read_to_string(&descriptor)
                .unwrap()
                .replace(">Accepted<", ">Incoming<"),
        )
        .unwrap();
        let suffix = match kind {
            "configuration" => "cf",
            "extension" => "cfe",
            "processing" => "epf",
            _ => "erf",
        };
        let artifact = created
            .project
            .root()
            .join(format!("unrelated.physical.alias.{suffix}"));
        fs::copy(&descriptor, &artifact).unwrap();
        Self {
            fixture,
            created,
            native,
            artifact,
        }
    }

    /// All trust is explicitly retained by this test caller; the child resolves native machine options.
    fn command(&self, locale: &str, ancestor: &str) -> Command {
        let mut command = self.fixture.command(locale);
        command
            .arg("--project-dir")
            .arg(self.created.project.root())
            .arg("import")
            .arg(&self.artifact)
            .args([
                "--accepted-base-sha256",
                &self.created.accepted.sha256,
                "--ancestor-sha256",
                ancestor,
                "--platform-version",
                "8.3.27.2325",
                "--ibcmd",
            ])
            .arg(crate::cli::build::fake_ibcmd(&self.fixture.0))
            .env("FAKE_IBCMD_NATIVE_TREE", &self.native.input)
            .env("FAKE_IBCMD_LOG", self.fixture.0.0.join("platform.log"))
            .args(["--format", "json"]);
        command
    }
}

/// Return the child's existing stable import envelope, independent from the facade locale.
fn success(output: &Output) -> Value {
    assert!(output.status.success(), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["tool"], "eska-build");
    assert_eq!(value["command"], "import");
    assert_eq!(value["status"], "success");
    value["data"].clone()
}

#[test]
/// Four actual child profiles preserve local BSL, old approval/settings/artifact and exact-plan semantics.
fn real_companion_previews_writes_and_repeats_four_profiles_in_both_locales() {
    for kind in query_creation::TYPES {
        for locale in ["ru", "en"] {
            let test = NativeImport::new(kind);
            let root = test.created.project.root();
            let initial = test.created.initial_ancestor.as_ref().unwrap().sha256();
            let settings = fs::read(root.join("eska.toml")).unwrap();
            let approved = query_creation::snapshot(&root.join(".eska/accepted"));
            let artifact = fs::read(&test.artifact).unwrap();
            let module = root.join("src").join(test.native.module_source);
            let mut local = fs::read(&module).unwrap();
            local.extend_from_slice(b"// local facade module edit\r\n");
            fs::write(&module, &local).unwrap();
            let before = query_creation::snapshot(&root.join("src"));
            let history = query_creation::snapshot(&root.join(".eska/source-import"));
            let preview = success(&test.command(locale, initial).output().unwrap());
            assert_eq!(preview["applied"], false);
            assert_eq!(preview["can_apply"], true);
            assert_eq!(preview["unchanged"], false);
            assert_eq!(preview["project_type"], kind);
            assert_eq!(query_creation::snapshot(&root.join("src")), before);
            assert_eq!(
                query_creation::snapshot(&root.join(".eska/source-import")),
                history
            );
            let written = success(
                &test
                    .command(locale, initial)
                    .args([
                        "--expected-plan-sha256",
                        preview["transaction"]["sha256"].as_str().unwrap(),
                        "--write",
                        "--receipt",
                        "recovery.json",
                    ])
                    .output()
                    .unwrap(),
            );
            assert_eq!(written["applied"], true);
            assert!(root.join("recovery.json").is_file());
            assert_eq!(fs::read(&module).unwrap(), local);
            assert_eq!(fs::read(root.join("eska.toml")).unwrap(), settings);
            assert_eq!(
                query_creation::snapshot(&root.join(".eska/accepted")),
                approved
            );
            assert_eq!(fs::read(&test.artifact).unwrap(), artifact);
            assert!(!root.join(".eska-project-source-import.json").exists());
            let after = query_creation::snapshot(&root.join("src"));
            let next = written["resulting_ancestor"]["sha256"].as_str().unwrap();
            let repeated = success(
                &test
                    .command(locale, next)
                    .args(["--write", "--receipt", "repeat-recovery.json"])
                    .output()
                    .unwrap(),
            );
            assert_eq!(repeated["unchanged"], true);
            assert_eq!(repeated["publication"], "unchanged");
            assert!(repeated["receipt"].is_null());
            assert!(!root.join("repeat-recovery.json").exists());
            assert_eq!(query_creation::snapshot(&root.join("src")), after);
        }
    }
}

#[test]
/// A stale independently retained preview reference cannot publish a receipt or replace local source.
fn changed_expected_plan_refuses_before_source_publication() {
    let test = NativeImport::new("configuration");
    let root = test.created.project.root();
    let before = query_creation::snapshot(&root.join("src"));
    let output = test
        .command(
            "en",
            test.created.initial_ancestor.as_ref().unwrap().sha256(),
        )
        .args([
            "--expected-plan-sha256",
            &"a".repeat(64),
            "--write",
            "--receipt",
            "recovery.json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["tool"], "eska-build");
    assert_eq!(value["error"]["code"], "source_input_changed");
    assert!(!root.join("recovery.json").exists());
    assert!(!root.join(".eska-project-source-import.json").exists());
    assert_eq!(query_creation::snapshot(&root.join("src")), before);
}

#[test]
/// A selected member uses inherited platform policy and leaves its sibling and root settings intact.
fn real_member_import_inherits_platform_and_changes_only_the_selected_source() {
    let test = NativeImport::new("processing");
    let workspace = &test.fixture.0.0;
    fs::create_dir_all(workspace.join("sibling/src")).unwrap();
    fs::write(
        workspace.join("sibling/eska.toml"),
        "[project]\nname='sibling'\ntype='report'\n",
    )
    .unwrap();
    fs::write(
        workspace.join("sibling/src/user.txt"),
        b"retain sibling source",
    )
    .unwrap();
    let settings = "# preserve workspace\n[workspace]\nmembers=['project','sibling']\n[build]\nplatform_version='8.3.27.2325'\n";
    fs::write(workspace.join("eska.toml"), settings).unwrap();
    let sibling = query_creation::snapshot(&workspace.join("sibling"));
    let initial = test.created.initial_ancestor.as_ref().unwrap().sha256();
    let command = || {
        let mut command = test.fixture.command("en");
        command
            .arg("import")
            .arg(&test.artifact)
            .args([
                "-p",
                "demo",
                "--accepted-base-sha256",
                &test.created.accepted.sha256,
                "--ancestor-sha256",
                initial,
                "--format",
                "json",
                "--ibcmd",
            ])
            .arg(crate::cli::build::fake_ibcmd(&test.fixture.0))
            .env("FAKE_IBCMD_NATIVE_TREE", &test.native.input);
        command
    };
    let preview = success(&command().output().unwrap());
    assert_eq!(preview["scope"]["kind"], "member");
    assert_eq!(preview["scope"]["project"], "demo");
    assert_eq!(
        preview["project"],
        test.created.project.root().to_str().unwrap()
    );
    let written = success(
        &command()
            .args([
                "--write",
                "--receipt",
                "member-recovery.json",
                "--expected-plan-sha256",
                preview["transaction"]["sha256"].as_str().unwrap(),
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(written["applied"], true);
    assert!(workspace.join("member-recovery.json").is_file());
    assert_eq!(
        fs::read_to_string(workspace.join("eska.toml")).unwrap(),
        settings
    );
    assert_eq!(
        query_creation::snapshot(&workspace.join("sibling")),
        sibling
    );
}
