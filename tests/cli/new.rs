use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use eska::{
    config::ProjectConfig,
    project::ProjectType,
    project::create::{self, CreationError},
    project::discovery,
    vcs::workflow::WorkflowPreset,
};

use crate::support::TestDir;

fn command(root: &Path, locale: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_eska"));
    command
        .current_dir(root)
        .env_remove("ESKA_LANG")
        .args(["--lang", locale]);
    command
}

fn success(output: &Output) -> String {
    assert!(
        !output.stdout.contains(&0x1b),
        "no TUI escape sequences outside a terminal"
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout.clone()).expect("UTF-8")
}

#[test]
fn cli_creates_all_types_and_presets_in_both_locales() {
    let fixture = TestDir::new();
    for locale in ["ru", "en"] {
        for kind in ["configuration", "extension", "processing", "report"] {
            for workflow in ["trunk", "git-flow", "github-flow", "custom"] {
                let name = format!("{locale}-{kind}-{workflow} проект");
                let output = command(&fixture.0, locale)
                    .args([
                        "new",
                        &name,
                        "--type",
                        kind,
                        "--workflow",
                        workflow,
                        "--no-vcs",
                    ])
                    .output()
                    .expect("new");
                let text = success(&output);
                assert!(
                    text.contains(if locale == "ru" {
                        "Каркас проекта создан"
                    } else {
                        "Project scaffold created"
                    }),
                    "{text}"
                );
                let root = fixture.0.join(&name);
                assert!(text.contains(root.to_str().expect("UTF-8 path")));
                assert!(root.join("src/.gitkeep").is_file());
                assert!(root.join(".gitattributes").is_file());
                assert!(root.join(".gitignore").is_file());
                assert!(!root.join(".git").exists());
                let config = ProjectConfig::load(&root.join("eska.toml")).expect("config");
                assert_eq!(
                    config.configuration().workflow(),
                    WorkflowPreset::from_name(workflow)
                );
                assert!(
                    config
                        .to_toml()
                        .expect("TOML")
                        .contains(&format!("type = \"{kind}\""))
                );
                let project = discovery::discover(&root.join("src")).expect("discover");
                assert_eq!(project.configuration(), config.configuration());
                assert!(success(&command(&root, locale).output().expect("validate")).is_empty());
            }
        }
    }
}

#[test]
fn git_initialization_is_local_isolated_and_has_no_commit() {
    let fixture = TestDir::new();
    let sentinel = fixture.0.join("untouched");
    fs::create_dir(&sentinel).expect("sentinel directory");
    let output = command(&fixture.0, "en")
        .args([
            "new",
            "with-git",
            "--type",
            "configuration",
            "--workflow",
            "git-flow",
        ])
        .env("GIT_DIR", &sentinel)
        .env("GIT_WORK_TREE", &sentinel)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "init.defaultBranch")
        .env("GIT_CONFIG_VALUE_0", "unwanted")
        .env("PATH", "")
        .output()
        .expect("new with embedded Git");
    success(&output);
    assert_eq!(fs::read_dir(&sentinel).expect("read sentinel").count(), 0);
    let root = fixture.0.join("with-git");
    let repo = gix::open_opts(&root, gix::open::Options::isolated()).expect("valid Git repository");
    assert_eq!(repo.workdir(), Some(root.as_path()));
    assert!(repo.head().expect("HEAD").is_unborn());
    assert_eq!(
        fs::read_to_string(root.join(".git/HEAD")).expect("HEAD"),
        "ref: refs/heads/main\n"
    );
    assert_eq!(
        fs::read_dir(root.join(".git/refs/heads"))
            .expect("heads")
            .count(),
        0
    );
    assert!(
        !fs::read_to_string(root.join(".git/config"))
            .expect("git config")
            .contains("remote")
    );
}

