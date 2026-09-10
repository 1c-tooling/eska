use std::{fs, process::Command};

use crate::support::TestDir;

use super::build::{fake_ibcmd, project, workspace};

#[test]
/// Remove only the managed infobase and keep the published artifact.
fn clean_removes_managed_infobase_and_preserves_artifact() {
    let fixture = TestDir::new();
    let root = project(&fixture, "configuration", "Cleanable");
    let ibcmd = fake_ibcmd(&fixture);
    let build = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .args(["--lang", "en", "build", "--ibcmd"])
        .arg(&ibcmd)
        .output()
        .expect("build project");
    assert!(build.status.success(), "{build:?}");
    let artifact = root.join("build/Cleanable.cf");
    let infobase = root.join("build/.eska/infobases/Cleanable.cf");
    assert!(artifact.is_file());
    assert!(infobase.is_dir());

    let clean = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .args(["--lang", "ru", "clean"])
        .output()
        .expect("clean project");
    assert!(clean.status.success(), "{clean:?}");
    assert!(String::from_utf8_lossy(&clean.stdout).contains("Удалена служебная база"));
    assert!(artifact.is_file());
    assert!(!infobase.exists());

    let repeated = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .args(["--lang", "en", "clean"])
        .output()
        .expect("repeat clean");
    assert!(repeated.status.success(), "{repeated:?}");
    assert!(String::from_utf8_lossy(&repeated.stdout).contains("already absent"));
}

#[test]
/// Clean every selected workspace member without deleting shared artifacts.
fn clean_workspace_removes_each_managed_infobase() {
    let fixture = workspace();
    let ibcmd = fake_ibcmd(&fixture);
    let build = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&fixture.0)
        .args(["--lang", "en", "build", "--workspace", "--ibcmd"])
        .arg(&ibcmd)
        .output()
        .expect("build workspace");
    assert!(build.status.success(), "{build:?}");
    let report = fixture.0.join("build/.eska/infobases/sales-report.erf");
    let processing = fixture.0.join("build/.eska/infobases/import-orders.epf");
    assert!(report.is_dir());
    assert!(processing.is_dir());

    let clean = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&fixture.0)
        .args(["--lang", "en", "clean", "--workspace"])
        .output()
        .expect("clean workspace");
    assert!(clean.status.success(), "{clean:?}");
    assert!(!report.exists());
    assert!(!processing.exists());
    assert!(fixture.0.join("build/sales-report.erf").is_file());
    assert!(fixture.0.join("build/import-orders.epf").is_file());
}

#[test]
/// Refuse to delete a colliding directory without the eska ownership marker.
fn clean_preserves_unowned_directory() {
    let fixture = TestDir::new();
    let root = project(&fixture, "configuration", "Unowned");
    let infobase = root.join("build/.eska/infobases/Unowned.cf");
    fs::create_dir_all(&infobase).expect("create colliding directory");
    fs::write(infobase.join("user.txt"), b"keep").expect("write user file");

    let clean = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .args(["--lang", "en", "clean"])
        .output()
        .expect("clean unowned path");
    assert_eq!(clean.status.code(), Some(1), "{clean:?}");
    assert!(String::from_utf8_lossy(&clean.stderr).contains("not owned by eska"));
    assert_eq!(
        fs::read(infobase.join("user.txt")).expect("user file"),
        b"keep"
    );
}

#[test]
/// Localize the clean command help in both supported languages.
fn clean_help_is_localized() {
    for (locale, expected) in [
        ("ru", "Удалить служебную базу сборки"),
        ("en", "Remove the managed build infobase"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska"))
            .args(["--lang", locale, "clean", "--help"])
            .output()
            .expect("clean help");
        assert!(output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains(expected));
    }
}
