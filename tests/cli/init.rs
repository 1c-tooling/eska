use eska::{
    config::{ProjectConfig, WorkspaceConfig},
    project::ProjectType,
    project::discovery,
    project::init::{self, InitError},
    vcs::workflow::WorkflowPreset,
};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use crate::support::TestDir;

fn command(root: &Path, locale: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_eska"));
    command
        .current_dir(root)
        .env_remove("ESKA_LANG")
        .args(["--lang", locale, "init"]);
    command
}

fn descriptor(directory: &Path, kind: &str) {
    fs::create_dir_all(directory).expect("source directory");
    let (tag, properties, name) = match kind {
        "configuration" => (
            "Configuration",
            "<ConfigurationExtensionCompatibilityMode>Version8_3_27</ConfigurationExtensionCompatibilityMode>",
            "Configuration.xml",
        ),
        "extension" => (
            "Configuration",
            "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose><ConfigurationExtensionCompatibilityMode>Version8_3_27</ConfigurationExtensionCompatibilityMode>",
            "Configuration.xml",
        ),
        "processing" => ("ExternalDataProcessor", "", "Обработка.xml"),
        "report" => ("ExternalReport", "", "Отчёт.xml"),
        _ => panic!("fixture type"),
    };
    fs::write(directory.join(name), format!("\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?><MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.17\"><{tag} uuid=\"12345678-1234-1234-1234-123456789012\"><Properties><Name>Пример</Name>{properties}</Properties><ChildObjects/></{tag}></MetaDataObject>")).expect("XML");
    fs::write(
        directory.join("ObjectModule.bsl"),
        "// Существующий модуль\r\n",
    )
    .expect("module");
}

fn success(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(!output.stdout.contains(&0x1b));
    String::from_utf8(output.stdout.clone()).expect("UTF-8")
}

fn failure(output: &Output, code: i32, locale: &str) -> String {
    assert_eq!(output.status.code(), Some(code));
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr.clone()).expect("UTF-8");
    // Test paths may contain Cyrillic; compare against English/Russian sentences.
    assert!(!error.is_empty());
    assert!(!error.contains('\u{1b}'));
    if locale == "en" {
        assert!(!error.contains("Не удалось"));
    }
    error
}

/// Creates an empty workspace whose comments must survive member enrollment.
fn empty_workspace() -> TestDir {
    let fixture = TestDir::new();
    fs::write(
        fixture.0.join("eska.toml"),
        "# root comment\n[workspace]\nmembers = [] # members comment\n\n[build]\nplatform_version = \"8.3.27.2325\"\n\n[vcs.workflow]\npreset = \"trunk\"\n",
    )
    .expect("workspace manifest");
    fixture
}

#[test]
fn workspace_init_enrolls_a_copied_export_without_workflow_or_git_files() {
    for locale in ["ru", "en"] {
        let fixture = empty_workspace();
        let member = fixture.0.join("src/my-orders");
        descriptor(&member, "processing");
        let before_source = snapshot(&member);
        let output = command(&member, locale).output().expect("workspace init");
        let text = success(&output);
        assert!(text.contains(if locale == "ru" {
            "добавлен в workspace.members"
        } else {
            "added to workspace.members"
        }));
        let manifest = fs::read_to_string(member.join("eska.toml")).unwrap();
        assert!(manifest.contains("name = \"my-orders\""));
        assert!(manifest.contains("type = \"processing\""));
        assert!(manifest.contains("source = \".\""));
        assert!(!manifest.contains("[build]"));
        assert!(!manifest.contains("[vcs"));
        assert!(!member.join(".git").exists());
        assert!(!member.join(".gitignore").exists());
        assert!(!member.join(".gitattributes").exists());
        let mut after_source = snapshot(&member);
        after_source.retain(|(path, _)| path != Path::new("eska.toml"));
        assert_eq!(after_source, before_source);
        let root_manifest = fs::read_to_string(fixture.0.join("eska.toml")).unwrap();
        assert!(root_manifest.starts_with("# root comment\n"));
        assert!(root_manifest.contains("# members comment"));
        assert_eq!(
            WorkspaceConfig::from_toml(&root_manifest)
                .unwrap()
                .members(),
            &[std::path::PathBuf::from("src/my-orders")]
        );
        let context = discovery::discover_context(&member).unwrap();
        let discovery::DiscoveryContext::Workspace { current_member, .. } = context else {
            panic!("workspace member");
        };
        assert_eq!(current_member.unwrap().as_str(), "my-orders");
    }
}

