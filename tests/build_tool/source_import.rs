//! Native reimport exercises actual standalone processes with independent ancestry and support pins.

use super::*;
use eska::project_core::{
    SourceFormat,
    config::ProjectConfig,
    source_format::{
        SourcePath, SourceReader, accept_source, capture_import_ancestor, import_designer_queries,
    },
};
use std::path::PathBuf;

/// Each test owns complete source, ancestry and native inputs without installed 1C or Git.
struct Fixture {
    test: TestDir,
    root: PathBuf,
    pin: String,
    ancestor: String,
    owner: PathBuf,
    extension: &'static str,
}

impl Fixture {
    /// The artifact's native descriptor defines type and UUID; its physical stem is unrelated.
    fn new(kind: &str, frozen: bool) -> Self {
        let test = TestDir::new();
        let root = test.0.join("project");
        fs::create_dir_all(root.join(".eska")).unwrap();
        let native = test.0.join("native");
        fs::create_dir(&native).unwrap();
        let (tag, extension, descriptor) = match kind {
            "extension" => ("Configuration", "cfe", "Configuration.xml"),
            "processing" => ("ExternalDataProcessor", "epf", "original.xml"),
            "report" => ("ExternalReport", "erf", "original.xml"),
            _ => ("Configuration", "cf", "Configuration.xml"),
        };
        let extra = if kind == "extension" {
            "<ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>"
        } else {
            ""
        };
        let version = if matches!(kind, "configuration" | "extension") {
            "<Version>1.2.3.4</Version>"
        } else {
            ""
        };
        let contents = format!(
            "\u{feff}<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'><{tag} uuid='10000000-0000-4000-8000-000000000001'><Properties><Name>Sample</Name><Comment>Original</Comment>{extra}{version}</Properties></{tag}></MetaDataObject>\r\n"
        );
        fs::write(native.join(descriptor), &contents).unwrap();
        if descriptor == "Configuration.xml" {
            fs::write(native.join("ConfigDumpInfo.xml"), "<ConfigDumpInfo/>").unwrap();
        }
        import_designer_queries(&native, &root.join("src")).unwrap();
        let reader = SourceReader::open(&root.join("src")).unwrap();
        let owner = reader
            .manifest()
            .files
            .iter()
            .find(|file| {
                matches!(
                    file.role,
                    eska::project_core::source_format::FileRole::Descriptor { .. }
                )
            })
            .unwrap()
            .source_path()
            .as_path()
            .to_owned();
        let freeze = if frozen {
            vec![
                eska::project_core::source_format::NativeUuid::parse(
                    "10000000-0000-4000-8000-000000000001",
                )
                .unwrap(),
            ]
        } else {
            Vec::new()
        };
        let pin = accept_source(&root.join("src"), &root.join(".eska/accepted"), &freeze)
            .unwrap()
            .sha256;
        let ancestor = capture_import_ancestor(&root.join("src"), &test.0.join("ancestor"))
            .unwrap()
            .sha256()
            .to_owned();
        let config = ProjectConfig::new(reader.project_type().unwrap())
            .with_name(ProjectName::parse("project".to_owned()).unwrap())
            .with_source_format(SourceFormat::Eska {
                accepted_base: SourcePath::parse(".eska/accepted").unwrap(),
            })
            .unwrap();
        fs::write(root.join("eska.toml"), config.to_toml().unwrap()).unwrap();
        fs::write(
            root.join(format!("Входящая.v2.{extension}")),
            contents.replace(">Original<", ">Incoming<"),
        )
        .unwrap();
        Self {
            test,
            root,
            pin,
            ancestor,
            owner,
            extension,
        }
    }

    /// The independent process receives no implicit ancestry, support approval or installed platform.
    fn command(&self, locale: &str) -> Command {
        self.with_ancestor(locale, &self.ancestor)
    }

    /// A repeat supplies the newly confirmed ancestry without duplicate command-line options.
    fn with_ancestor(&self, locale: &str, ancestor: &str) -> Command {
        let mut cmd = command(
            &self.root,
            locale,
            &[
                "import",
                &format!("Входящая.v2.{}", self.extension),
                "--format",
                "json",
                "--accepted-base-sha256",
                &self.pin,
                "--ancestor-sha256",
                ancestor,
                "--platform-version",
                "8.3.27.2325",
                "--ibcmd",
            ],
        );
        cmd.arg(ibcmd_fixture::executable());
        cmd
    }

    /// First imports use an explicit immutable store; repeats select the project-persisted snapshot.
    fn first(&self, locale: &str) -> Command {
        let mut cmd = self.command(locale);
        cmd.arg("--ancestor").arg(self.test.0.join("ancestor"));
        cmd
    }
}

