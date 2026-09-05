use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use crate::support::TestDir;
use gix::bstr::ByteSlice;

/// Run Git only to construct and inspect isolated test repositories.
fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join("absent-global-config"))
        .env("GIT_AUTHOR_NAME", "Eska Test")
        .env("GIT_AUTHOR_EMAIL", "eska@example.invalid")
        .env("GIT_COMMITTER_NAME", "Eska Test")
        .env("GIT_COMMITTER_EMAIL", "eska@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00")
        .args(["-c", "core.hooksPath=", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("run Git fixture command")
}

/// Require a successful fixture Git command.
fn git_ok(root: &Path, args: &[&str]) {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Create a committed eska project that can act as a local clone source.
fn source_project(valid: bool) -> (TestDir, std::path::PathBuf) {
    let fixture = TestDir::new();
    let root = fixture.0.join("source project.git");
    fs::create_dir(&root).expect("source directory");
    git_ok(&root, &["init", "--initial-branch=main", "--template="]);
    fs::create_dir(root.join("src")).expect("source tree");
    fs::write(root.join("src/object.xml"), "<object/>\n").expect("source file");
    fs::write(
        root.join("eska.toml"),
        if valid {
            "[project]\ntype = \"configuration\"\n"
        } else {
            "[project]\ntype = \"unknown\"\n"
        },
    )
    .expect("project config");
    git_ok(&root, &["add", "."]);
    git_ok(&root, &["commit", "-m", "fixture"]);
    (fixture, root)
}

/// Render a local repository either as a path or as a canonical file URL.
fn repository_address(source: &Path, as_url: bool) -> String {
    if as_url {
        gix::Url::try_from(source)
            .expect("local repository URL")
            .with_request_alternate_form(false)
            .to_string()
    } else {
        source.to_str().expect("UTF-8 source path").to_owned()
    }
}

/// Run eska with a deterministic locale.
fn eska(base: &Path, locale: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(base)
        .env_remove("ESKA_LANG")
        .args(["--lang", locale])
        .args(args)
        .output()
        .expect("run eska")
}

/// Run eska without protocol helpers available in `PATH`.
fn eska_without_helpers(base: &Path, locale: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(base)
        .env_remove("ESKA_LANG")
        .env("PATH", "")
        .args(["--lang", locale])
        .args(args)
        .output()
        .expect("run eska")
}

#[test]
fn clones_local_path_and_file_url_in_both_locales() {
    for (locale, expected, as_url) in [
        ("ru", "Проект клонирован", false),
        ("en", "Project cloned", true),
    ] {
        let (_source_fixture, source) = source_project(true);
        let destination_fixture = TestDir::new();
        let source = repository_address(&source, as_url);
        let output = eska(&destination_fixture.0, locale, &["clone", &source]);

        assert!(output.status.success(), "{}", text(&output.stderr));
        assert!(output.stderr.is_empty());
        assert!(text(&output.stdout).contains(expected));
        let destination = destination_fixture.0.join("source project");
        assert_eq!(
            fs::read_to_string(destination.join("src/object.xml")).expect("checked out file"),
            "<object/>\n"
        );
        let repository = gix::open(&destination).expect("cloned repository");
        let head = repository.head().expect("HEAD");
        assert_eq!(
            head.referent_name().map(gix::refs::FullNameRef::as_bstr),
            Some(b"refs/heads/main".as_bstr())
        );
        assert!(repository.find_remote("origin").is_ok());
    }
}

#[test]
fn missing_local_transport_helper_is_localized_and_rolls_back() {
    for (locale, expected) in [
        ("ru", "Не удалось клонировать репозиторий через gix"),
        ("en", "Could not clone the repository with gix"),
    ] {
        let (_source_fixture, source) = source_project(true);
        let destination_fixture = TestDir::new();
        let output = eska_without_helpers(
            &destination_fixture.0,
            locale,
            &[
                "clone",
                source.to_str().expect("UTF-8 source path"),
                "missing-helper",
            ],
        );

        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(text(&output.stderr).contains(expected));
        assert!(!destination_fixture.0.join("missing-helper").exists());
    }
}

#[test]
fn custom_remote_and_explicit_directory_are_persisted() {
    let (_source_fixture, source) = source_project(true);
    let destination_fixture = TestDir::new();
    let output = eska(
        &destination_fixture.0,
        "en",
        &[
            "clone",
            source.to_str().expect("UTF-8 source path"),
            "working-copy",
            "--remote",
            "upstream",
        ],
    );

    assert!(output.status.success(), "{}", text(&output.stderr));
    let repository = gix::open(destination_fixture.0.join("working-copy")).expect("repository");
    assert!(repository.find_remote("upstream").is_ok());
    assert!(repository.find_remote("origin").is_err());
}

#[test]
fn invalid_project_rolls_back_only_new_destination() {
    for (locale, expected) in [
        ("ru", "Неизвестный тип проекта"),
        ("en", "Unknown project type"),
    ] {
        let (_source_fixture, source) = source_project(false);
        let destination_fixture = TestDir::new();
        let sentinel = destination_fixture.0.join("user-file");
        fs::write(&sentinel, "preserved").expect("sentinel");
        let output = eska(
            &destination_fixture.0,
            locale,
            &[
                "clone",
                source.to_str().expect("UTF-8 source path"),
                "invalid-project",
            ],
        );

        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(text(&output.stderr).contains(expected));
        assert!(!destination_fixture.0.join("invalid-project").exists());
        assert_eq!(fs::read_to_string(sentinel).expect("sentinel"), "preserved");
    }
}

#[test]
fn existing_destination_and_help_are_localized() {
    for (locale, help, exists) in [
        (
            "ru",
            "Клонировать и проверить существующий проект eska",
            "уже существует",
        ),
        (
            "en",
            "Clone and validate an existing eska project",
            "already exists",
        ),
    ] {
        let help_output = eska(Path::new("."), locale, &["clone", "--help"]);
        assert!(help_output.status.success());
        assert!(text(&help_output.stdout).contains(help));

        let (_source_fixture, source) = source_project(true);
        let destination_fixture = TestDir::new();
        let destination = destination_fixture.0.join("occupied");
        fs::create_dir(&destination).expect("existing destination");
        fs::write(destination.join("user-file"), "preserved").expect("user file");
        let output = eska(
            &destination_fixture.0,
            locale,
            &[
                "clone",
                source.to_str().expect("UTF-8 source path"),
                "occupied",
            ],
        );

        assert_eq!(output.status.code(), Some(1));
        assert!(text(&output.stderr).contains(exists));
        assert_eq!(
            fs::read_to_string(destination.join("user-file")).expect("user file"),
            "preserved"
        );
    }
}

/// Normalize Fluent isolation marks for readable assertions.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace(['\u{2068}', '\u{2069}'], "")
}
