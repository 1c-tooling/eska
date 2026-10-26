//! Configuration transactions use tiny owned Designer exports and real cross-process file leases.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

use eska_analyzer::{
    Error,
    configuration::{self, Status},
    migration,
    process::Analyzer,
};
use eska_project_core::{
    config::{ProjectConfig, WorkspaceConfig},
    project::{ProjectType, create},
};

#[path = "../../../tests/support/mod.rs"]
mod support;
use support::TestDir;

/// Create only source descriptors needed by context resolution and check-config.
fn project(root: &Path, kind: ProjectType) {
    create::create(root, &ProjectConfig::new(kind)).unwrap();
    let (tag, extra, name) = match kind {
        ProjectType::Configuration => (
            "Configuration",
            "<CompatibilityMode>Version8_3_27</CompatibilityMode>",
            "Configuration.xml",
        ),
        ProjectType::Extension => (
            "Configuration",
            "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose><ConfigurationExtensionCompatibilityMode>Version8_3_17</ConfigurationExtensionCompatibilityMode>",
            "Configuration.xml",
        ),
        ProjectType::Processing => ("ExternalDataProcessor", "", "Обработка.xml"),
        ProjectType::Report => ("ExternalReport", "", "Отчёт.xml"),
    };
    fs::write(root.join("src").join(name),format!("\u{feff}<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.17\"><{tag} uuid=\"12345678-1234-1234-1234-123456789012\"><Properties><Name>Пример</Name><Version>1.2.3.4</Version>{extra}</Properties><ChildObjects/></{tag}></MetaDataObject>\r\n")).unwrap();
    if matches!(kind, ProjectType::Processing | ProjectType::Report) {
        fs::create_dir(root.join("src").join(Path::new(name).file_stem().unwrap())).unwrap();
    }
}

/// Model executable failures separately from actual upstream validation, tested explicitly below.
#[cfg(unix)]
fn fake_analyzer(root: &Path) -> PathBuf {
    let path = root.join("analyzer fixture");
    // Keep the executable inode immutable while parallel tests spawn child processes.
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/analyzer-config.py");
    std::os::unix::fs::symlink(fixture, &path).unwrap();
    path
}

/// Freshness and generation remain read-only and do not auto-include neighboring extensions.
#[test]
fn inspection_is_deterministic_and_policy_ownership_is_strict() {
    let fixture = TestDir::new();
    let root = fixture.0.join("Проект с пробелами");
    project(&root, ProjectType::Configuration);
    let manifest = root.join("eska.toml");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.push_str("\n[tools.bsl-analyzer.formatting]\nuse_tabs=true\nindent_size=2\n[tools.bsl-analyzer.diagnostics.parameters]\nLineLength=false\n");
    fs::write(&manifest, &text).unwrap();
    let first = configuration::inspect(&root, None).unwrap();
    let second = configuration::inspect(&root, None).unwrap();
    assert_eq!(first.contents, second.contents);
    assert_eq!(first.token, second.token);
    assert_eq!(first.status, Status::Missing);
    assert!(!root.join(".eska").exists());
    assert!(!first.path().exists());
    let generated = String::from_utf8(first.contents).unwrap();
    assert!(generated.contains("externals = []"));
    assert!(generated.contains("extensions = []"));
    assert!(generated.contains("Version8_3_27"));
    assert!(generated.contains("LineLength = false"));
    let config = ProjectConfig::from_toml(&text).unwrap();
    let roundtrip = config.to_toml().unwrap();
    assert_eq!(ProjectConfig::from_toml(&roundtrip).unwrap(), config);
    for policy in [
        "target_platform_version='8.3.27'",
        "compatibility_mode='DontUse'",
        "source.root='other'",
        "source.extensions=[]",
        "source.externals=[]",
        "source.configuration.id='other'",
    ] {
        assert!(
            ProjectConfig::from_toml(&format!(
                "[project]\ntype='configuration'\n[tools.bsl-analyzer]\n{policy}\n"
            ))
            .is_err()
        );
    }
    let workspace = WorkspaceConfig::from_toml(
        "[workspace]\nmembers={}\n[tools.bsl-analyzer.output]\ndisplay_language='en'\n",
    )
    .unwrap();
    assert_eq!(
        WorkspaceConfig::from_toml(&workspace.to_toml().unwrap()).unwrap(),
        workspace
    );
}

