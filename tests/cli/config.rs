#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, process::Command};

use crate::support::TestDir;

#[test]
fn initializes_and_transactionally_edits_global_config() {
    let fixture = TestDir::new();
    let config_dir = fixture.0.join("settings");
    let init = Command::new(env!("CARGO_BIN_EXE_eska"))
        .env("ESKA_CONFIG_DIR", &config_dir)
        .args(["--lang", "en", "config", "init"])
        .output()
        .expect("initialize config");
    assert!(init.status.success(), "{init:?}");
    let config = config_dir.join("config.toml");
    assert_eq!(
        fs::read_to_string(&config).expect("config"),
        "[build]\nrunner = \"auto\"\n"
    );

    let editor = fixture.0.join("editor");
    fs::write(
        &editor,
        "#!/bin/sh\nprintf '[build]\\nrunner = \"host\"\\n' > \"$1\"\n",
    )
    .expect("editor script");
    let mut permissions = fs::metadata(&editor).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&editor, permissions).expect("permissions");
    let edit = Command::new(env!("CARGO_BIN_EXE_eska"))
        .env("ESKA_CONFIG_DIR", &config_dir)
        .env("ESKA_EDITOR", &editor)
        .args(["--lang", "en", "config", "edit"])
        .output()
        .expect("edit config");
    assert!(edit.status.success(), "{edit:?}");
    assert_eq!(
        fs::read_to_string(&config).expect("updated config"),
        "[build]\nrunner = \"host\"\n"
    );
    let backups: Vec<_> = fs::read_dir(&config_dir)
        .expect("config directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains("backup"))
        .collect();
    assert_eq!(backups.len(), 1);
}
