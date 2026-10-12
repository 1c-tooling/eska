use super::super::super::{files::Inventory, layout::migration::publication as storage};
use super::*;
use crate::{
    project_core::{
        config::ProjectConfig,
        creation,
        eska_source::EskaSource,
        metadata_model::ProjectScope,
        platform_settings::PlatformVersion,
        source_format::{
            AcceptedBase, LayoutPublication, NativeUuid, Operation, PermissionReason, PropertyName,
            PropertyValue, export_designer, prepare_property_edit,
        },
    },
    test_support::TestDir,
};
use std::fs;

/// Copy only public fixture bytes into this test's own playground workspace.
fn fixture(test: &TestDir) -> (Workspace, WorkspaceAcceptedRefs) {
    let root = test.0.join("old");
    fs::create_dir_all(root.join("projects")).unwrap();
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    let inventory = Inventory::read(&input).unwrap();
    let mut pins = BTreeMap::new();
    for (name, readable) in [("one", false), ("two", true)] {
        let native = test.0.join(format!("native-{name}"));
        fs::create_dir(&native).unwrap();
        for path in &inventory.directories {
            fs::create_dir_all(native.join(path.as_path())).unwrap();
        }
        for path in inventory.files.keys() {
            fs::copy(input.join(path.as_path()), native.join(path.as_path())).unwrap();
        }
        fs::remove_file(native.join("Ext/ParentConfigurations.bin")).unwrap();
        let platform = readable.then(|| PlatformVersion::parse("8.3.26.1000").unwrap());
        let frozen = if readable {
            Vec::new()
        } else {
            vec![NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap()]
        };
        let import = if readable {
            creation::import_readable_project
        } else {
            creation::import_project
        };
        let imported = import(
            &native,
            &root.join("projects").join(name),
            ProjectName::parse(name.into()).unwrap(),
            platform.as_ref(),
            &frozen,
        )
        .unwrap();
        pins.insert(name.to_owned(), imported.accepted.sha256);
        let path = imported.project.root().join("eska.toml");
        let config = ProjectConfig::load(&path).unwrap();
        fs::write(
            path,
            format!(
                "# Member {name}\r\n{}",
                config
                    .to_workspace_member_toml()
                    .unwrap()
                    .replace("\n[build]\nplatform_version = \"\"\n", "\n")
            ),
        )
        .unwrap();
    }
    fs::write(root.join("eska.toml"),
        "# Retain root comments\r\n[workspace]\r\nmembers = ['projects/one', 'projects/two']\r\n[build]\r\nplatform_version = '8.3.27.2325'\r\nartifacts_directory = 'dist'\r\n[vcs.workflow]\r\npreset = 'trunk'\r\n"
    ).unwrap();
    fs::write(
        root.join(".gitattributes"),
        "# Scoped bytes\r\n*.yaml -text\r\n",
    )
    .unwrap();
    let expected = WorkspaceAcceptedRefs::from_json(
        &serde_json::json!({"schema_version":1,"members":pins}).to_string(),
    )
    .unwrap();
    (workspace(&root), expected)
}

/// Ordinary discovery is the authority for all resolved workspace settings.
fn workspace(root: &Path) -> Workspace {
    let DiscoveryContext::Workspace { workspace, .. } = discover_context(root).unwrap() else {
        panic!("workspace fixture");
    };
    workspace
}

/// Interruption carries exact receipts; this helper never authorizes an overwrite.
fn interrupt(plan: &WorkspaceLayoutPlan) {
    fs::create_dir(plan.destination()).unwrap();
    super::super::publish_new(
        plan.destination(),
        WORKSPACE_LAYOUT_RECEIPT,
        &serde_json::to_string(&plan.receipt).unwrap(),
    )
    .unwrap();
}