/// Explicit reference XML outside the project is fingerprinted and policy changes regenerate it.
#[cfg(unix)]
#[test]
fn reference_context_regenerates_from_policy_and_external_descriptor_changes() {
    let fixture = TestDir::new();
    let root = fixture.0.join("обработка");
    let base = fixture.0.join("база БСП\\test");
    project(&root, ProjectType::Processing);
    project(&base, ProjectType::Configuration);
    fs::remove_file(base.join("eska.toml")).unwrap();
    let manifest = root.join("eska.toml");
    let text = fs::read_to_string(&manifest).unwrap()
        + "\n[analysis.configuration]\nsource='../база БСП\\test/src'\n[tools.bsl-analyzer.diagnostics.parameters]\nLineLength=false\n";
    fs::write(&manifest, &text).unwrap();
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: fake_analyzer(&fixture.0),
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let plan = configuration::inspect(&root, None).unwrap();
    let current = configuration::prepare(&plan, &analyzer, false).unwrap();
    let generated: toml::Value =
        toml::from_str(std::str::from_utf8(&current.contents).unwrap()).unwrap();
    assert_eq!(
        generated["source"]["root"].as_str().unwrap(),
        base.join("src").to_str().unwrap()
    );
    assert_eq!(
        generated["diagnostics"]["parameters"]["LineLength"].as_bool(),
        Some(false)
    );
    fs::write(
        &manifest,
        text + "\n[tools.bsl-analyzer.formatting]\nindent_size=2\n",
    )
    .unwrap();
    let stale = configuration::inspect(&root, None).unwrap();
    assert_eq!(stale.status, Status::Stale);
    let updated = configuration::prepare(&stale, &analyzer, false).unwrap();
    assert_eq!(updated.status, Status::Current);
    assert!(
        std::str::from_utf8(&updated.contents)
            .unwrap()
            .contains("LineLength = false")
    );
    let descriptor = base.join("src/Configuration.xml");
    fs::write(
        &descriptor,
        fs::read_to_string(&descriptor).unwrap() + "<!-- reference changed -->",
    )
    .unwrap();
    assert_eq!(
        configuration::inspect(&root, None).unwrap().status,
        Status::Stale
    );
    assert!(!base.join("bsl-analyzer.toml").exists());
    assert!(!base.join(".eska").exists());
}

/// The published analyzer, rather than a fixture process, validates the absolute reference topology.
#[test]
#[ignore = "requires ESKA_BSL_ANALYZER pointing to published bsl-analyzer-app 0.4.0"]
fn published_analyzer_accepts_external_configuration_reference() {
    let fixture = TestDir::new();
    let root = fixture.0.join("report");
    let reference = fixture.0.join("База с БСП");
    project(&root, ProjectType::Report);
    project(&reference, ProjectType::Configuration);
    fs::remove_file(reference.join("eska.toml")).unwrap();
    let manifest = root.join("eska.toml");
    fs::write(&manifest, fs::read_to_string(&manifest).unwrap() + "\n[analysis.configuration]\nsource='../База с БСП/src'\n[tools.bsl-analyzer.diagnostics.parameters]\nLineLength=false\n").unwrap();
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: std::env::var_os("ESKA_BSL_ANALYZER")
            .map(PathBuf::from)
            .expect("explicit upstream executable"),
        timeout: Duration::from_secs(60),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let plan = configuration::inspect(&root, None).unwrap();
    let current = configuration::prepare(&plan, &analyzer, false).unwrap();
    assert_eq!(current.status, Status::Current);
    assert!(!current.context.limited_context());
    assert!(!reference.join("bsl-analyzer.toml").exists());
}

/// Prepare is idempotent, manual edits conflict and a changed context cannot reuse the old config.
#[cfg(unix)]
#[test]
fn prepare_checks_freshness_ownership_and_actual_inputs() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: fake_analyzer(&fixture.0),
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let plan = configuration::inspect(&root, None).unwrap();
    let current = configuration::prepare(&plan, &analyzer, false).unwrap();
    assert_eq!(current.status, Status::Current);
    let before = fs::metadata(current.path()).unwrap().modified().unwrap();
    configuration::prepare(&current, &analyzer, false).unwrap();
    assert_eq!(
        fs::metadata(current.path()).unwrap().modified().unwrap(),
        before
    );
    let mut manifest = fs::read_to_string(root.join("eska.toml")).unwrap();
    manifest.push_str("\n[tools.bsl-analyzer.formatting]\nindent_size=2\n");
    fs::write(root.join("eska.toml"), manifest).unwrap();
    assert!(matches!(
        configuration::lease(&current),
        Err(Error::StalePreview)
    ));
    let stale = configuration::inspect(&root, None).unwrap();
    assert_eq!(stale.status, Status::Stale);
    configuration::prepare(&stale, &analyzer, false).unwrap();
    let owned = fs::read(stale.path()).unwrap();
    fs::write(stale.path(), [owned.as_slice(), b"\n# manual\n"].concat()).unwrap();
    let conflict = configuration::inspect(&root, None).unwrap();
    assert_eq!(conflict.status, Status::ManagedConflict);
    assert!(matches!(
        configuration::prepare(&conflict, &analyzer, false),
        Err(Error::ManagedConflict { .. })
    ));
    assert!(fs::read(conflict.path()).unwrap().ends_with(b"# manual\n"));
}

