//! Late caller cancellation must unwind the existing filesystem owner, including root enrollment.

use eska_project_core::{
    config::ProjectConfig,
    project::{
        ProjectName, ProjectType, create,
        discovery::{self, DiscoveryContext},
        init,
    },
};
use std::{fs, path::Path};

#[path = "../../../tests/support/mod.rs"]
mod support;

const WORKSPACE: &[u8] = b"# exact user comment\r\n[workspace]\r\nmembers = {}\r\n";
const XML: &[u8] = b"\xef\xbb\xbf<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><ExternalReport uuid=\"11111111-1111-1111-1111-111111111111\"><Properties><Name>Existing</Name></Properties></ExternalReport></MetaDataObject>\r\n";

/// A source tree includes both bounded XML and opaque bytes; neither may change during init.
fn export(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/Report.xml"), XML).unwrap();
    fs::write(root.join("src/Form.bin"), b"\0\xff\r\n").unwrap();
}

/// Fail only after enrollment has already published the new root manifest.
fn verify_enrollment(root: &Path) -> Result<(), &'static str> {
    if fs::read(root.join("eska.toml")).unwrap() == WORKSPACE {
        Ok(())
    } else {
        Err("cancelled")
    }
}

/// A cancellation at the final validation point restores new member files and the root manifest.
#[test]
fn late_member_creation_cancellation_restores_root_manifest_and_owned_parents() {
    let fixture = support::TestDir::new();
    fs::write(fixture.0.join("eska.toml"), WORKSPACE).unwrap();
    fs::write(fixture.0.join("user-file"), b"preserved").unwrap();
    let DiscoveryContext::Workspace { workspace, .. } =
        discovery::discover_context(&fixture.0).unwrap()
    else {
        panic!("workspace")
    };
    let plan = create::inspect_workspace_member::<&'static str>(
        &workspace,
        ProjectName::parse("member".into()).unwrap(),
    )
    .unwrap();
    let result = create::create_member_configured(
        &plan,
        &ProjectConfig::new(ProjectType::Report),
        None,
        || verify_enrollment(&fixture.0),
    );
    assert!(matches!(
        result,
        Err(create::CreationError::Operation("cancelled"))
    ));
    assert_eq!(fs::read(fixture.0.join("eska.toml")).unwrap(), WORKSPACE);
    assert_eq!(fs::read(fixture.0.join("user-file")).unwrap(), b"preserved");
    assert!(!fixture.0.join("src").exists());
}

/// Existing copied exports survive late attachment cancellation byte for byte.
#[test]
fn late_member_initialization_cancellation_restores_both_manifests_only() {
    let fixture = support::TestDir::new();
    fs::write(fixture.0.join("eska.toml"), WORKSPACE).unwrap();
    let target = fixture.0.join("src/copied");
    export(&target);
    let DiscoveryContext::Workspace { workspace, .. } =
        discovery::discover_context(&fixture.0).unwrap()
    else {
        panic!("workspace")
    };
    let detected = init::inspect::<&'static str>(&target, None).unwrap();
    let plan = init::prepare_workspace_member::<&'static str>(
        detected,
        &workspace,
        ProjectName::parse("copied".into()).unwrap(),
    )
    .unwrap();
    let result = init::apply_workspace_member(&plan, || verify_enrollment(&fixture.0));
    assert!(matches!(
        result,
        Err(init::InitError::Operation("cancelled"))
    ));
    assert_eq!(fs::read(fixture.0.join("eska.toml")).unwrap(), WORKSPACE);
    assert!(!target.join("eska.toml").exists());
    assert_eq!(fs::read(target.join("src/Report.xml")).unwrap(), XML);
    assert_eq!(
        fs::read(target.join("src/Form.bin")).unwrap(),
        b"\0\xff\r\n"
    );
    assert_eq!(fs::read_dir(target).unwrap().count(), 1);
}

/// Late cancellation after repository setup removes only newly claimed control files.
#[test]
fn late_standalone_initialization_cancellation_preserves_sources_and_user_rules() {
    let fixture = support::TestDir::new();
    export(&fixture.0);
    fs::write(fixture.0.join(".gitignore"), b"user rule\r\n").unwrap();
    let plan = init::inspect::<&'static str>(&fixture.0, None).unwrap();
    let config = ProjectConfig::new(ProjectType::Report);
    let result = init::apply_configured(
        &plan,
        &config,
        true,
        |git| {
            fs::write(git.join("HEAD"), b"ref: refs/heads/main\n").unwrap();
            Ok(())
        },
        || {
            if fixture.0.join(".git/HEAD").is_file() {
                Err("cancelled")
            } else {
                Ok(())
            }
        },
    );
    assert!(matches!(
        result,
        Err(init::InitError::Operation("cancelled"))
    ));
    assert!(!fixture.0.join("eska.toml").exists());
    assert!(!fixture.0.join(".git").exists());
    assert!(!fixture.0.join(".gitattributes").exists());
    assert_eq!(
        fs::read(fixture.0.join(".gitignore")).unwrap(),
        b"user rule\r\n"
    );
    assert_eq!(fs::read(fixture.0.join("src/Report.xml")).unwrap(), XML);
    assert_eq!(
        fs::read(fixture.0.join("src/Form.bin")).unwrap(),
        b"\0\xff\r\n"
    );
}
