//! One-shot analysis must reject broken or partial process output, even with a zero exit status.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

#[cfg(unix)]
use std::{sync::atomic::Ordering, thread, time::Instant};

use eska_analyzer::{
    check::{self, Failure},
    configuration::{self, Plan},
    process::Analyzer,
};
use eska_project_core::{config::ProjectConfig, project::ProjectType};

#[path = "../../../tests/support/mod.rs"]
mod support;

/// Minimal Designer exports retain native Unicode paths and module bytes.
fn project(root: &Path, kind: ProjectType) -> PathBuf {
    let config = ProjectConfig::new(kind)
        .with_source("Выгрузка XML".into())
        .unwrap();
    fs::write(root.join("eska.toml"), config.to_toml().unwrap()).unwrap();
    let (tag, extra, descriptor) = match kind {
        ProjectType::Configuration => (
            "Configuration",
            "<CompatibilityMode>Version8_3_27</CompatibilityMode>",
            "Configuration.xml",
        ),
        ProjectType::Extension => (
            "Configuration",
            "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>",
            "Configuration.xml",
        ),
        ProjectType::Processing => ("ExternalDataProcessor", "", "Обработка.xml"),
        ProjectType::Report => ("ExternalReport", "", "Отчёт.xml"),
    };
    let source = root.join("Выгрузка XML");
    fs::create_dir(&source).unwrap();
    fs::write(source.join(descriptor), format!("\u{feff}<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><{tag} uuid=\"11111111-1111-1111-1111-111111111111\"><Properties><Name>Пример</Name>{extra}</Properties><ChildObjects/></{tag}></MetaDataObject>\r\n")).unwrap();
    let parent = if matches!(kind, ProjectType::Processing | ProjectType::Report) {
        source
            .join(Path::new(descriptor).file_stem().unwrap())
            .join("Ext")
    } else {
        source.join("Ext")
    };
    fs::create_dir_all(&parent).unwrap();
    let module = parent.join("ObjectModule.bsl");
    fs::write(
        &module,
        "\u{feff}Процедура Пример()\r\n    А = 1;\r\nКонецПроцедуры\r\n",
    )
    .unwrap();
    module
}

/// Executable fixtures are immutable inodes, avoiding ETXTBSY during parallel process spawns.
#[cfg(unix)]
fn fake(root: &Path) -> PathBuf {
    let binary = root.join("analyzer fixture");
    std::os::unix::fs::symlink(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/analyzer-check.py"),
        &binary,
    )
    .unwrap();
    binary
}

/// Prepare through the same external contract that the actual CLI uses.
fn prepare(root: &Path, analyzer: &Analyzer<'_>) -> Plan {
    let initial = configuration::inspect(root, None).unwrap();
    configuration::prepare(&initial, analyzer, false).unwrap()
}

/// A complete clean file proves coverage; warnings obey only the root-owned thresholds.
#[cfg(unix)]
#[test]
fn complete_stream_and_thresholds_keep_clean_files_and_policy() {
    let directory = support::TestDir::new();
    let module = project(&directory.0, ProjectType::Configuration);
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: fake(&directory.0),
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let plan = prepare(&directory.0, &analyzer);
    let report = check::analyze(&plan, &analyzer).unwrap();
    assert_eq!(report.files.len(), 1);
    assert_eq!(report.files[0].diagnostics, []);
    assert!(!report.counts.exceeds(&report.policy));
    let unchanged = fs::read(&module).unwrap();
    fs::write(directory.0.join("stream-mode"), "warning").unwrap();
    let report = check::analyze(&plan, &analyzer).unwrap();
    assert_eq!(report.counts.warnings, 1);
    assert!(!report.counts.exceeds(&report.policy));
    let manifest = directory.0.join("eska.toml");
    let text = format!(
        "{}\n[tools.eska-check]\nmax_warnings=0\n",
        fs::read_to_string(&manifest).unwrap()
    );
    fs::write(&manifest, text).unwrap();
    let plan = prepare(&directory.0, &analyzer);
    let report = check::analyze(&plan, &analyzer).unwrap();
    assert!(report.counts.exceeds(&report.policy));
    assert_eq!(fs::read(module).unwrap(), unchanged);
    assert!(!directory.0.join(".git").exists());
}