#[test]
fn workspace_init_accepts_an_explicit_portable_name() {
    let fixture = empty_workspace();
    let member = fixture.0.join("src/Скопированный отчёт");
    descriptor(&member.join("src"), "report");
    success(
        &command(&member, "en")
            .args(["--name", "sales-report", "--no-vcs"])
            .output()
            .expect("workspace init with name"),
    );
    let config = ProjectConfig::load(&member.join("eska.toml")).unwrap();
    assert_eq!(config.name().unwrap().as_str(), "sales-report");
    assert_eq!(config.source(), Path::new("src"));
    assert_eq!(
        WorkspaceConfig::load(&fixture.0.join("eska.toml"))
            .unwrap()
            .members(),
        &[std::path::PathBuf::from("src/Скопированный отчёт")]
    );
}

#[test]
fn workspace_init_rejects_workspace_owned_workflow_without_writing() {
    for locale in ["ru", "en"] {
        let fixture = empty_workspace();
        let member = fixture.0.join("src/my-orders");
        descriptor(&member, "processing");
        let root_before = fs::read(fixture.0.join("eska.toml")).unwrap();
        let member_before = snapshot(&member);
        let output = command(&member, locale)
            .args(["--workflow", "trunk"])
            .output()
            .expect("workspace workflow rejection");
        failure(&output, 2, locale);
        assert_eq!(fs::read(fixture.0.join("eska.toml")).unwrap(), root_before);
        assert_eq!(snapshot(&member), member_before);
    }
}

#[test]
fn workspace_init_rejects_nested_members_and_duplicate_names_before_writing() {
    let fixture = TestDir::new();
    let existing = fixture.0.join("src/existing");
    descriptor(&existing.join("src"), "report");
    fs::write(
        existing.join("eska.toml"),
        "[project]\nname = \"existing\"\ntype = \"report\"\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("eska.toml"),
        "[workspace]\nmembers = [\"src/existing\"]\n",
    )
    .unwrap();

    let nested = existing.join("copied");
    descriptor(&nested, "processing");
    let root_before = fs::read(fixture.0.join("eska.toml")).unwrap();
    let nested_before = snapshot(&nested);
    let output = command(&nested, "en").output().expect("nested member");
    failure(&output, 1, "en");
    assert_eq!(fs::read(fixture.0.join("eska.toml")).unwrap(), root_before);
    assert_eq!(snapshot(&nested), nested_before);

    let duplicate = fixture.0.join("src/duplicate");
    descriptor(&duplicate, "processing");
    let duplicate_before = snapshot(&duplicate);
    let output = command(&duplicate, "en")
        .args(["--name", "existing"])
        .output()
        .expect("duplicate name");
    failure(&output, 1, "en");
    assert_eq!(fs::read(fixture.0.join("eska.toml")).unwrap(), root_before);
    assert_eq!(snapshot(&duplicate), duplicate_before);
}