/// Failure, timeout, cancellation and source races never replace the prior valid config.
#[cfg(unix)]
#[test]
fn failed_validation_and_cancellation_preserve_previous_files() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let binary = fake_analyzer(&fixture.0);
    let cancelled = AtomicBool::new(false);
    let mut analyzer = Analyzer {
        binary,
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let plan = configuration::inspect(&root, None).unwrap();
    configuration::prepare(&plan, &analyzer, false).unwrap();
    let config = fs::read(plan.path()).unwrap();
    let mut manifest = fs::read_to_string(root.join("eska.toml")).unwrap();
    manifest.push_str("\n# new input\n");
    fs::write(root.join("eska.toml"), &manifest).unwrap();
    for (flag, expected) in [
        ("reject", "check_config"),
        ("sleep", "timeout"),
        ("mutate", "stale_preview"),
    ] {
        fs::write(root.join(flag), b"").unwrap();
        let plan = configuration::inspect(&root, None).unwrap();
        analyzer.timeout = if flag == "sleep" {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(5)
        };
        let error = configuration::prepare(&plan, &analyzer, false).unwrap_err();
        assert_eq!(error.code(), expected);
        assert_eq!(fs::read(plan.path()).unwrap(), config);
        fs::remove_file(root.join(flag)).unwrap();
        if flag == "mutate" {
            assert!(
                fs::read_to_string(root.join("eska.toml"))
                    .unwrap()
                    .contains("# concurrent change")
            );
        }
    }
    let plan = configuration::inspect(&root, None).unwrap();
    cancelled.store(true, std::sync::atomic::Ordering::Release);
    assert!(matches!(
        configuration::prepare(&plan, &analyzer, false),
        Err(Error::Cancelled)
    ));
    assert_eq!(fs::read(plan.path()).unwrap(), config);
    assert!(!fs::read_dir(&root).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".eska-analyzer-")
    }));
    cancelled.store(false, std::sync::atomic::Ordering::Release);
    fs::write(root.join("sleep"), b"").unwrap();
    let plan = configuration::inspect(&root, None).unwrap();
    std::thread::scope(|scope| {
        let result = scope.spawn(|| configuration::prepare(&plan, &analyzer, false));
        std::thread::sleep(Duration::from_millis(100));
        cancelled.store(true, std::sync::atomic::Ordering::Release);
        assert!(matches!(result.join().unwrap(), Err(Error::Cancelled)));
    });
    assert_eq!(fs::read(plan.path()).unwrap(), config);
}