/// Different layouts, pins, freeze and explicit/inherited settings survive one complete copy.
#[test]
fn workspace_copy_retains_inheritance_and_independent_member_authority() {
    let test = TestDir::new();
    let (old, pins) = fixture(&test);
    assert_eq!(
        old.members()[0]
            .project()
            .configuration()
            .build_settings()
            .platform_version()
            .unwrap()
            .as_str(),
        "8.3.27.2325"
    );
    assert_eq!(
        old.members()[1]
            .project()
            .configuration()
            .build_settings()
            .platform_version()
            .unwrap()
            .as_str(),
        "8.3.26.1000"
    );
    let project = old.members()[0].project();
    let base = AcceptedBase::open(
        &project.root().join(".eska/accepted"),
        pins.reference(old.members()[0].name()).unwrap(),
    )
    .unwrap();
    let uuid = NativeUuid::parse("10000000-0000-4000-8000-000000000001").unwrap();
    prepare_property_edit(
        project.source(),
        &base,
        &uuid,
        PropertyName::Comment,
        PropertyValue::Text("Working workspace edit".into()),
    )
    .unwrap()
    .publish(&base)
    .unwrap();
    fs::write(old.root().join("notes.md"), "Preserve user notes").unwrap();
    fs::create_dir(old.root().join(".git")).unwrap();
    fs::write(old.root().join(".git/marker"), "Git state").unwrap();
    let before = Inventory::read(old.root()).unwrap();
    let plan = prepare_readable_workspace(&old, &pins, &test.0.join("new")).unwrap();
    assert!(!plan.destination().exists());
    assert!(plan.layout_changed());
    assert!(plan.members()[0].layout_changed());
    assert!(!plan.members()[1].layout_changed());
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
    assert_eq!(Inventory::read(old.root()).unwrap(), before);
    let new = workspace(plan.destination());
    assert_eq!(old.build_settings(), new.build_settings());
    assert_eq!(old.workflow_settings(), new.workflow_settings());
    assert_eq!(
        ancillary(old.root()).unwrap(),
        ancillary(new.root()).unwrap()
    );
    assert!(!new.root().join("notes.md").exists());
    assert!(!new.root().join(".git").exists());
    for ((original, copied), planned) in old.members().iter().zip(new.members()).zip(plan.members())
    {
        assert_member_copy(&test, original, copied, planned);
    }
}

/// Verify native parity and independent authority using the ordinary member source/build boundary.
fn assert_member_copy(
    test: &TestDir,
    original: &crate::project_core::WorkspaceMember,
    copied: &crate::project_core::WorkspaceMember,
    planned: &WorkspaceLayoutMember,
) {
    assert_eq!(original.name(), copied.name());
    assert_eq!(
        original.project().configuration(),
        copied.project().configuration()
    );
    assert_eq!(
        ancillary(original.root()).unwrap(),
        ancillary(copied.root()).unwrap()
    );
    let authority = AcceptedBase::open(
        &copied.root().join(".eska/accepted"),
        planned.accepted_reference(),
    )
    .unwrap();
    let output = EskaSource::open(
        copied.project().clone(),
        ProjectScope::Member(copied.name().clone()),
    )
    .unwrap()
    .prepare(
        planned.accepted_reference(),
        &test.0.join(format!("generation-{}", copied.name())),
        true,
    )
    .unwrap();
    let control = test.0.join(format!("control-{}", copied.name()));
    export_designer(original.project().source(), &control).unwrap();
    assert_eq!(
        Inventory::read(output.xml()).unwrap(),
        Inventory::read(&control).unwrap()
    );
    let restored = test.0.join(format!("restored-{}", copied.name()));
    authority.restore(&restored).unwrap();
    assert!(
        !fs::read_to_string(restored.join("configuration.yaml"))
            .unwrap()
            .contains("Working workspace edit")
    );
    if copied.name().as_str() == "one" {
        let frozen = NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(
            authority
                .permission(&frozen, Operation::Property(PropertyName::Comment))
                .reason,
            PermissionReason::EskaFrozen
        );
    }
}