#[test]
fn detects_all_types_locations_and_locales_without_touching_sources() {
    let fixture = TestDir::new();
    for locale in ["ru", "en"] {
        for kind in ["configuration", "extension", "processing", "report"] {
            for location in [".", "src", "sources/designer"] {
                let root = fixture.0.join(format!(
                    "{locale}-{kind}-{}",
                    location.replace(['/', '.'], "-")
                ));
                let source = root.join(location);
                fs::create_dir_all(&root).expect("project root");
                descriptor(&source, kind);
                let before = snapshot(&root);
                let mut cli = command(&fixture.0, locale);
                cli.arg(&root)
                    .args(["--workflow", "github-flow", "--no-vcs"]);
                if location == "sources/designer" {
                    cli.args(["--source", location]);
                }
                let message = success(&cli.output().expect("init"));
                assert!(message.contains(if locale == "ru" {
                    "Проект подключён"
                } else {
                    "Project initialized"
                }));
                assert!(
                    fs::read_to_string(root.join("eska.toml"))
                        .expect("project config text")
                        .contains("platform_version = \"\"")
                );
                let config = ProjectConfig::load(&root.join("eska.toml")).expect("config");
                assert_eq!(
                    fs::read(root.join(".gitattributes")).expect("attributes"),
                    include_bytes!("../../assets/project/.gitattributes")
                );
                assert_eq!(
                    fs::read(root.join(".gitignore")).expect("ignore rules"),
                    include_bytes!("../../assets/project/.gitignore")
                );
                assert_eq!(config.source(), Path::new(location));
                assert_eq!(
                    config.configuration().workflow(),
                    Some(WorkflowPreset::GithubFlow)
                );
                assert!(
                    config
                        .to_toml()
                        .expect("TOML")
                        .contains(&format!("type = \"{kind}\""))
                );
                let project = discovery::discover(&source).expect("discovery");
                assert_eq!(project.root(), root);
                assert_eq!(project.source(), fs::canonicalize(&source).expect("source"));
                let mut after = snapshot(&root);
                after.retain(|(name, _)| {
                    ![
                        Path::new("eska.toml"),
                        Path::new(".gitattributes"),
                        Path::new(".gitignore"),
                    ]
                    .contains(&name.as_path())
                });
                assert_eq!(before, after, "all existing bytes preserved");
            }
        }
    }
}

/// Existing Git files are authoritative; init fills only absent shared files.
#[test]
fn creates_only_missing_git_files_and_preserves_existing_files() {
    let fixture = TestDir::new();
    for (index, missing) in [None, Some(".gitattributes"), Some(".gitignore")]
        .into_iter()
        .enumerate()
    {
        let root = fixture.0.join(format!("existing-git-files-{index}"));
        descriptor(&root.join("src"), "configuration");
        fs::write(root.join(".gitattributes"), "user attributes\n").expect("attributes");
        fs::write(root.join(".gitignore"), "user ignore rules\n").expect("ignore rules");
        if let Some(path) = missing {
            fs::remove_file(root.join(path)).expect("remove missing fixture file");
        }

        success(
            &command(&root, "en")
                .args(["--workflow", "trunk", "--no-vcs"])
                .output()
                .expect("init"),
        );

        assert_eq!(
            fs::read(root.join(".gitattributes")).expect("attributes"),
            if missing == Some(".gitattributes") {
                include_bytes!("../../assets/project/.gitattributes").as_slice()
            } else {
                b"user attributes\n"
            }
        );
        assert_eq!(
            fs::read(root.join(".gitignore")).expect("ignore rules"),
            if missing == Some(".gitignore") {
                include_bytes!("../../assets/project/.gitignore").as_slice()
            } else {
                b"user ignore rules\n"
            }
        );
    }
}

fn snapshot(root: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    fn visit(directory: &Path, root: &Path, files: &mut Vec<(std::path::PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(directory).expect("list") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                visit(&path, root, files);
            } else {
                files.push((
                    path.strip_prefix(root).expect("relative").to_path_buf(),
                    fs::read(path).expect("read"),
                ));
            }
        }
    }
    let mut files = Vec::new();
    visit(root, root, &mut files);
    files.sort();
    files
}