/// Every policy section is preserved; applying a stale preview cannot modify either config.
#[cfg(unix)]
#[test]
fn migration_requires_exact_preview_and_keeps_all_policy_and_original_bytes() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let original=b"# original\r\n[source]\r\nroot='src'\r\nexclude=['**/generated/**']\r\n[diagnostics.parameters]\r\nLineLength=false\r\n[formatting]\r\nuse_tabs=true\r\nindent_size=2\r\n[code_lens]\r\nshow_cognitive_complexity=true\r\n[analysis]\r\nignored_authors=['Vendor']\r\n[output]\r\ndisplay_language='en'\r\n";
    fs::write(root.join("bsl-analyzer.toml"), original).unwrap();
    let original_manifest = fs::read(root.join("eska.toml")).unwrap();
    let preview = migration::preview(&root, None).unwrap();
    assert_eq!(preview.conflicts, [] as [String; 0]);
    assert_eq!(
        preview.policy["formatting"]["indent_size"].as_integer(),
        Some(2)
    );
    assert!(!preview.policy.contains_key("target_platform_version"));
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: fake_analyzer(&fixture.0),
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
        connection_closed: None,
    };
    migration::validate_preview(&preview, &analyzer).unwrap();
    assert_eq!(fs::read(root.join("eska.toml")).unwrap(), original_manifest);
    assert_eq!(fs::read(root.join("bsl-analyzer.toml")).unwrap(), original);
    assert!(matches!(
        migration::apply(&preview, "wrong", &analyzer),
        Err(Error::StalePreview)
    ));
    let mut edited = original_manifest;
    edited.extend_from_slice(b"\n# user\n");
    fs::write(root.join("eska.toml"), &edited).unwrap();
    assert!(matches!(
        migration::apply(&preview, &preview.token, &analyzer),
        Err(Error::StalePreview)
    ));
    let preview = migration::preview(&root, None).unwrap();
    let (current, backup) = migration::apply(&preview, &preview.token, &analyzer).unwrap();
    assert_eq!(current.status, Status::Current);
    assert_eq!(fs::read(backup).unwrap(), original);
    let manifest = fs::read_to_string(root.join("eska.toml")).unwrap();
    assert!(manifest.contains("# user"));
    assert!(manifest.contains("[tools.bsl-analyzer.formatting]"));
    let policy = ProjectConfig::from_toml(&manifest).unwrap();
    let expected: toml::Table = toml::from_str(
        policy
            .configuration()
            .analysis_settings()
            .tools()
            .bsl_analyzer
            .as_toml(),
    )
    .unwrap();
    assert_eq!(expected, preview.policy);
}

/// Mismatched managed fields and conflicting ESKA policy remain explicit preview decisions.
#[test]
fn migration_does_not_discard_paths_targets_or_conflicting_policy() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let manifest = root.join("eska.toml");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.push_str("\n[tools.bsl-analyzer.formatting]\nindent_size=2\n");
    fs::write(&manifest, text).unwrap();
    fs::write(
        root.join("bsl-analyzer.toml"),
        "target_platform_version='8.3.17'\n[source]\nroot='wrong'\n[formatting]\nindent_size=4\n",
    )
    .unwrap();
    let preview = migration::preview(&root, None).unwrap();
    assert_eq!(
        preview.conflicts,
        vec![
            "source.root",
            "target_platform_version",
            "tools.bsl-analyzer.formatting.indent_size"
        ]
    );
    assert_eq!(
        preview.policy["formatting"]["indent_size"].as_integer(),
        Some(2)
    );
}

/// A selected unsupported release cannot validate or replace a config even if it exits successfully.
#[cfg(unix)]
#[test]
fn unsupported_release_preserves_existing_config() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: fake_analyzer(&fixture.0),
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let plan = configuration::inspect(&root, None).unwrap();
    let plan = configuration::prepare(&plan, &analyzer, false).unwrap();
    let original = fs::read(plan.path()).unwrap();
    fs::write(root.join("version"), b"0.3.1").unwrap();
    let result = configuration::prepare(&plan, &analyzer, false);
    assert!(
        matches!(result, Err(Error::AnalyzerVersion { .. })),
        "{result:?}"
    );
    assert_eq!(fs::read(plan.path()).unwrap(), original);
}

/// A second editor can reuse the same context but cannot publish a different one under a live lease.
#[cfg(unix)]
#[test]
fn active_context_is_protected_across_processes() {
    let fixture = TestDir::new();
    let root = fixture.0.join("workspace");
    fs::create_dir(&root).unwrap();
    for name in ["a", "b"] {
        let member = root.join(name);
        project(&member, ProjectType::Configuration);
        fs::write(
            member.join("eska.toml"),
            format!("[project]\nname='{name}'\ntype='configuration'\n"),
        )
        .unwrap();
    }
    fs::write(root.join("eska.toml"),"[workspace]\n[workspace.members.one]\nconfiguration='a'\n[workspace.members.two]\nconfiguration='b'\n").unwrap();
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: fake_analyzer(&fixture.0),
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let first = configuration::inspect(&root, Some("one")).unwrap();
    let first = configuration::prepare(&first, &analyzer, false).unwrap();
    let original = fs::read(first.path()).unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lease_child", "--nocapture"])
        .env("ESKA_TEST_LEASE_ROOT", &root)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !root.join("lease-ready").exists() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    configuration::prepare(&first, &analyzer, false).unwrap();
    let second = configuration::inspect(&root, Some("two")).unwrap();
    assert_eq!(second.status, Status::ContextConflict);
    assert!(matches!(
        configuration::prepare(&second, &analyzer, false),
        Err(Error::ContextConflict { .. })
    ));
    assert!(matches!(
        configuration::prepare(&second, &analyzer, true),
        Err(Error::Busy { .. })
    ));
    assert_eq!(fs::read(first.path()).unwrap(), original);
    fs::write(root.join("release-lease"), b"").unwrap();
    assert!(child.wait().unwrap().success());
    let second = configuration::prepare(&second, &analyzer, true).unwrap();
    assert_eq!(second.status, Status::Current);
    assert_eq!(second.ownership.context.as_deref(), Some("two"));
}

