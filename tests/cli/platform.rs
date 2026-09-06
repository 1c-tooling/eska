#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, process::Command};

use serde_json::Value;

use crate::support::TestDir;

#[test]
fn lists_an_explicit_platform_with_stable_json() {
    let fixture = TestDir::new();
    let ibcmd = fixture.0.join("ibcmd");
    fs::write(&ibcmd, "#!/bin/sh\necho '1C ibcmd version 8.5.4.1234'\n").expect("fake ibcmd");
    let mut permissions = fs::metadata(&ibcmd).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&ibcmd, permissions).expect("permissions");
    let output = Command::new(env!("CARGO_BIN_EXE_eska"))
        .env("ESKA_CONFIG_DIR", fixture.0.join("settings"))
        .args([
            "--lang", "ru", "platform", "list", "--format", "json", "--ibcmd",
        ])
        .arg(&ibcmd)
        .output()
        .expect("list platforms");
    assert!(output.status.success(), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("platform JSON");
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["platforms"][0]["version"], "8.5.4.1234");
    assert_eq!(
        document["platforms"][0]["source"],
        ibcmd.to_string_lossy().as_ref()
    );
}