/// Version, unique keys and exact member selection cannot be inferred from editable settings.
#[test]
fn workspace_pin_document_refuses_ambiguous_and_incomplete_approval() {
    let test = TestDir::new();
    let (old, pins) = fixture(&test);
    let pin = pins
        .reference(old.members()[0].name())
        .unwrap()
        .sha256
        .clone();
    for input in [
        "{}".into(),
        format!("{{\"schema_version\":2,\"members\":{{\"one\":\"{pin}\"}}}}"),
        format!("{{\"schema_version\":1,\"members\":{{\"one\":\"{pin}\",\"one\":\"{pin}\"}}}}"),
        format!("{{\"schema_version\":1,\"members\":{{\"One\":\"{pin}\"}}}}"),
        "{\"schema_version\":1,\"members\":{\"one\":\"short\"}}".into(),
        "{\"schema_version\":1,\"members\":{},\"other\":true}".into(),
    ] {
        assert_eq!(
            WorkspaceAcceptedRefs::from_json(&input).unwrap_err().code,
            "source_workspace_pins_invalid"
        );
    }
    for names in [
        vec![],
        vec!["one"],
        vec!["one", "two", "extra"],
        vec!["one", "other"],
    ] {
        let members = names
            .into_iter()
            .map(|name| (name, pin.clone()))
            .collect::<BTreeMap<_, _>>();
        let pins = WorkspaceAcceptedRefs::from_json(
            &serde_json::json!({"schema_version":1,"members":members}).to_string(),
        )
        .unwrap();
        assert_eq!(
            prepare_readable_workspace(&old, &pins, &test.0.join("refused"))
                .unwrap_err()
                .code,
            "source_workspace_pins_mismatch"
        );
    }
    let first = old.members()[0].name();
    let second = old.members()[1].name();
    let swapped = WorkspaceAcceptedRefs::from_json(
        &serde_json::json!({
            "schema_version":1,
            "members":{
                first.as_str():pins.reference(second).unwrap().sha256,
                second.as_str():pins.reference(first).unwrap().sha256
            }
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        prepare_readable_workspace(&old, &swapped, &test.0.join("refused"))
            .unwrap_err()
            .code,
        "source_baseline_untrusted"
    );
    assert!(!test.0.join("refused").exists());
}

/// A new copy of a fully readable workspace is byte-exact and retains every independently selected pin.
#[test]
fn readable_workspace_copy_retains_bytes_and_pins() {
    let test = TestDir::new();
    let (old, pins) = fixture(&test);
    let plan = prepare_readable_workspace(&old, &pins, &test.0.join("new")).unwrap();
    plan.publish().unwrap();
    let copied = workspace(plan.destination());
    let members = plan
        .members()
        .iter()
        .map(|member| {
            (
                member.name().as_str(),
                member.accepted_reference().sha256.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let pins = WorkspaceAcceptedRefs::from_json(
        &serde_json::json!({"schema_version":1,"members":members}).to_string(),
    )
    .unwrap();
    let again = prepare_readable_workspace(&copied, &pins, &test.0.join("again")).unwrap();
    assert!(!again.layout_changed());
    again.publish().unwrap();
    assert_eq!(
        Inventory::read(plan.destination()).unwrap(),
        Inventory::read(again.destination()).unwrap()
    );
}

/// Partial members are blocked even before the root manifest exists, then resume as one operation.
#[test]
fn interrupted_workspace_blocks_nested_consumers_and_resumes_all_members() {
    let test = TestDir::new();
    let (old, pins) = fixture(&test);
    let plan = prepare_readable_workspace(&old, &pins, &test.0.join("new")).unwrap();
    interrupt(&plan);
    fs::create_dir(plan.destination().join("projects")).unwrap();
    let first = &plan.members[0].plan;
    first.publish().unwrap();
    let detached = crate::project_core::discovery::discover(first.destination()).unwrap();
    assert_eq!(
        EskaSource::open(detached, ProjectScope::Standalone)
            .err()
            .unwrap()
            .code,
        "source_migration_incomplete"
    );
    let second = &plan.members[1].plan;
    fs::create_dir(second.destination()).unwrap();
    super::super::publish_new(
        second.destination(),
        super::super::PROJECT_LAYOUT_RECEIPT,
        &serde_json::to_string(&second.receipt).unwrap(),
    )
    .unwrap();
    fs::create_dir(second.destination().join(".eska")).unwrap();
    second.source.publish(&second.source_destination()).unwrap();
    storage::interrupt(second.base.publication_view(), &second.base_destination());
    let resumed = prepare_readable_workspace(&old, &pins, plan.destination()).unwrap();
    assert_eq!(resumed.publish().unwrap(), LayoutPublication::Resumed);
    let new = workspace(plan.destination());
    for member in new.members() {
        EskaSource::open(
            member.project().clone(),
            ProjectScope::Member(member.name().clone()),
        )
        .unwrap();
    }
    assert_eq!(resumed.publish().unwrap(), LayoutPublication::Unchanged);
    super::super::publish_new(
        plan.destination(),
        WORKSPACE_LAYOUT_RECEIPT,
        &serde_json::to_string(&plan.receipt).unwrap(),
    )
    .unwrap();
    assert_eq!(
        EskaSource::open(
            new.members()[0].project().clone(),
            ProjectScope::Member(new.members()[0].name().clone())
        )
        .err()
        .unwrap()
        .code,
        "source_migration_incomplete"
    );
    assert_eq!(resumed.publish().unwrap(), LayoutPublication::Resumed);
}

/// A foreign last member is found before publishing any earlier member, even during recovery.
#[test]
fn foreign_workspace_output_never_publishes_an_earlier_member() {
    let test = TestDir::new();
    let (old, pins) = fixture(&test);
    let plan = prepare_readable_workspace(&old, &pins, &test.0.join("new")).unwrap();
    interrupt(&plan);
    fs::create_dir_all(plan.members[1].plan.destination()).unwrap();
    fs::write(
        plan.members[1].plan.destination().join("notes.md"),
        "Foreign member",
    )
    .unwrap();
    let before = Inventory::read(plan.destination()).unwrap();
    assert!(plan.publish().is_err());
    assert_eq!(Inventory::read(plan.destination()).unwrap(), before);
    assert!(!plan.members[0].plan.destination().exists());
    fs::remove_file(plan.members[1].plan.destination().join("notes.md")).unwrap();
    fs::remove_dir(plan.members[1].plan.destination()).unwrap();
    plan.publish().unwrap();
    fs::write(
        plan.members[1]
            .plan
            .source_destination()
            .join("configuration.yaml"),
        "Edited output",
    )
    .unwrap();
    assert!(plan.publish().is_err());
}

/// Neither a tampered aggregate receipt nor a foreign ancestor authorizes publishing missing members.
#[test]
fn invalid_workspace_receipts_and_foreign_ancestors_refuse_recovery() {
    for fault in ["invalid", "pin", "ancestor"] {
        let test = TestDir::new();
        let (old, pins) = fixture(&test);
        let plan = prepare_readable_workspace(&old, &pins, &test.0.join("new")).unwrap();
        interrupt(&plan);
        let receipt = plan.destination().join(WORKSPACE_LAYOUT_RECEIPT);
        match fault {
            "invalid" => fs::write(receipt, "invalid").unwrap(),
            "pin" => {
                let mut stored = serde_json::to_value(&plan.receipt).unwrap();
                stored["members"]["one"]["project"]["accepted_pin"] = "0".repeat(64).into();
                fs::write(receipt, stored.to_string()).unwrap();
            }
            _ => {
                fs::create_dir(plan.destination().join("projects")).unwrap();
                fs::write(
                    plan.destination().join("projects/notes.md"),
                    "Foreign ancestor file",
                )
                .unwrap();
            }
        }
        let before = Inventory::read(plan.destination()).unwrap();
        assert!(plan.publish().is_err());
        assert_eq!(Inventory::read(plan.destination()).unwrap(), before);
        assert!(!plan.destination().join("eska.toml").exists());
        assert!(!plan.members[0].plan.destination().exists());
    }
}

/// Root settings, member settings, authored bytes and accepted bytes are all stale-plan boundaries.
#[test]
fn stale_workspace_inputs_refuse_publication_without_creating_output() {
    for fault in ["root", "absent", "member", "source", "base"] {
        let test = TestDir::new();
        let (old, pins) = fixture(&test);
        let plan = prepare_readable_workspace(&old, &pins, &test.0.join("new")).unwrap();
        let path = match fault {
            "root" => old.root().join("eska.toml"),
            "absent" => old.root().join(".gitignore"),
            "member" => old.members()[1].root().join("eska.toml"),
            "source" => old.members()[1]
                .project()
                .source()
                .join("configuration.yaml"),
            _ => old.members()[1].root().join(".eska/accepted/accepted.json"),
        };
        let content = fs::read_to_string(&path).unwrap_or_default();
        fs::write(path, format!("{content}\n# Stale\n")).unwrap();
        assert_eq!(
            plan.publish().unwrap_err().code,
            "source_input_changed",
            "{fault}"
        );
        assert!(!plan.destination().exists());
    }
}

/// Unsupported members and overlap refuse the complete plan, preserving already valid members.
#[test]
fn unsupported_workspace_member_and_overlap_refuse_without_output() {
    let test = TestDir::new();
    let (old, pins) = fixture(&test);
    for destination in [old.root().join("new"), test.0.clone()] {
        assert_eq!(
            prepare_readable_workspace(&old, &pins, &destination)
                .unwrap_err()
                .code,
            "source_tree_overlap"
        );
    }
    let path = old.members()[1].root().join("eska.toml");
    let config = ProjectConfig::load(&path)
        .unwrap()
        .with_source_format(crate::project_core::SourceFormat::DesignerXml)
        .unwrap();
    fs::write(path, config.to_workspace_member_toml().unwrap()).unwrap();
    assert_eq!(
        prepare_readable_workspace(&workspace(old.root()), &pins, &test.0.join("new"))
            .unwrap_err()
            .code,
        "source_format_mismatch"
    );
    assert!(!test.0.join("new").exists());
}

/// Shared ancestors with different case spellings cannot silently alias after transfer.
#[test]
fn workspace_member_case_collisions_refuse_before_publication() {
    let test = TestDir::new();
    let (old, pins) = fixture(&test);
    fs::create_dir(old.root().join("Projects")).unwrap();
    fs::rename(old.members()[1].root(), old.root().join("Projects/two")).unwrap();
    let path = old.root().join("eska.toml");
    let content = fs::read_to_string(&path)
        .unwrap()
        .replace("projects/two", "Projects/two");
    fs::write(path, content).unwrap();
    assert_eq!(
        prepare_readable_workspace(&workspace(old.root()), &pins, &test.0.join("new"))
            .unwrap_err()
            .code,
        "source_path_collision"
    );
    assert!(!test.0.join("new").exists());
}

#[cfg(unix)]
/// Input and output directory aliases refuse without following a write into their targets.
#[test]
fn workspace_aliases_refuse_and_preserve_their_targets() {
    let test = TestDir::new();
    let (old, pins) = fixture(&test);
    let outside = test.0.join("outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, test.0.join("new")).unwrap();
    assert!(prepare_readable_workspace(&old, &pins, &test.0.join("new")).is_err());
    assert!(fs::read_dir(&outside).unwrap().next().is_none());
    std::os::unix::fs::symlink(old.members()[0].root(), old.root().join("alias")).unwrap();
    let path = old.root().join("eska.toml");
    let content = fs::read_to_string(&path)
        .unwrap()
        .replace("projects/one", "alias");
    fs::write(path, content).unwrap();
    assert_eq!(
        prepare_readable_workspace(&workspace(old.root()), &pins, &test.0.join("refused"))
            .unwrap_err()
            .code,
        "source_path_invalid"
    );
    assert!(!test.0.join("refused").exists());
}