/// Preview, exact-plan application and repeat preserve source bytes/settings and the original pin.
#[test]
fn previews_and_applies_all_native_profiles_with_independent_authorities() {
    for kind in ["configuration", "extension", "processing", "report"] {
        let fixture = Fixture::new(kind, false);
        let path = fixture.root.join("src").join(&fixture.owner);
        let original = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("{original}# local annotation\n")).unwrap();
        let before = fs::read(&path).unwrap();
        let settings = fs::read(fixture.root.join("eska.toml")).unwrap();
        let mut previews = Vec::new();
        for locale in ["ru", "en"] {
            let output = fixture.first(locale).output().unwrap();
            let value = report(&output, "success");
            assert!(output.stderr.is_empty(), "{output:?}");
            assert_eq!(value["command"], "import");
            assert_eq!(value["data"]["applied"], false);
            assert_eq!(value["data"]["can_apply"], true, "{kind}: {value}");
            assert_eq!(value["data"]["approved"]["sha256"], fixture.pin);
            assert_eq!(value["data"]["project_type"], kind);
            let support = value["data"]["support"].as_array().unwrap();
            assert_eq!(support.len(), 1);
            assert_eq!(support[0]["baseline"], fixture.pin);
            assert_eq!(support[0]["state"], "unrestricted");
            assert_eq!(support[0]["native_reason"], "ownObject");
            assert_eq!(support[0]["reason"], "allowed");
            assert_eq!(support[0]["allowed"], true);
            previews.push(value);
        }
        assert_eq!(previews[0], previews[1]);
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(!fixture.root.join(".eska/source-import/state.json").exists());
        let output = fixture
            .first("ru")
            .args([
                "--write",
                "--receipt",
                "recovery.json",
                "--expected-plan-sha256",
                previews[0]["data"]["transaction"]["sha256"]
                    .as_str()
                    .unwrap(),
            ])
            .output()
            .unwrap();
        let value = report(&output, "success");
        assert_eq!(value["data"]["applied"], true);
        assert!(fixture.root.join("recovery.json").is_file());
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("Incoming") && text.contains("# local annotation"));
        let next = value["data"]["resulting_ancestor"]["sha256"]
            .as_str()
            .unwrap();
        let repeat = fixture.with_ancestor("en", next).output().unwrap();
        let value = report(&repeat, "success");
        assert_eq!(
            value["data"]["working_revision"],
            value["data"]["output_revision"]
        );
        assert_eq!(value["data"]["unchanged"], true);
        let applied = fixture
            .with_ancestor("en", next)
            .args(["--write", "--receipt", "repeat-recovery.json"])
            .output()
            .unwrap();
        let value = report(&applied, "success");
        assert_eq!(value["data"]["publication"], "unchanged");
        assert_eq!(value["data"]["receipt"], Value::Null);
        assert!(!fixture.root.join("repeat-recovery.json").exists());
        assert_eq!(fs::read(fixture.root.join("eska.toml")).unwrap(), settings);
    }
}

/// Workspace selection mutates only the named member and retains exact inherited/root settings.
#[test]
fn reimports_only_the_selected_workspace_member() {
    let fixture = Fixture::new("configuration", false);
    let other = fixture.test.0.join("other");
    fs::create_dir_all(other.join("src")).unwrap();
    fs::write(
        other.join("eska.toml"),
        "[project]\nname='other'\ntype='configuration'\n",
    )
    .unwrap();
    fs::write(other.join("src/keep.yaml"), "unselected invalid source").unwrap();
    let settings = b"# root settings\n[workspace]\nmembers=['project','other']\n[build]\nplatform_version='8.3.27.2325'\n";
    fs::write(fixture.test.0.join("eska.toml"), settings).unwrap();
    eska::project_core::discovery::discover_context(&fixture.test.0).expect("workspace discovery");
    let output = fixture
        .first("en")
        .arg("--project-dir")
        .arg(&fixture.test.0)
        .args([
            "--project",
            "project",
            "--write",
            "--receipt",
            "recovery.json",
        ])
        .output()
        .unwrap();
    let value = report(&output, "success");
    assert_eq!(
        value["data"]["scope"],
        serde_json::json!({"kind":"member","project":"project"})
    );
    assert_eq!(
        fs::read(fixture.test.0.join("eska.toml")).unwrap(),
        settings
    );
    assert_eq!(
        fs::read_to_string(other.join("src/keep.yaml")).unwrap(),
        "unselected invalid source"
    );
    assert!(!other.join(".eska/source-import").exists());
}