/// Success requires both stream completeness and a successful process, with the exact input set.
#[cfg(unix)]
#[test]
fn malformed_partial_and_failed_runs_never_produce_a_report() {
    for mode in [
        "empty",
        "no-done",
        "duplicate",
        "unexpected",
        "bad-count",
        "file-error",
        "failed-files",
        "missing-file",
        "truncated",
        "bad-json",
        "stream-version",
        "partial-baseline",
        "exit-after-done",
        "mutate",
        "add-file",
        "wrong-version",
    ] {
        let directory = support::TestDir::new();
        project(&directory.0, ProjectType::Configuration);
        let cancelled = AtomicBool::new(false);
        let analyzer = Analyzer {
            binary: fake(&directory.0),
            timeout: Duration::from_secs(5),
            cancelled: &cancelled,
            connection_closed: None,
        };
        let plan = prepare(&directory.0, &analyzer);
        fs::write(directory.0.join("stream-mode"), mode).unwrap();
        let failure = check::analyze(&plan, &analyzer).unwrap_err();
        let expected = match mode {
            "exit-after-done" => "analyzer_exit",
            "mutate" | "add-file" => "inputs_changed",
            "wrong-version" => "analyzer_version",
            _ => "invalid_stream",
        };
        assert_eq!(failure.code(), expected, "{mode}: {failure:?}");
    }
}

