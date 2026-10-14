//! Compound onboarding refuses an unacknowledged root and preserves post-creation manifest edits.

use super::*;
use crate::test_support::TestDir;

/// A real successful process status accompanies the fixture's machine response.
fn success() -> std::process::ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }
}

/// Fixture acknowledgment binds the original constructor manifest before it becomes editable.
fn response(root: &Path, manifest: &str) -> Captured {
    let value = json!({"schema_version":1,"tool":"eska-project","command":"new","status":"success",
        "data":{"root":root,"name":"demo","source":root.join("src"),"source_format":"eska",
        "source_layout":"queries","accepted_base":".eska/accepted","manifest_sha256":digest_bytes(manifest.as_bytes()),
        "accepted_base_sha256":"a".repeat(64),"ancestor_sha256":"b".repeat(64)}});
    Captured {
        status: success(),
        stdout: serde_json::to_vec(&value).unwrap(),
        oversized: false,
    }
}

#[test]
/// An external workflow edit cannot become the expected input of the parent's finalization write.
fn rejects_changed_manifest_without_erasing_created_or_intervening_files() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    fs::create_dir(&root).unwrap();
    let original = "[project]\nname='demo'\ntype='configuration'\nsource_format='eska'\n[eska]\naccepted_base='.eska/accepted'\n";
    fs::write(root.join("eska.toml"), original).unwrap();
    let report = response(&root, original);
    let created = created(
        &report,
        Operation::New,
        &root,
        &ProjectName::parse("demo".to_owned()).unwrap(),
        Path::new("src"),
    )
    .unwrap();
    assert!(manifest(&created).is_ok());
    let changed = format!("{original}\n# intervening user edit\n");
    fs::write(root.join("eska.toml"), &changed).unwrap();
    fs::write(root.join("user.txt"), "retained").unwrap();
    assert!(matches!(manifest(&created), Err(Error::ManifestChanged)));
    let receipt = unfinished(&created, "manifest-finalization", false);
    assert_eq!(receipt["destination_removed"], false);
    assert_eq!(receipt["support_approved"], false);
    assert_eq!(receipt["onboarding_complete"], false);
    assert_eq!(fs::read_to_string(root.join("eska.toml")).unwrap(), changed);
    assert_eq!(
        fs::read_to_string(root.join("user.txt")).unwrap(),
        "retained"
    );
}

#[test]
/// A candidate digest must acknowledge this exact root, label, layout and direct command.
fn refuses_foreign_or_incomplete_acknowledgments() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    let report = response(&root, "initial");
    let name = ProjectName::parse("demo".to_owned()).unwrap();
    assert!(matches!(
        created(&report, Operation::Import, &root, &name, Path::new("src")),
        Err(Error::ResponseInvalid)
    ));
    assert!(matches!(
        created(
            &report,
            Operation::New,
            &fixture.0.join("foreign"),
            &name,
            Path::new("src")
        ),
        Err(Error::ResponseInvalid)
    ));
    for field in ["manifest_sha256", "accepted_base_sha256", "ancestor_sha256"] {
        let mut value: Value = serde_json::from_slice(&report.stdout).unwrap();
        value["data"][field] = json!("not-a-pin");
        let modified = Captured {
            status: success(),
            stdout: serde_json::to_vec(&value).unwrap(),
            oversized: false,
        };
        assert!(matches!(
            created(&modified, Operation::New, &root, &name, Path::new("src")),
            Err(Error::ResponseInvalid)
        ));
    }
}
