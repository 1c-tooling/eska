//! Minimal installations and repository-wide safety through the actual standalone process.

use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    path::Path,
    process::{Command, Output},
};

#[path = "../../../tests/support/mod.rs"]
mod support;
use support::TestDir;

/// Execute fixture Git with deterministic identity and no developer configuration or redirects.
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    let output = command
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join("absent-config"))
        .env("GIT_AUTHOR_NAME", "Eska Test")
        .env("GIT_AUTHOR_EMAIL", "eska@example.invalid")
        .env("GIT_COMMITTER_NAME", "Eska Test")
        .env("GIT_COMMITTER_EMAIL", "eska@example.invalid")
        .args(["-c", "core.hooksPath=", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    output.stdout
}

/// Execute the tool with only the explicitly selected executables available to subprocesses.
fn tool(root: &Path, path: &OsStr, locale: &str, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_eska-vcs"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .current_dir(root)
        .env("PATH", path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join("absent-config"))
        .env("GIT_AUTHOR_NAME", "Eska Test")
        .env("GIT_AUTHOR_EMAIL", "eska@example.invalid")
        .env("GIT_COMMITTER_NAME", "Eska Test")
        .env("GIT_COMMITTER_EMAIL", "eska@example.invalid")
        .env("NO_COLOR", "1")
        .args(["--lang", locale])
        .arg("--project-dir")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

/// Require a complete successful JSON response without hidden stderr or terminal decoration.
fn json(output: &Output) -> Value {
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(!output.stdout.contains(&0x1b));
    serde_json::from_slice(&output.stdout).unwrap()
}

/// All declared command help works before discovery and names the standalone executable.
#[test]
fn help_and_version_need_no_project_or_other_components() {
    let fixture = TestDir::new();
    for locale in ["ru", "en"] {
        for command in [
            "clone", "status", "diff", "history", "start", "switch", "save", "finish", "shelve",
            "unshelve", "shelves",
        ] {
            let output = tool(&fixture.0, OsStr::new(""), locale, &[command, "--help"]);
            assert!(output.status.success(), "{command}: {output:?}");
            assert_eq!(output.stderr, [] as [u8; 0]);
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(text.contains(&format!("eska-vcs {command}")), "{text}");
            assert!(text.contains(if locale == "ru" {
                "Использование:"
            } else {
                "Usage:"
            }));
        }
        let output = tool(&fixture.0, OsStr::new(""), locale, &["--version"]);
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("eska-vcs {}\n", env!("CARGO_PKG_VERSION"))
        );
        let invalid = tool(&fixture.0, OsStr::new(""), locale, &["diff", "--unknown"]);
        assert_eq!(invalid.status.code(), Some(2));
    }
    assert!(fs::read_dir(&fixture.0).unwrap().next().is_none());
}

/// Read-only operations require neither system Git nor the other ESKA binaries on PATH.
#[test]
fn every_project_type_is_readable_in_a_minimal_installation() {
    for kind in ["configuration", "extension", "processing", "report"] {
        let fixture = TestDir::new();
        fs::create_dir(fixture.0.join("src")).unwrap();
        fs::write(
            fixture.0.join("eska.toml"),
            format!("[project]\ntype = '{kind}'\n[vcs.workflow]\npreset = 'trunk'\n"),
        )
        .unwrap();
        fs::write(fixture.0.join("src/module.bsl"), b"base\r\n").unwrap();
        git(
            &fixture.0,
            &["init", "--initial-branch=main", "--template="],
        );
        git(&fixture.0, &["add", "."]);
        git(&fixture.0, &["commit", "-m", "base"]);
        fs::write(fixture.0.join("src/module.bsl"), b"pending\r\n").unwrap();
        let head = fs::read(fixture.0.join(".git/HEAD")).unwrap();
        let index = fs::read(fixture.0.join(".git/index")).unwrap();
        for command in ["status", "diff", "history", "shelves"] {
            let ru = json(&tool(
                &fixture.0,
                OsStr::new(""),
                "ru",
                &[command, "--format", "json"],
            ));
            let en = json(&tool(
                &fixture.0,
                OsStr::new(""),
                "en",
                &[command, "--format", "json"],
            ));
            assert_eq!(ru, en, "{kind}/{command}");
            assert_eq!(
                ru["schema_version"],
                if command == "status" { 3 } else { 1 }
            );
        }
        let missing_git = tool(&fixture.0, OsStr::new(""), "en", &["start", "NO-GIT"]);
        assert_eq!(missing_git.status.code(), Some(1));
        assert_eq!(
            git(&fixture.0, &["branch", "--list", "task/NO-GIT"]),
            [] as [u8; 0]
        );
        assert_eq!(fs::read(fixture.0.join(".git/HEAD")).unwrap(), head);
        assert_eq!(fs::read(fixture.0.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read(fixture.0.join("src/module.bsl")).unwrap(),
            b"pending\r\n"
        );
        assert!(!fixture.0.join(".eska").exists());
    }
}

/// The bridge scopes changed paths to the selected source and remains independent of locale/Git CLI.
#[test]
fn read_only_bridge_keeps_provenance_and_source_scope() {
    let fixture = TestDir::new();
    fs::create_dir(fixture.0.join("src")).unwrap();
    fs::write(
        fixture.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    fs::write(fixture.0.join("src/module.bsl"), b"base\r\n").unwrap();
    fs::write(fixture.0.join("outside.txt"), b"base").unwrap();
    git(
        &fixture.0,
        &["init", "--initial-branch=main", "--template="],
    );
    git(&fixture.0, &["add", "."]);
    git(&fixture.0, &["commit", "-m", "base"]);
    let commit = String::from_utf8(git(&fixture.0, &["rev-parse", "HEAD"])).unwrap();
    let head = fs::read(fixture.0.join(".git/HEAD")).unwrap();
    let index = fs::read(fixture.0.join(".git/index")).unwrap();
    fs::write(fixture.0.join("outside.txt"), b"changed outside source").unwrap();
    for locale in ["ru", "en"] {
        let provenance = json(&tool(
            &fixture.0,
            OsStr::new(""),
            locale,
            &["__bridge", "provenance"],
        ));
        assert_eq!(provenance["schema_version"], 1);
        assert_eq!(provenance["operation"], "provenance");
        assert_eq!(provenance["data"]["status"], "available");
        assert_eq!(provenance["data"]["commit"], commit.trim());
        assert_eq!(provenance["data"]["dirty"], true);
        let changes = json(&tool(
            &fixture.0,
            OsStr::new(""),
            locale,
            &["__bridge", "source-changes", "--files", "1"],
        ));
        assert_eq!(changes["operation"], "source_changes");
        assert_eq!(changes["data"]["local_changes"], 0);
    }
    fs::write(fixture.0.join("src/module.bsl"), b"pending\r\n").unwrap();
    let changes = json(&tool(
        &fixture.0,
        OsStr::new(""),
        "en",
        &["__bridge", "source-changes", "--files", "1"],
    ));
    assert_eq!(changes["data"]["local_changes"], 1);
    assert_eq!(fs::read(fixture.0.join(".git/HEAD")).unwrap(), head);
    assert_eq!(fs::read(fixture.0.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.0.join("src/module.bsl")).unwrap(),
        b"pending\r\n"
    );
}

/// Broken repository metadata cannot be reported as a clean source; Git-less fallback stays explicit.
#[test]
fn source_bridge_distinguishes_absent_and_broken_git() {
    let fixture = TestDir::new();
    fs::create_dir(fixture.0.join("src")).unwrap();
    fs::write(
        fixture.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    fs::write(fixture.0.join("src/module.bsl"), b"user bytes\r\n").unwrap();
    for locale in ["ru", "en"] {
        let absent = json(&tool(
            &fixture.0,
            OsStr::new(""),
            locale,
            &["__bridge", "provenance"],
        ));
        assert_eq!(absent["data"]["status"], "absent");
        let changes = json(&tool(
            &fixture.0,
            OsStr::new(""),
            locale,
            &["__bridge", "source-changes", "--files", "1"],
        ));
        assert_eq!(changes["data"]["local_changes"], 1);
    }
    fs::create_dir(fixture.0.join(".git")).unwrap();
    for locale in ["ru", "en"] {
        let unknown = json(&tool(
            &fixture.0,
            OsStr::new(""),
            locale,
            &["__bridge", "provenance"],
        ));
        assert_eq!(unknown["data"]["status"], "unavailable");
        let output = tool(
            &fixture.0,
            OsStr::new(""),
            locale,
            &["__bridge", "source-changes", "--files", "1"],
        );
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(output.stderr, [] as [u8; 0]);
        let error: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(error["data"].is_null());
        assert_eq!(error["error"], "repository");
    }
    assert_eq!(
        fs::read(fixture.0.join("src/module.bsl")).unwrap(),
        b"user bytes\r\n"
    );
}

/// Both initialization forms target only the claimed namespace and preserve existing Git bytes.
#[test]
fn initialization_bridge_preserves_existing_repository_metadata() {
    let fixture = TestDir::new();
    for locale in ["ru", "en"] {
        for reserved in [false, true] {
            let root = fixture.0.join(format!("{locale}-{reserved}"));
            fs::create_dir(&root).unwrap();
            fs::write(root.join("user-file"), b"preserved\r\n").unwrap();
            let exists = json(&tool(
                &root,
                OsStr::new(""),
                locale,
                &["__bridge", "repository-exists"],
            ));
            assert_eq!(exists["data"]["exists"], false);
            let target = if reserved {
                fs::create_dir(root.join(".git")).unwrap();
                root.join(".git")
            } else {
                root.clone()
            };
            let operation = if reserved {
                "initialize-reserved"
            } else {
                "initialize"
            };
            let initialized = json(&tool(
                &target,
                OsStr::new(""),
                locale,
                &["__bridge", operation],
            ));
            assert_eq!(initialized["data"]["initialized"], true);
            assert!(!root.join(".git/eska-init").exists());
            let head = fs::read(root.join(".git/HEAD")).unwrap();
            let config = fs::read(root.join(".git/config")).unwrap();
            assert_eq!(
                json(&tool(
                    &root,
                    OsStr::new(""),
                    locale,
                    &["__bridge", "repository-exists"]
                ))["data"]["exists"],
                true
            );
            for (directory, operation) in [
                (&root, "initialize"),
                (&root.join(".git"), "initialize-reserved"),
            ] {
                let rejected = tool(directory, OsStr::new(""), locale, &["__bridge", operation]);
                assert_eq!(rejected.status.code(), Some(1));
                let body: Value = serde_json::from_slice(&rejected.stdout).unwrap();
                assert_eq!(body["error"], "git_collision");
            }
            assert_eq!(fs::read(root.join(".git/HEAD")).unwrap(), head);
            assert_eq!(fs::read(root.join(".git/config")).unwrap(), config);
            assert_eq!(fs::read(root.join("user-file")).unwrap(), b"preserved\r\n");
        }
    }
}

/// Only the documented Git capability fallback is present during task and shelf mutations.
#[cfg(unix)]
#[test]
fn task_switch_and_shelves_preserve_both_stages_without_other_components() {
    let fixture = TestDir::new();
    let bin = fixture.0.join("bin");
    fs::create_dir(&bin).unwrap();
    let git_path = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join("git"))
        .find(|path| path.is_file())
        .unwrap();
    std::os::unix::fs::symlink(fs::canonicalize(git_path).unwrap(), bin.join("git")).unwrap();
    for locale in ["ru", "en"] {
        let root = fixture.0.join(format!("проект с пробелом {locale}"));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(
            root.join("eska.toml"),
            "[project]\ntype='configuration'\n[vcs.workflow]\npreset='trunk'\n",
        )
        .unwrap();
        fs::write(root.join("src/module.bsl"), b"base\r\n").unwrap();
        git(&root, &["init", "--initial-branch=main", "--template="]);
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "base"]);
        let id = format!("T95-{locale}");
        let start = tool(&root, bin.as_os_str(), locale, &["start", &id]);
        assert!(start.status.success(), "{start:?}");
        fs::write(root.join("src/module.bsl"), b"staged\r\n").unwrap();
        git(&root, &["add", "src/module.bsl"]);
        fs::write(root.join("src/module.bsl"), b"unstaged\r\n").unwrap();
        fs::write(root.join("untracked кириллица"), b"untracked\r\n").unwrap();
        let index = fs::read(root.join(".git/index")).unwrap();
        json(&tool(
            &root,
            bin.as_os_str(),
            locale,
            &["switch", "--base", "--format", "json"],
        ));
        assert_eq!(fs::read(root.join("src/module.bsl")).unwrap(), b"base\r\n");
        assert!(!root.join("untracked кириллица").exists());
        json(&tool(
            &root,
            bin.as_os_str(),
            locale,
            &["switch", &id, "--format", "json"],
        ));
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read(root.join("src/module.bsl")).unwrap(),
            b"unstaged\r\n"
        );
        assert_eq!(
            fs::read(root.join("untracked кириллица")).unwrap(),
            b"untracked\r\n"
        );
        let saved = tool(&root, bin.as_os_str(), locale, &["save", "-m", "own task"]);
        assert!(saved.status.success(), "{saved:?}");
        let refused = tool(&root, bin.as_os_str(), locale, &["finish"]);
        assert_eq!(refused.status.code(), Some(1));
        assert_eq!(
            git(&root, &["branch", "--show-current"]).trim_ascii(),
            format!("task/{id}").as_bytes()
        );
        git(&root, &["switch", "main"]);
        git(&root, &["merge", "--ff-only", &format!("task/{id}")]);
        git(&root, &["switch", &format!("task/{id}")]);
        let finished = tool(&root, bin.as_os_str(), locale, &["finish"]);
        assert!(finished.status.success(), "{finished:?}");
        assert_eq!(
            git(&root, &["branch", "--show-current"]).trim_ascii(),
            b"main"
        );
        assert_eq!(
            git(&root, &["branch", "--list", &format!("task/{id}")]),
            [] as [u8; 0]
        );
    }
}