#[test]
fn initializes_git_without_system_git_or_redirecting_environment() {
    let fixture = TestDir::new();
    descriptor(&fixture.0.join("src"), "configuration");
    let elsewhere = fixture.0.join("untouched");
    let message = success(
        &command(&fixture.0, "en")
            .args(["--workflow", "trunk"])
            .env("PATH", "")
            .env("GIT_DIR", &elsewhere)
            .env("GIT_WORK_TREE", &elsewhere)
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "init.defaultBranch")
            .env("GIT_CONFIG_VALUE_0", "wrong")
            .output()
            .expect("init"),
    );
    assert!(message.contains("Project initialized"));
    let repo = gix::open(&fixture.0).expect("repo");
    assert_eq!(repo.workdir(), Some(fixture.0.as_path()));
    assert!(repo.head().expect("HEAD").is_unborn());
    assert_eq!(
        fs::read_to_string(fixture.0.join(".git/HEAD")).expect("HEAD"),
        "ref: refs/heads/main\n"
    );
    assert!(!elsewhere.exists());
    assert!(!fixture.0.join(".git/eska-init").exists());
}

#[test]
fn preserves_existing_and_ancestor_repositories_byte_for_byte() {
    for nested in [false, true] {
        let fixture = TestDir::new();
        gix::init(&fixture.0).expect("existing repo");
        let git = fixture.0.join(".git");
        fs::write(git.join("HEAD"), "ref: refs/heads/user-branch\n").expect("custom branch");
        fs::write(git.join("untracked-sentinel"), "untouched").expect("sentinel");
        let before = snapshot(&git);
        let root = if nested {
            fixture.0.join("nested")
        } else {
            fixture.0.clone()
        };
        descriptor(&root.join("src"), "extension");
        success(
            &command(&root, "en")
                .args(["--workflow", "custom"])
                .output()
                .expect("init"),
        );
        assert_eq!(snapshot(&git), before);
        if nested {
            assert!(!root.join(".git").exists());
        }
    }
}

#[test]
fn existing_gitfile_is_preserved_and_never_reinitialized() {
    let fixture = TestDir::new();
    let original = fixture.0.join("original");
    fs::create_dir(&original).expect("original");
    gix::init(&original).expect("repo");
    let root = fixture.0.join("project");
    descriptor(&root.join("src"), "report");
    let gitfile = b"gitdir: ../original/.git\n";
    fs::write(root.join(".git"), gitfile).expect("gitfile");
    let before = snapshot(&original);
    success(
        &command(&root, "ru")
            .args(["--workflow", "trunk"])
            .output()
            .expect("init"),
    );
    assert_eq!(fs::read(root.join(".git")).expect("gitfile"), gitfile);
    assert_eq!(snapshot(&original), before);
}

#[test]
fn config_collisions_missing_options_and_invalid_xml_never_write() {
    for locale in ["ru", "en"] {
        let fixture = TestDir::new();
        descriptor(&fixture.0.join("src"), "processing");
        let before = snapshot(&fixture.0);
        for args in [vec![], vec!["--workflow", "unknown"]] {
            failure(
                &command(&fixture.0, locale)
                    .args(args)
                    .output()
                    .expect("bad options"),
                2,
                locale,
            );
            assert_eq!(snapshot(&fixture.0), before);
        }
        for contents in ["existing config", "[broken"] {
            let config = fixture.0.join("eska.toml");
            fs::write(&config, contents).expect("collision");
            let error = failure(
                &command(&fixture.0, locale)
                    .args(["--workflow", "trunk"])
                    .output()
                    .expect("collision"),
                1,
                locale,
            );
            assert!(error.contains(if locale == "ru" {
                "уже существует"
            } else {
                "already exists"
            }));
            assert_eq!(fs::read_to_string(&config).expect("config"), contents);
            fs::remove_file(config).expect("remove fixture collision");
        }
        fs::write(fixture.0.join("src/Обработка.xml"), "<broken>").expect("invalid XML");
        let before = snapshot(&fixture.0);
        let error = failure(
            &command(&fixture.0, locale)
                .args(["--workflow", "trunk"])
                .output()
                .expect("bad XML"),
            1,
            locale,
        );
        assert!(error.contains(if locale == "ru" {
            "Некорректный"
        } else {
            "Invalid or unsafe"
        }));
        assert_eq!(snapshot(&fixture.0), before);
    }
}