/// CI refuses configured diff/author filters and empty source sets instead of weakening coverage.
#[cfg(unix)]
#[test]
fn source_selection_and_structure_fail_before_analysis() {
    for policy in [
        "[tools.bsl-analyzer.analysis]\ndiff_base='main'",
        "[tools.bsl-analyzer.analysis]\nignored_authors=['Vendor']",
        "[tools.bsl-analyzer.source]\nexclude=['Выгрузка XML']",
    ] {
        let directory = support::TestDir::new();
        project(&directory.0, ProjectType::Configuration);
        let manifest = directory.0.join("eska.toml");
        fs::write(
            &manifest,
            format!("{}\n{policy}\n", fs::read_to_string(&manifest).unwrap()),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let analyzer = Analyzer {
            binary: fake(&directory.0),
            timeout: Duration::from_secs(5),
            cancelled: &cancelled,
            connection_closed: None,
        };
        let plan = prepare(&directory.0, &analyzer);
        assert!(matches!(
            check::analyze(&plan, &analyzer),
            Err(Failure::PartialPolicy { .. } | Failure::EmptyInput)
        ));
        assert!(!directory.0.join("analysis-started").exists());
    }
    let directory = support::TestDir::new();
    project(&directory.0, ProjectType::Configuration);
    let descriptor = directory.0.join("Выгрузка XML/Configuration.xml");
    let text = fs::read_to_string(&descriptor).unwrap().replace(
        "<ChildObjects/>",
        "<ChildObjects><Catalog>Missing</Catalog></ChildObjects>",
    );
    fs::write(descriptor, text).unwrap();
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: fake(&directory.0),
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let plan = prepare(&directory.0, &analyzer);
    assert!(matches!(
        check::analyze(&plan, &analyzer),
        Err(Failure::Structure { .. })
    ));
    assert!(!directory.0.join("analysis-started").exists());
}

/// Cancellation and deadline kill and reap the owned analyzer, without returning partial diagnostics.
#[cfg(unix)]
#[test]
fn timeout_and_cancellation_reap_the_actual_child() {
    for cancel in [false, true] {
        let directory = support::TestDir::new();
        project(&directory.0, ProjectType::Configuration);
        let cancelled = AtomicBool::new(false);
        let mut analyzer = Analyzer {
            binary: fake(&directory.0),
            timeout: Duration::from_secs(5),
            cancelled: &cancelled,
            connection_closed: None,
        };
        let plan = prepare(&directory.0, &analyzer);
        analyzer.timeout = Duration::from_millis(300);
        fs::write(directory.0.join("stream-mode"), "hang").unwrap();
        let marker = directory.0.join("analysis-started");
        let failure = thread::scope(|scope| {
            if cancel {
                scope.spawn(|| {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while !marker.exists() {
                        assert!(Instant::now() < deadline);
                        thread::sleep(Duration::from_millis(5));
                    }
                    cancelled.store(true, Ordering::Release);
                });
            }
            check::analyze(&plan, &analyzer).unwrap_err()
        });
        assert_eq!(failure.code(), if cancel { "cancelled" } else { "timeout" });
        #[cfg(target_os = "linux")]
        {
            let pid = fs::read_to_string(marker).unwrap();
            assert!(!Path::new("/proc").join(pid).exists());
        }
    }
}

/// The published application, not a fake stream, supplies the actual diagnostics for all four kinds.
#[test]
#[ignore = "requires ESKA_BSL_ANALYZER pointing to published bsl-analyzer-app 0.4.0"]
fn published_analyzer_checks_real_designer_modules_for_all_types() {
    let binary = std::env::var_os("ESKA_BSL_ANALYZER").expect("explicit analyzer");
    for kind in [
        ProjectType::Configuration,
        ProjectType::Extension,
        ProjectType::Processing,
        ProjectType::Report,
    ] {
        let directory = support::TestDir::new();
        let module = project(&directory.0, kind);
        let before = fs::read(&module).unwrap();
        let cancelled = AtomicBool::new(false);
        let analyzer = Analyzer {
            binary: binary.clone().into(),
            timeout: Duration::from_secs(120),
            cancelled: &cancelled,
            connection_closed: None,
        };
        let plan = prepare(&directory.0, &analyzer);
        let report = check::analyze(&plan, &analyzer).unwrap();
        assert_eq!(report.files.len(), 1);
        assert!(
            !report.files[0].diagnostics.is_empty(),
            "actual diagnostics expected for unused local variable"
        );
        assert_eq!(fs::read(module).unwrap(), before);
    }
}

/// The real CLI must use the selected base plus five extensions and EPF, including literal excludes.
#[test]
#[ignore = "requires ESKA_BSL_ANALYZER pointing to published bsl-analyzer-app 0.4.0"]
fn published_application_uses_exact_members_and_exclusions() {
    let directory = support::TestDir::new();
    for (name, kind) in [
        ("base", ProjectType::Configuration),
        ("e1", ProjectType::Extension),
        ("e2", ProjectType::Extension),
        ("e3", ProjectType::Extension),
        ("e4", ProjectType::Extension),
        ("e5", ProjectType::Extension),
        ("epf", ProjectType::Processing),
        ("unselected", ProjectType::Configuration),
    ] {
        let root = directory.0.join(name);
        fs::create_dir_all(&root).unwrap();
        project(&root, kind);
        let config = ProjectConfig::new(kind)
            .with_source("Выгрузка XML".into())
            .unwrap()
            .with_name(eska_project_core::project::ProjectName::parse(name.to_owned()).unwrap());
        fs::write(
            root.join("eska.toml"),
            config.to_workspace_member_toml().unwrap(),
        )
        .unwrap();
    }
    fs::write(directory.0.join("eska.toml"), "[workspace]\n[workspace.members.unselected]\nconfiguration='unselected'\n[workspace.members.demo]\nconfiguration='base'\nextensions = [{path='e1'}, {path='e2',depends_on=['e1']}, {path='e3',depends_on=['e2']}, {path='e4',depends_on=['e3']}, {path='e5',depends_on=['e4']}]\nexternal_objects=['epf']\n[analysis]\ndefault_application='demo'\n[tools.bsl-analyzer.source]\nexclude=['base/Выгрузка XML/skip']\n").unwrap();
    let excluded = directory.0.join("base/Выгрузка XML/skip");
    fs::create_dir(&excluded).unwrap();
    fs::write(excluded.join("Ignored.bsl"), "broken fixture").unwrap();
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: std::env::var_os("ESKA_BSL_ANALYZER")
            .expect("explicit analyzer")
            .into(),
        timeout: Duration::from_secs(120),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let plan = prepare(&directory.0, &analyzer);
    let report = check::analyze(&plan, &analyzer).unwrap();
    assert_eq!(report.files.len(), 7);
    assert!(
        report
            .files
            .iter()
            .all(|file| !file.path.starts_with(&excluded)
                && !file.path.starts_with(directory.0.join("unselected")))
    );
}
