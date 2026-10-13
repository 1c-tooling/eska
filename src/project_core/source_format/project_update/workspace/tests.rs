//! Supplies by member name preserve inheritance and cannot authorize a partially updated workspace.

use super::*;
use crate::{
    project_core::{
        config::ProjectConfig,
        creation,
        discovery::{DiscoveryContext, discover_context},
        eska_source::EskaSource,
        metadata_model::ProjectScope,
        platform_settings::PlatformVersion,
        source_format::{LayoutPublication, NativeUuid, files::Inventory},
    },
    test_support::TestDir,
};
use std::fs;

/// Two profiles and different incoming paths/order force name-based rather than positional selection.
fn fixture(test: &TestDir, label: &str, frozen: bool) -> (Workspace, WorkspaceAcceptedRefs) {
    let root = test.0.join(label);
    let folder = if label == "old" {
        "projects"
    } else {
        "supplies"
    };
    fs::create_dir_all(root.join(folder)).unwrap();
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    let inventory = Inventory::read(&input).unwrap();
    let mut pins = BTreeMap::new();
    for (name, extension) in [("one", false), ("two", true)] {
        let native = test.0.join(format!("{label}-{name}-native"));
        fs::create_dir(&native).unwrap();
        for path in &inventory.directories {
            fs::create_dir_all(native.join(path.as_path())).unwrap();
        }
        for path in inventory.files.keys() {
            fs::copy(input.join(path.as_path()), native.join(path.as_path())).unwrap();
        }
        fs::remove_file(native.join("Ext/ParentConfigurations.bin")).unwrap();
        let descriptor = native.join("Configuration.xml");
        let mut xml = fs::read_to_string(&descriptor).unwrap();
        if extension {
            xml = xml.replacen("<Properties>", "<Properties><ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>", 1);
        }
        if label != "old" {
            xml = xml.replace("Configuration &amp; resources", "Supplier update");
        }
        fs::write(descriptor, xml).unwrap();
        let frozen = if frozen && extension {
            vec![NativeUuid::parse("60000000-0000-4000-8000-000000000001").unwrap()]
        } else {
            Vec::new()
        };
        let platform = extension.then(|| PlatformVersion::parse("8.3.26.1000").unwrap());
        let imported = creation::import_readable_project(
            &native,
            &root.join(folder).join(name),
            ProjectName::parse(name.into()).unwrap(),
            platform.as_ref(),
            &frozen,
        )
        .unwrap();
        pins.insert(name.to_owned(), imported.accepted.sha256);
        let manifest = imported.project.root().join("eska.toml");
        fs::write(
            &manifest,
            format!(
                "# Member {name}\r\n{}",
                ProjectConfig::load(&manifest)
                    .unwrap()
                    .to_workspace_member_toml()
                    .unwrap()
            ),
        )
        .unwrap();
    }
    let members = if label == "old" {
        "'projects/one', 'projects/two'"
    } else {
        "'supplies/two', 'supplies/one'"
    };
    fs::write(root.join("eska.toml"), format!("# Exact workspace\r\n[workspace]\r\nmembers = [{members}]\r\n[build]\r\nplatform_version = '8.3.27.2325'\r\nartifacts_directory = 'dist'\r\n[vcs.workflow]\r\npreset = 'trunk'\r\n")).unwrap();
    fs::write(root.join(".gitattributes"), "*.yaml -text\r\n").unwrap();
    let pins = WorkspaceAcceptedRefs::from_json(
        &serde_json::json!({"schema_version":1,"members":pins}).to_string(),
    )
    .unwrap();
    (workspace(&root), pins)
}

/// Ordinary discovery remains the single authority for member names and resolved settings.
fn workspace(root: &Path) -> Workspace {
    let DiscoveryContext::Workspace { workspace, .. } = discover_context(root).unwrap() else {
        panic!("workspace fixture")
    };
    workspace
}

/// Inherited settings, explicit overrides, local bytes and independently selected pins all survive.
#[test]
fn complete_update_maps_supplies_by_name_and_keeps_exact_settings() {
    let test = TestDir::new();
    let (old, previous) = fixture(&test, "old", false);
    let (incoming, new) = fixture(&test, "incoming", false);
    let local = b"\xef\xbb\xbf// Local member edit\r\n";
    fs::write(
        old.members()[0]
            .project()
            .source()
            .join("CommonModules/Tools/Module.bsl"),
        local,
    )
    .unwrap();
    let before = Inventory::read(old.root()).unwrap();
    let plan =
        prepare_workspace_base_update(&old, &previous, &incoming, &new, &test.0.join("updated"))
            .unwrap();
    assert!(plan.can_write());
    assert_eq!(plan.members()[0].name().as_str(), "one");
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
    let updated = workspace(plan.destination());
    for (old, current) in old.members().iter().zip(updated.members()) {
        assert_eq!(
            old.project().configuration(),
            current.project().configuration()
        );
        assert_eq!(
            fs::read(old.root().join("eska.toml")).unwrap(),
            fs::read(current.root().join("eska.toml")).unwrap()
        );
        let source = EskaSource::open(
            current.project().clone(),
            ProjectScope::Member(current.name().clone()),
        )
        .unwrap();
        source
            .prepare(
                new.reference(current.name()).unwrap(),
                &test.0.join(format!("generation-{}", current.name())),
                true,
            )
            .unwrap();
    }
    assert_eq!(
        fs::read(
            updated.members()[0]
                .project()
                .source()
                .join("CommonModules/Tools/Module.bsl")
        )
        .unwrap(),
        local
    );
    assert_eq!(Inventory::read(old.root()).unwrap(), before);
    assert_eq!(
        fs::read(old.root().join("eska.toml")).unwrap(),
        fs::read(updated.root().join("eska.toml")).unwrap()
    );
}