#[test]
fn ambiguity_requires_explicit_source_and_multiple_exports_are_rejected() {
    let fixture = TestDir::new();
    descriptor(&fixture.0, "report");
    descriptor(&fixture.0.join("src"), "configuration");
    assert!(matches!(
        init::inspect(&fixture.0, None),
        Err(InitError::AmbiguousSource { .. })
    ));
    let plan = init::inspect(&fixture.0, Some(Path::new("src"))).expect("explicit source");
    assert_eq!(plan.project_type(), ProjectType::Configuration);
    assert_eq!(plan.root(), fixture.0);
    assert_eq!(plan.source(), Path::new("src"));
    descriptor(&fixture.0.join("src"), "processing");
    assert!(matches!(
        init::inspect(&fixture.0, Some(Path::new("src"))),
        Err(InitError::MultipleDescriptors { .. })
    ));
    assert!(!fixture.0.join("eska.toml").exists());
}

#[test]
fn rejects_missing_sources_unsafe_paths_and_changes_after_detection() {
    let fixture = TestDir::new();
    assert!(matches!(
        init::inspect(&fixture.0, None),
        Err(InitError::MissingSource { .. })
    ));
    for path in [Path::new(""), Path::new("../outside"), fixture.0.as_path()] {
        assert!(matches!(
            init::inspect(&fixture.0, Some(path)),
            Err(InitError::Config(_))
        ));
    }
    descriptor(&fixture.0.join("src"), "configuration");
    let plan = init::inspect(&fixture.0, None).expect("plan");
    descriptor(&fixture.0.join("src"), "extension");
    assert!(matches!(
        init::apply(&plan, WorkflowPreset::Trunk, true),
        Err(InitError::ChangedSource { .. })
    ));
    assert!(!fixture.0.join("eska.toml").exists());
}

#[test]
fn rejects_unsupported_roots_config_directories_and_bare_repositories() {
    let fixture = TestDir::new();
    let xml = fixture.0.join("Configuration.xml");
    fs::write(&xml, "<Configuration/>").expect("not Designer XML");
    assert!(matches!(
        init::inspect(&fixture.0, None),
        Err(InitError::InvalidDescriptor { .. })
    ));
    fs::remove_file(xml).expect("remove fixture");
    descriptor(&fixture.0, "configuration");
    let config = fixture.0.join("eska.toml");
    fs::create_dir(&config).expect("collision");
    assert!(matches!(
        init::inspect(&fixture.0, None),
        Err(InitError::ExistingConfig { .. })
    ));
    fs::remove_dir(config).expect("remove fixture");
    let bare = fixture.0.join("bare");
    gix::init_bare(&bare).expect("bare repo");
    descriptor(&bare.join("src"), "report");
    let before = snapshot(&bare);
    let plan = init::inspect(&bare, None).expect("XML");
    assert!(matches!(
        init::apply(&plan, WorkflowPreset::Trunk, true),
        Err(InitError::ExistingGit { .. })
    ));
    assert_eq!(snapshot(&bare), before);
}

#[cfg(unix)]
#[test]
fn internal_source_symlink_is_canonicalized_without_becoming_ambiguous() {
    use std::os::unix::fs::symlink;
    let fixture = TestDir::new();
    descriptor(&fixture.0, "configuration");
    symlink(".", fixture.0.join("src")).expect("alias");
    let plan = init::inspect(&fixture.0, None).expect("same source");
    assert_eq!(plan.source(), Path::new("."));
    init::apply(&plan, WorkflowPreset::Trunk, false).expect("init");
    assert_eq!(
        fs::read_link(fixture.0.join("src")).expect("unchanged alias"),
        Path::new(".")
    );
}

