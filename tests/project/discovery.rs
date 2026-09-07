use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use eska::{
    config::ProjectConfigError,
    project::ProjectType,
    project::discovery::{
        ContextDiscoveryError, DiscoveryContext, DiscoveryError, discover, discover_context,
    },
    vcs::workflow::WorkflowPreset,
};

use crate::support::TestDir as Fixture;

impl Fixture {
    fn project(&self, name: &str) -> PathBuf {
        let root = self.0.join(name);
        fs::create_dir_all(root.join("src/CommonModules/Module/Ext")).expect("create sources");
        fs::write(
            root.join("eska.toml"),
            "[project]\ntype = 'configuration'\n",
        )
        .expect("write config");
        root
    }

    fn workspace(&self) -> (PathBuf, PathBuf, PathBuf) {
        let root = self.0.join("workspace");
        let report = root.join("src/sales-report");
        let processing = root.join("src/import-orders");
        fs::create_dir_all(&report).expect("create report member");
        fs::create_dir_all(&processing).expect("create processing member");
        fs::write(
            root.join("eska.toml"),
            "[workspace]\nmembers = ['src/sales-report', 'src/import-orders']\n\
             [build]\nplatform_version = '8.3.27.2325'\nartifacts_directory = 'dist'\n\
             [vcs.workflow]\npreset = 'trunk'\n",
        )
        .expect("write workspace config");
        fs::write(
            report.join("eska.toml"),
            "[project]\nname = 'sales-report'\ntype = 'report'\nsource = '.'\n",
        )
        .expect("write report config");
        fs::write(
            processing.join("eska.toml"),
            "[project]\nname = 'import-orders'\ntype = 'processing'\nsource = '.'\n\
             [build]\nplatform_version = '8.5.4.1000'\n",
        )
        .expect("write processing config");
        (root, report, processing)
    }
}

fn cli(directory: &Path, locale: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(directory)
        .env_remove("ESKA_LANG")
        .args(["--lang", locale])
        .args(args)
        .output()
        .expect("run eska")
}

fn success(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

fn failure(output: &Output, expected: &str) {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(expected), "{stderr}");
    assert!(!stderr.contains("TOML parse error"), "{stderr}");
}

#[test]
fn discovers_from_root_and_deep_descendants() {
    let fixture = Fixture::new();
    let root = fixture.project("project");
    for start in [&root, &root.join("src/CommonModules/Module/Ext")] {
        let project = discover(start).expect("valid project");
        assert_eq!(project.root(), root);
        assert_eq!(project.source(), root.join("src"));
        assert_eq!(
            project.configuration().project_type(),
            ProjectType::Configuration
        );
    }
}

#[test]
fn discovers_workspace_root_and_current_member_with_resolved_settings() {
    let fixture = Fixture::new();
    let (root, report, processing) = fixture.workspace();

    let DiscoveryContext::Workspace {
        workspace,
        current_member,
    } = discover_context(&root).expect("workspace root")
    else {
        panic!("expected workspace context");
    };
    assert_eq!(workspace.root(), root);
    assert_eq!(current_member, None);
    assert_eq!(workspace.members().len(), 2);
    assert_eq!(workspace.members()[0].name().as_str(), "sales-report");
    assert_eq!(
        workspace.members()[0]
            .project()
            .configuration()
            .project_type(),
        ProjectType::Report
    );
    assert_eq!(
        workspace.members()[0]
            .project()
            .configuration()
            .build_settings()
            .platform_version()
            .unwrap()
            .as_str(),
        "8.3.27.2325"
    );
    assert_eq!(
        workspace.members()[1]
            .project()
            .configuration()
            .build_settings()
            .platform_version()
            .unwrap()
            .as_str(),
        "8.5.4.1000"
    );
    assert_eq!(
        workspace.members()[0]
            .project()
            .configuration()
            .build_settings()
            .artifacts_directory(),
        Path::new("dist")
    );
    assert_eq!(
        workspace.members()[0].project().configuration().workflow(),
        Some(WorkflowPreset::Trunk)
    );

    let DiscoveryContext::Workspace { current_member, .. } =
        discover_context(&processing).expect("current workspace member")
    else {
        panic!("expected workspace member context");
    };
    assert_eq!(current_member.unwrap().as_str(), "import-orders");

    // Legacy command discovery remains member-local until workspace selectors land.
    assert_eq!(
        discover(&report)
            .expect("legacy member discovery")
            .configuration()
            .build_settings()
            .platform_version(),
        None
    );
    for locale in ["ru", "en"] {
        success(&cli(&root, locale, &[]));
        success(&cli(&processing, locale, &[]));
    }
}

