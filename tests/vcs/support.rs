use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use gix::ObjectId;

use crate::support::TestDir;

pub fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = git_output(root, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

pub fn git_output(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new("git");
    // Test fixture commands must not inherit developer Git configuration or redirects.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
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

pub fn repository() -> TestDir {
    let dir = TestDir::new();
    git(&dir.0, &["init", "--initial-branch=main", "--template="]);
    dir
}

pub fn commit(root: &Path, name: &str) -> ObjectId {
    fs::write(root.join(name), format!("{name}\n")).expect("write fixture file");
    git(root, &["add", "--", name]);
    git(root, &["commit", "-m", name]);
    let id = git(root, &["rev-parse", "HEAD"]);
    ObjectId::from_hex(id.trim_ascii()).expect("commit ID")
}