/// A later member's conflict and an incomplete pin set both prevent the first member's publication.
#[test]
fn any_conflict_or_incomplete_approval_refuses_the_entire_copy() {
    let test = TestDir::new();
    let (old, previous) = fixture(&test, "old", false);
    let (incoming, new) = fixture(&test, "incoming", true);
    fs::write(
        old.members()[1]
            .project()
            .source()
            .join("CommonModules/Tools/Module.bsl"),
        "// Keep denied data\r\n",
    )
    .unwrap();
    let destination = test.0.join("updated");
    let plan =
        prepare_workspace_base_update(&old, &previous, &incoming, &new, &destination).unwrap();
    assert!(!plan.can_write());
    assert_eq!(plan.publish().unwrap_err().code, "source_update_conflicts");
    assert!(!destination.exists());
    let incomplete = WorkspaceAcceptedRefs::from_json(&serde_json::json!({"schema_version":1,"members":{"one":new.reference(old.members()[0].name()).unwrap().sha256}}).to_string()).unwrap();
    assert_eq!(
        prepare_workspace_base_update(&old, &previous, &incoming, &incomplete, &destination)
            .err()
            .unwrap()
            .code,
        "source_workspace_pins_mismatch"
    );
    assert!(!destination.exists());
}

/// The outer marker blocks every completed member until all outputs and root settings are verified.
#[test]
fn interrupted_workspace_remains_blocked_and_only_exact_output_can_resume() {
    for foreign in [false, true] {
        let test = TestDir::new();
        let (old, previous) = fixture(&test, "old", false);
        let (incoming, new) = fixture(&test, "incoming", false);
        let plan = prepare_workspace_base_update(
            &old,
            &previous,
            &incoming,
            &new,
            &test.0.join("updated"),
        )
        .unwrap();
        fs::create_dir(plan.destination()).unwrap();
        fs::write(
            plan.destination().join(WORKSPACE_BASE_UPDATE_RECEIPT),
            serde_json::to_vec(&plan.receipt).unwrap(),
        )
        .unwrap();
        fs::create_dir_all(plan.destination().join("projects")).unwrap();
        plan.members[0].plan.publish().unwrap();
        assert_eq!(
            require_project_ready(plan.members[0].destination())
                .unwrap_err()
                .code,
            "source_update_incomplete"
        );
        if foreign {
            let root = plan.members[1].destination();
            fs::create_dir(root).unwrap();
            fs::write(root.join("notes.md"), "Foreign bytes").unwrap();
            let before = Inventory::read(plan.destination()).unwrap();
            assert!(plan.publish().is_err());
            assert_eq!(Inventory::read(plan.destination()).unwrap(), before);
        } else {
            assert_eq!(plan.publish().unwrap(), LayoutPublication::Resumed);
            require_project_ready(plan.members[0].destination()).unwrap();
        }
    }
}

/// Incoming aliases are explicit selection, while even non-semantic manifest edits stale a preview.
#[test]
fn changed_workspace_settings_and_member_names_refuse_before_writing() {
    let test = TestDir::new();
    let (old, previous) = fixture(&test, "old", false);
    let (incoming, new) = fixture(&test, "incoming", false);
    let destination = test.0.join("updated");
    let plan =
        prepare_workspace_base_update(&old, &previous, &incoming, &new, &destination).unwrap();
    let root_manifest = incoming.root().join("eska.toml");
    let original = fs::read(&root_manifest).unwrap();
    let mut changed = original.clone();
    changed.extend_from_slice(b"# Changed during preview\r\n");
    fs::write(&root_manifest, changed).unwrap();
    assert_eq!(plan.publish().unwrap_err().code, "source_input_changed");
    assert!(!destination.exists());
    fs::write(root_manifest, original).unwrap();
    let name = ProjectName::parse("one".into()).unwrap();
    let manifest = incoming
        .members()
        .iter()
        .find(|member| member.name() == &name)
        .unwrap()
        .root()
        .join("eska.toml");
    let config = ProjectConfig::load(&manifest)
        .unwrap()
        .with_name(ProjectName::parse("other".into()).unwrap());
    fs::write(manifest, config.to_workspace_member_toml().unwrap()).unwrap();
    let second = ProjectName::parse("two".into()).unwrap();
    let renamed = WorkspaceAcceptedRefs::from_json(
        &serde_json::json!({"schema_version":1,"members":{
            "other":new.reference(&name).unwrap().sha256,
            "two":new.reference(&second).unwrap().sha256,
        }})
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        prepare_workspace_base_update(
            &old,
            &previous,
            &workspace(incoming.root()),
            &renamed,
            &destination
        )
        .err()
        .unwrap()
        .code,
        "source_update_members_mismatch"
    );
    assert!(!destination.exists());
}