#[test]
fn collisions_preserve_existing_files_and_directories() {
    let fixture = TestDir::new();
    fs::write(fixture.0.join("file"), "user data").expect("file");
    fs::create_dir(fixture.0.join("empty")).expect("empty");
    fs::create_dir(fixture.0.join("occupied")).expect("occupied");
    fs::write(fixture.0.join("occupied/eska.toml"), "user config").expect("config");
    for locale in ["ru", "en"] {
        for name in ["file", "empty", "occupied"] {
            let output = command(&fixture.0, locale)
                .args(["new", name, "--type", "report", "--workflow", "custom"])
                .output()
                .expect("collision");
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
            let error = String::from_utf8(output.stderr).expect("UTF-8");
            assert!(
                error.contains(if locale == "ru" {
                    "уже существует"
                } else {
                    "already exists"
                }),
                "{error}"
            );
        }
    }
    assert_eq!(
        fs::read_to_string(fixture.0.join("file")).expect("file preserved"),
        "user data"
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("occupied/eska.toml")).expect("config preserved"),
        "user config"
    );
    assert_eq!(
        fs::read_dir(fixture.0.join("empty"))
            .expect("empty preserved")
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.0.join("occupied"))
            .expect("occupied preserved")
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn destination_symlinks_are_never_followed_or_replaced() {
    use std::os::unix::fs::symlink;
    let fixture = TestDir::new();
    let target = fixture.0.join("target");
    fs::create_dir(&target).expect("target");
    for (name, destination) in [
        ("link", target.clone()),
        ("dangling", fixture.0.join("missing")),
    ] {
        let link = fixture.0.join(name);
        symlink(&destination, &link).expect("symlink");
        assert!(matches!(
            create::create(&link, ProjectType::Report, WorkflowPreset::Trunk, true),
            Err(CreationError::AlreadyExists { .. })
        ));
        assert_eq!(fs::read_link(&link).expect("unchanged link"), destination);
    }
    assert_eq!(fs::read_dir(target).expect("unchanged target").count(), 0);
    assert!(!fixture.0.join("missing").exists());
}

#[test]
fn invalid_or_missing_options_fail_without_creating_any_files() {
    let fixture = TestDir::new();
    for locale in ["ru", "en"] {
        for options in [
            vec![],
            vec!["--type", "report"],
            vec!["--workflow", "trunk"],
            vec!["--type", "invalid", "--workflow", "trunk"],
            vec!["--type", "report", "--workflow", "invalid"],
        ] {
            let output = command(&fixture.0, locale)
                .args(["new", "missing"])
                .args(options)
                .output()
                .expect("invalid arguments");
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
            assert!(!output.stderr.is_empty());
            let error = String::from_utf8(output.stderr).expect("UTF-8");
            assert_eq!(
                error.chars().any(|ch| ('А'..='я').contains(&ch)),
                locale == "ru",
                "{error}"
            );
            assert!(!error.contains("1)"), "must not prompt without TTY");
        }
    }
    assert_eq!(
        fs::read_dir(&fixture.0)
            .expect("unchanged directory")
            .count(),
        0
    );
}

#[test]
fn invalid_paths_do_not_create_partial_parents() {
    let fixture = TestDir::new();
    for name in [".", "..", "missing/child", "child/../other"] {
        let output = command(&fixture.0, "en")
            .args(["new", name, "--type", "report", "--workflow", "trunk"])
            .output()
            .expect("invalid path");
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    assert_eq!(
        fs::read_dir(&fixture.0)
            .expect("unchanged directory")
            .count(),
        0
    );
}

#[test]
fn global_options_work_after_subcommand_and_help_needs_no_project() {
    let fixture = TestDir::new();
    let base = fixture.0.join("base");
    fs::create_dir(&base).expect("base");
    success(
        &command(&fixture.0, "ru")
            .args(["new", "demo", "--lang", "en", "--project-dir"])
            .arg(&base)
            .args([
                "--type",
                "extension",
                "--workflow",
                "github-flow",
                "--no-vcs",
            ])
            .output()
            .expect("global options"),
    );
    assert!(base.join("demo/eska.toml").is_file());
    assert!(!fixture.0.join("demo").exists());
    for (locale, usage, unexpected) in [
        (
            "ru",
            "Использование: eska new [ПАРАМЕТРЫ] <КАТАЛОГ>",
            "Options:",
        ),
        ("en", "Usage: eska new [OPTIONS] <DIRECTORY>", "Параметры:"),
    ] {
        let output = command(&fixture.0, locale)
            .args(["new", "--help"])
            .output()
            .expect("help");
        let help = success(&output);
        assert!(help.contains(usage), "{help}");
        assert!(!help.contains(unexpected));
        for option in [
            "--type",
            "--workflow",
            "--no-vcs",
            "--lang",
            "--project-dir",
        ] {
            assert!(help.contains(option), "{help}");
        }
    }
}