#[test]
fn rejects_unlisted_and_invalid_workspace_members() {
    let fixture = Fixture::new();
    let (root, report, _) = fixture.workspace();
    let unlisted = root.join("src/unlisted");
    fs::create_dir_all(&unlisted).expect("create unlisted member");
    fs::write(
        unlisted.join("eska.toml"),
        "[project]\nname = 'unlisted'\ntype = 'report'\nsource = '.'\n",
    )
    .expect("write unlisted config");
    assert!(matches!(
        discover_context(&unlisted),
        Err(ContextDiscoveryError::UnlistedProject { .. })
    ));

    fs::write(
        report.join("eska.toml"),
        "[project]\ntype = 'report'\nsource = '.'\n",
    )
    .expect("remove member name");
    assert!(matches!(
        discover_context(&root),
        Err(ContextDiscoveryError::MemberNameMissing { .. })
    ));
    for (locale, expected) in [
        ("ru", "должен быть задан [project].name"),
        ("en", "must define [project].name"),
    ] {
        failure(&cli(&root, locale, &[]), expected);
    }
}

#[test]
fn rejects_member_level_repository_and_artifact_settings() {
    let fixture = Fixture::new();
    let (root, report, _) = fixture.workspace();
    fs::write(
        report.join("eska.toml"),
        "[project]\nname = 'sales-report'\ntype = 'report'\nsource = '.'\n\
         [vcs.workflow]\npreset = 'trunk'\n",
    )
    .expect("write member workflow");
    assert!(matches!(
        discover_context(&root),
        Err(ContextDiscoveryError::MemberWorkflowUnsupported { .. })
    ));

    fs::write(
        report.join("eska.toml"),
        "[project]\nname = 'sales-report'\ntype = 'report'\nsource = '.'\n\
         [build]\nartifacts_directory = 'member-build'\n",
    )
    .expect("write member artifact path");
    assert!(matches!(
        discover_context(&root),
        Err(ContextDiscoveryError::MemberArtifactsDirectoryUnsupported { .. })
    ));
}

#[test]
fn rejects_duplicate_names_and_nested_member_roots() {
    let fixture = Fixture::new();
    let (root, _, processing) = fixture.workspace();
    fs::write(
        processing.join("eska.toml"),
        "[project]\nname = 'sales-report'\ntype = 'processing'\nsource = '.'\n",
    )
    .expect("duplicate member name");
    assert!(matches!(
        discover_context(&root),
        Err(ContextDiscoveryError::DuplicateName { .. })
    ));

    fs::write(
        root.join("eska.toml"),
        "[workspace]\nmembers = ['src/sales-report', 'src']\n",
    )
    .expect("nested member list");
    assert!(matches!(
        discover_context(&root),
        Err(ContextDiscoveryError::NestedMembers { .. })
    ));
}

#[cfg(unix)]
#[test]
fn rejects_workspace_member_symlinks_outside_workspace() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let (root, _, _) = fixture.workspace();
    let outside = fixture.0.join("outside-member");
    fs::create_dir(&outside).expect("outside member directory");
    symlink(&outside, root.join("src/outside")).expect("outside member link");
    fs::write(
        root.join("eska.toml"),
        "[workspace]\nmembers = ['src/outside']\n",
    )
    .expect("outside member config");

    assert!(matches!(
        discover_context(&root),
        Err(ContextDiscoveryError::MemberOutsideWorkspace { .. })
    ));
}

#[test]
fn selects_nearest_project_and_never_skips_invalid_config() {
    let fixture = Fixture::new();
    fixture.project("outer");
    let inner = fixture.project("outer/inner");
    let start = inner.join("src/CommonModules");
    assert_eq!(discover(&start).expect("inner project").root(), inner);

    fs::write(inner.join("eska.toml"), "not valid TOML").expect("break inner config");
    assert!(matches!(
        discover(&start),
        Err(DiscoveryError::Config { path, source: ProjectConfigError::Toml(_) })
            if path == inner.join("eska.toml")
    ));
}