/// Conflicts, denied rights and stale approval refuse write before creating a recovery file or guard.
#[test]
fn conflicts_and_stale_preview_never_select_bytes_or_start_publication() {
    for mode in ["conflict", "frozen", "stale"] {
        let fixture = Fixture::new("configuration", mode == "frozen");
        let path = fixture.root.join("src").join(&fixture.owner);
        if mode == "conflict" {
            let original = fs::read_to_string(&path).unwrap();
            fs::write(&path, original.replace("Original", "Local")).unwrap();
        }
        let before = fs::read(&path).unwrap();
        let mut cmd = fixture.first("en");
        cmd.args(["--write", "--receipt", "recovery.json"]);
        if mode == "stale" {
            cmd.args(["--expected-plan-sha256", &"f".repeat(64)]);
        }
        let value = report(&cmd.output().unwrap(), "error");
        assert_eq!(
            value["error"]["code"],
            if mode == "stale" {
                "source_input_changed"
            } else {
                "source_import_conflicts"
            }
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(!fixture.root.join("recovery.json").exists());
        assert!(
            !fixture
                .root
                .join(".eska-project-source-import.json")
                .exists()
        );
    }
}

/// Recovery-file collisions and missing independent pins are preflight errors before native export.
#[test]
fn invalid_receipt_and_ancestry_are_rejected_before_platform_work() {
    for mode in ["source", "ancestor", "existing", "missing"] {
        let fixture = Fixture::new("configuration", false);
        let mut cmd = fixture.first("en");
        let destination = match mode {
            "source" => fixture.root.join("src/recovery.json"),
            "ancestor" => fixture.test.0.join("ancestor/recovery.json"),
            "existing" => {
                fs::write(fixture.root.join("recovery.json"), "keep").unwrap();
                fixture.root.join("recovery.json")
            }
            _ => fixture.root.join("recovery.json"),
        };
        cmd.arg("--write");
        if mode != "missing" {
            cmd.arg("--receipt").arg(destination);
        }
        let value = report(&cmd.output().unwrap(), "error");
        assert_eq!(
            value["error"]["code"],
            match mode {
                "existing" => "source_destination_exists",
                "missing" => "arguments_invalid",
                _ => "source_import_receipt_path_invalid",
            }
        );
        assert!(!fixture.root.join("platform.log").exists());
    }
}

/// Native failure, another artifact kind and root UUID mismatch never publish source or a guard.
#[test]
fn native_failures_and_wrong_identity_preserve_the_selected_source() {
    for mode in ["type", "uuid", "platform"] {
        let fixture = Fixture::new("configuration", false);
        let input = fixture.root.join("Входящая.v2.cf");
        let contents = fs::read_to_string(&input).unwrap();
        if mode == "type" {
            fs::write(&input, contents.replace("<Properties>", "<Properties><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>")).unwrap();
        } else if mode == "uuid" {
            fs::write(
                &input,
                contents.replace(
                    "10000000-0000-4000-8000-000000000001",
                    "90000000-0000-4000-8000-000000000001",
                ),
            )
            .unwrap();
        }
        let before = fs::read(fixture.root.join("src").join(&fixture.owner)).unwrap();
        let mut cmd = fixture.first("ru");
        cmd.args(["--write", "--receipt", "recovery.json"]);
        if mode == "platform" {
            cmd.env("FAKE_IBCMD_FAIL_EXPORT", "1");
        }
        let value = report(&cmd.output().unwrap(), "error");
        assert_eq!(
            value["error"]["code"],
            match mode {
                "type" => "source_import_profile_mismatch",
                "uuid" => "source_identity_mismatch",
                _ => "platform_command_failed",
            }
        );
        assert_eq!(
            fs::read(fixture.root.join("src").join(&fixture.owner)).unwrap(),
            before
        );
        assert!(!fixture.root.join("recovery.json").exists());
        assert!(
            !fixture
                .root
                .join(".eska-project-source-import.json")
                .exists()
        );
    }
}

/// SIGINT during the native process cancels before source publication and retains every input byte.
#[cfg(unix)]
#[test]
fn cancellation_during_native_export_keeps_source_and_recovery_state_untouched() {
    use std::{
        process::Stdio,
        thread,
        time::{Duration, Instant},
    };
    let fixture = Fixture::new("configuration", false);
    let path = fixture.root.join("src").join(&fixture.owner);
    let before = fs::read(&path).unwrap();
    let ready = fixture.root.join("native-ready");
    let proceed = fixture.root.join("native-continue");
    let mut child = fixture
        .first("en")
        .args(["--write", "--receipt", "recovery.json"])
        .env("FAKE_IBCMD_IMPORT_READY", &ready)
        .env("FAKE_IBCMD_IMPORT_CONTINUE", &proceed)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "native process exited before rendezvous"
        );
        assert!(
            Instant::now() < deadline,
            "native process did not reach rendezvous"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    thread::sleep(Duration::from_millis(100));
    fs::write(&proceed, "").unwrap();
    let value = report(&child.wait_with_output().unwrap(), "error");
    assert_eq!(value["error"]["code"], "platform_interrupted");
    assert_eq!(fs::read(path).unwrap(), before);
    assert!(!fixture.root.join("recovery.json").exists());
    assert!(
        !fixture
            .root
            .join(".eska-project-source-import.json")
            .exists()
    );
}