/// Acquisition reads current inputs under its lease and releases failed attempts before later preparation.
#[cfg(unix)]
#[test]
fn leased_inspection_checks_fresh_inputs_and_preserves_config_bytes() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: fake_analyzer(&fixture.0),
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let first = configuration::inspect(&root, None).unwrap();
    assert!(matches!(
        configuration::inspect_leased(&root, None),
        Err(Error::StalePreview)
    ));
    let prepared = configuration::prepare(&first, &analyzer, false).unwrap();
    let original = fs::read(prepared.path()).unwrap();
    let (current, lease) = configuration::inspect_leased(&root.join("src"), None).unwrap();
    assert_eq!(current.contents, prepared.contents);
    assert_eq!(current.token, prepared.token);
    assert_eq!(current.context.root(), prepared.context.root());
    assert_eq!(fs::read(current.path()).unwrap(), original);
    drop(lease);

    let descriptor = root.join("src/Configuration.xml");
    let text = fs::read_to_string(&descriptor).unwrap();
    fs::write(&descriptor, text.replace("Version8_3_27", "Version8_3_17")).unwrap();
    assert!(matches!(
        configuration::inspect_leased(&root, None),
        Err(Error::StalePreview)
    ));
    let stale = configuration::inspect(&root, None).unwrap();
    let updated = configuration::prepare(&stale, &analyzer, false).unwrap();
    let (current, lease) = configuration::inspect_leased(&root, None).unwrap();
    assert_eq!(current.token, updated.token);
    assert_ne!(current.token, prepared.token);
    drop(lease);
    fs::write(
        current.path(),
        [current.contents.as_slice(), b"# manual\n"].concat(),
    )
    .unwrap();
    assert!(matches!(
        configuration::inspect_leased(&root, None),
        Err(Error::StalePreview)
    ));
}

/// Run in a separate process so advisory-lock behavior cannot be inferred from in-process mocks.
#[test]
fn lease_child() {
    let Some(root) = std::env::var_os("ESKA_TEST_LEASE_ROOT").map(PathBuf::from) else {
        return;
    };
    let (_plan, _lease) = configuration::inspect_leased(&root, Some("one")).unwrap();
    fs::write(root.join("lease-ready"), b"").unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !root.join("release-lease").exists() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Explicit integration uses the real published 0.4.0 binary; the ordinary suite does not fake this result.
#[test]
#[ignore = "requires ESKA_BSL_ANALYZER pointing to published bsl-analyzer-app 0.4.0"]
fn published_analyzer_accepts_all_project_kinds_and_rejects_unknown_policy() {
    let binary = std::env::var_os("ESKA_BSL_ANALYZER")
        .map(PathBuf::from)
        .expect("explicit upstream executable");
    let fixture = TestDir::new();
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary,
        timeout: Duration::from_secs(60),
        cancelled: &cancelled,
        connection_closed: None,
    };
    for (index, kind) in [
        ProjectType::Configuration,
        ProjectType::Extension,
        ProjectType::Processing,
        ProjectType::Report,
    ]
    .into_iter()
    .enumerate()
    {
        let root = fixture.0.join(format!("Проект {index}"));
        project(&root, kind);
        let plan = configuration::inspect(&root, None).unwrap();
        let current = configuration::prepare(&plan, &analyzer, false).unwrap();
        assert_eq!(current.status, Status::Current);
        assert_eq!(
            plan.context.limited_context(),
            kind != ProjectType::Configuration
        );
        let previous = fs::read(plan.path()).unwrap();
        let mut manifest = fs::read_to_string(root.join("eska.toml")).unwrap();
        manifest.push_str("\n[tools.bsl-analyzer.unknown_policy]\nvalue=true\n");
        fs::write(root.join("eska.toml"), manifest).unwrap();
        let plan = configuration::inspect(&root, None).unwrap();
        assert!(matches!(
            configuration::prepare(&plan, &analyzer, false),
            Err(Error::CheckConfig { .. })
        ));
        assert_eq!(fs::read(plan.path()).unwrap(), previous);
    }
}