#[test]
fn reports_missing_project_and_invalid_start_directory() {
    let fixture = Fixture::new();
    assert!(matches!(
        discover(&fixture.0),
        Err(DiscoveryError::NotFound { .. })
    ));
    assert!(matches!(
        discover(&fixture.0.join("missing")),
        Err(DiscoveryError::Io { .. })
    ));
    let file = fixture.0.join("file");
    fs::write(&file, "").expect("create file");
    assert!(matches!(
        discover(&file),
        Err(DiscoveryError::StartNotDirectory { .. })
    ));
}

#[test]
fn validates_source_existence_type_and_custom_location() {
    let fixture = Fixture::new();
    let root = fixture.project("project");
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'report'\nsource = 'xml'\n",
    )
    .expect("custom source config");
    assert!(matches!(
        discover(&root),
        Err(DiscoveryError::Io { path, source })
            if path == root.join("xml") && source.kind() == std::io::ErrorKind::NotFound
    ));
    fs::write(root.join("xml"), "not a directory").expect("create source file");
    assert!(matches!(
        discover(&root),
        Err(DiscoveryError::SourceNotDirectory { .. })
    ));
    fs::remove_file(root.join("xml")).expect("remove source file");
    fs::create_dir(root.join("xml")).expect("create source directory");
    assert_eq!(
        discover(&root).expect("custom sources").source(),
        root.join("xml")
    );

    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'report'\nsource = '.'\n",
    )
    .expect("source at project root");
    assert_eq!(discover(&root).expect("source root").source(), root);
}

#[test]
fn rejects_config_directory_instead_of_skipping_it() {
    let fixture = Fixture::new();
    fixture.project("outer");
    let root = fixture.0.join("outer/inner");
    fs::create_dir_all(root.join("eska.toml")).expect("config directory");
    assert!(matches!(
        discover(&root),
        Err(DiscoveryError::ConfigNotFile { .. })
    ));
}

#[test]
fn cli_supports_current_explicit_and_relative_directories_in_both_locales() {
    let fixture = Fixture::new();
    let root = fixture.project("project");
    for locale in ["ru", "en"] {
        success(&cli(&root, locale, &[]));
        success(&cli(
            &root.join("src/CommonModules/Module/Ext"),
            locale,
            &[],
        ));
        success(&cli(
            &fixture.0,
            locale,
            &["--project-dir", "project/src/../src"],
        ));
        success(&cli(
            &fixture.0,
            locale,
            &["--project-dir", root.to_str().expect("UTF-8 path")],
        ));
        // A supplied start directory must take precedence over a valid cwd.
        failure(
            &cli(&root, locale, &["--project-dir", "missing"]),
            if locale == "ru" {
                "путь не существует"
            } else {
                "path does not exist"
            },
        );
    }
}

#[test]
fn cli_localizes_missing_project() {
    let fixture = Fixture::new();
    for (locale, expected) in [
        ("ru", "Файл eska.toml не найден"),
        ("en", "No eska.toml found"),
    ] {
        let output = cli(&fixture.0, locale, &[]);
        failure(&output, expected);
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(fixture.0.to_str().expect("UTF-8"))
        );
    }
}

#[test]
fn cli_localizes_absolute_source_paths() {
    let fixture = Fixture::new();
    let root = fixture.project("project");
    // Use a literal TOML string so Windows path separators are preserved too.
    let config = format!(
        "[project]\ntype = 'report'\nsource = '{}'\n",
        root.display()
    );
    fs::write(root.join("eska.toml"), config).expect("absolute source config");
    for (locale, expected) in [
        ("ru", "должен быть относительным"),
        ("en", "must be relative"),
    ] {
        failure(&cli(&root, locale, &[]), expected);
    }
}