#[test]
fn descriptor_size_is_bounded_and_dump_index_is_not_parsed() {
    let fixture = TestDir::new();
    descriptor(&fixture.0.join("src"), "configuration");
    // A sparse oversized file exercises the read cap without a large fixture.
    let oversized = fixture.0.join("src/large.xml");
    fs::File::create(&oversized)
        .expect("file")
        .set_len(64 * 1024 * 1024 + 1)
        .expect("size");
    assert!(matches!(
        init::inspect(&fixture.0, None),
        Err(InitError::DescriptorTooLarge { .. })
    ));
    fs::rename(oversized, fixture.0.join("src/ConfigDumpInfo.xml")).expect("dump index");
    assert!(
        init::inspect(&fixture.0, None).is_ok(),
        "no full dump index scan"
    );
}

#[test]
fn invalid_git_metadata_is_preserved_and_no_vcs_skips_it() {
    let fixture = TestDir::new();
    descriptor(&fixture.0.join("src"), "configuration");
    fs::write(fixture.0.join(".git"), "invalid gitfile").expect("gitfile");
    let before = snapshot(&fixture.0);
    failure(
        &command(&fixture.0, "en")
            .args(["--workflow", "trunk"])
            .output()
            .expect("bad gitfile"),
        1,
        "en",
    );
    assert_eq!(snapshot(&fixture.0), before);
    success(
        &command(&fixture.0, "en")
            .args(["--workflow", "trunk", "--no-vcs"])
            .output()
            .expect("no vcs"),
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join(".git")).expect("gitfile"),
        "invalid gitfile"
    );
}

#[cfg(unix)]
#[test]
fn symlink_escapes_and_dangling_config_are_not_followed_for_writes() {
    use std::os::unix::fs::symlink;
    let fixture = TestDir::new();
    let root = fixture.0.join("root");
    let outside = fixture.0.join("outside");
    fs::create_dir(&root).expect("root");
    descriptor(&outside, "configuration");
    let before = snapshot(&outside);
    symlink(&outside, root.join("src")).expect("source escape");
    assert!(matches!(
        init::inspect(&root, None),
        Err(InitError::InvalidSource { .. })
    ));
    fs::remove_file(root.join("src")).expect("remove fixture link");
    symlink(
        outside.join("Configuration.xml"),
        root.join("Configuration.xml"),
    )
    .expect("descriptor escape");
    assert!(matches!(
        init::inspect(&root, None),
        Err(InitError::InvalidSource { .. })
    ));
    fs::remove_file(root.join("Configuration.xml")).expect("remove fixture link");
    descriptor(&root, "configuration");
    symlink(outside.join("missing"), root.join("eska.toml")).expect("dangling config");
    assert!(matches!(
        init::inspect(&root, None),
        Err(InitError::ExistingConfig { .. })
    ));
    assert!(!outside.join("missing").exists());
    assert_eq!(snapshot(&outside), before);
}

#[test]
fn localized_help_and_global_base_work_with_default_path() {
    let fixture = TestDir::new();
    for locale in ["ru", "en"] {
        let help = success(
            &command(&fixture.0, locale)
                .arg("--help")
                .output()
                .expect("help"),
        );
        assert!(help.contains(if locale == "ru" {
            "eska init [ПАРАМЕТРЫ] [КАТАЛОГ]"
        } else {
            "eska init [OPTIONS] [DIRECTORY]"
        }));
        for option in [
            "--source",
            "--name",
            "--workflow",
            "--no-vcs",
            "--project-dir",
            "--lang",
        ] {
            assert!(help.contains(option));
        }
    }
    let standalone = fixture.0.join("standalone");
    descriptor(&standalone.join("src"), "report");
    let output = command(&standalone, "en")
        .args(["--name", "report", "--workflow", "trunk", "--no-vcs"])
        .output()
        .expect("standalone name rejection");
    failure(&output, 2, "en");
    assert!(!standalone.join("eska.toml").exists());
    let root = fixture.0.join("base");
    descriptor(&root.join("src"), "report");
    success(
        &command(&fixture.0, "ru")
            .arg("--project-dir")
            .arg(&root)
            .args(["--lang", "en", "--workflow", "git-flow", "--no-vcs"])
            .output()
            .expect("global base"),
    );
    assert!(root.join("eska.toml").exists());
    assert!(!fixture.0.join("eska.toml").exists());
}