#[test]
fn cli_localizes_config_errors_and_preserves_machine_values() {
    let fixture = Fixture::new();
    let root = fixture.project("project");
    for (config, ru, en) in [
        ("[project", "Некорректный TOML", "Invalid TOML"),
        ("[project]\ntype = 1", "Некорректный TOML", "Invalid TOML"),
        (
            "[project]\ntype = 'report'\n[vcs.workflow]\npreset = 'unsupported'",
            "Неизвестный workflow",
            "Unknown workflow",
        ),
        (
            "[project]\ntype = 'report'\n[vcs.workflow]",
            "Некорректный TOML",
            "Invalid TOML",
        ),
        (
            "[project]\ntype = 'configuration'\nlocale = 'ru'",
            "Некорректный TOML",
            "Invalid TOML",
        ),
        (
            "[project]\ntype = 'unknown-type'",
            "Неизвестный тип проекта",
            "Unknown project type",
        ),
        (
            "[project]\ntype = 'report'\nsource_format = 'edt'",
            "Неизвестный формат исходников",
            "Unknown source format",
        ),
        (
            "[project]\ntype = 'report'\nsource = ''",
            "не может быть пустым",
            "must not be empty",
        ),
        (
            "[project]\ntype = 'report'\nsource = '../outside'",
            "не должен содержать",
            "must not contain",
        ),
    ] {
        fs::write(root.join("eska.toml"), config).expect("write test config");
        for (locale, expected) in [("ru", ru), ("en", en)] {
            failure(&cli(&root, locale, &[]), expected);
        }
    }
    fs::write(root.join("eska.toml"), b"\xff").expect("write invalid UTF-8");
    for (locale, expected) in [
        ("ru", "не является корректным текстом UTF-8"),
        ("en", "not valid UTF-8 text"),
    ] {
        failure(&cli(&root, locale, &[]), expected);
    }
}

#[test]
fn cli_localizes_source_and_directory_errors() {
    let fixture = Fixture::new();
    let root = fixture.project("project");
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'report'\nsource = 'missing'\n",
    )
    .expect("missing sources config");
    for (locale, expected) in [("ru", "путь не существует"), ("en", "path does not exist")]
    {
        failure(&cli(&root, locale, &[]), expected);
    }
    fs::write(root.join("missing"), "").expect("source file");
    for (locale, expected) in [
        ("ru", "Путь исходников не является каталогом"),
        ("en", "source path is not a directory"),
    ] {
        failure(&cli(&root, locale, &[]), expected);
    }
    for (locale, expected) in [
        ("ru", "Начальный путь не является каталогом"),
        ("en", "starting path is not a directory"),
    ] {
        failure(&cli(&root, locale, &["--project-dir", "missing"]), expected);
    }
    fs::remove_file(root.join("eska.toml")).expect("remove config");
    fs::create_dir(root.join("eska.toml")).expect("config directory");
    for (locale, expected) in [
        ("ru", "должен быть обычным файлом"),
        ("en", "must be a regular file"),
    ] {
        failure(&cli(&root, locale, &[]), expected);
    }
}

#[test]
fn help_and_version_do_not_require_a_project_and_usage_errors_exit_two() {
    let fixture = Fixture::new();
    for locale in ["ru", "en"] {
        for flag in ["--help", "-h", "--version", "-V"] {
            let output = cli(&fixture.0, locale, &["--project-dir", "missing", flag]);
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stderr.is_empty());
            assert!(!output.stdout.is_empty());
        }
        let output = cli(&fixture.0, locale, &["--project-dir"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[cfg(unix)]
#[test]
fn symlinks_resolve_without_escaping_the_project_or_skipping_broken_configs() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let root = fixture.project("project");
    symlink(root.join("src"), root.join("linked-src")).expect("internal source link");
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'report'\nsource = 'linked-src'\n",
    )
    .expect("linked source config");
    assert_eq!(
        discover(&root).expect("internal link").source(),
        root.join("src")
    );
    symlink(&root, fixture.0.join("alias")).expect("start directory link");
    assert_eq!(
        discover(&fixture.0.join("alias"))
            .expect("linked start")
            .root(),
        root
    );

    symlink(&fixture.0, root.join("outside")).expect("external source link");
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'report'\nsource = 'outside'\n",
    )
    .expect("external source config");
    assert!(matches!(
        discover(&root),
        Err(DiscoveryError::Config {
            source: ProjectConfigError::ProjectPath(
                eska::project::ProjectPathError::SourceOutsideRoot { .. }
            ),
            ..
        })
    ));
    for (locale, expected) in [
        ("ru", "за пределами корня проекта"),
        ("en", "outside the project root"),
    ] {
        failure(&cli(&root, locale, &[]), expected);
    }

    let child = root.join("child");
    fs::create_dir(&child).expect("child directory");
    symlink("missing-config", child.join("eska.toml")).expect("dangling config link");
    assert!(
        matches!(discover(&child), Err(DiscoveryError::Io { path, .. }) if path == child.join("eska.toml"))
    );
}
