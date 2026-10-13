//! Explicit migrations keep current values, old evidence and exact publication receipts independent.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::*;
use crate::{
    project_core::{
        ProjectName, Workspace, creation,
        discovery::{DiscoveryContext, discover, discover_context},
        metadata_model::ProjectScope,
        source_format::{
            AcceptedBase, LayoutPublication, NativeUuid, Operation, SourceReader,
            WorkspaceAcceptedRefs, accept_source, export_designer, import_designer,
            import_designer_forms, import_designer_readable, import_designer_resources,
            import_designer_separated, prepare_accepted_form_layout, prepare_descriptor_layout,
            prepare_form_layout, prepare_form_project, prepare_form_workspace, prepare_protected,
            prepare_readable_layout, prepare_resource_layout,
        },
    },
    test_support::TestDir,
};

const OWNER: &str = "Catalogs/Sample/Forms/Main/form.yaml";
const DIALOG: &str = "Catalogs/Sample/Forms/Main/Ext/Form.xml";
const XML: &str = "\u{feff}<Form xmlns=\"http://v8.1c.ru/8.3/xcf/logform\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" version=\"2.20\">\r\n<AutoCommandBar name=\"\" id=\"-1\"/><Title><v8:item><v8:lang>ru</v8:lang><v8:content>Принятая форма</v8:content></v8:item></Title><ChildItems><UsualGroup name=\"First\" id=\"1\"><ChildItems/></UsualGroup><UsualGroup name=\"Second\" id=\"2\"><ChildItems/></UsualGroup></ChildItems><ReadOnly>true</ReadOnly>\r\n</Form>\r\n";

/// Native metadata identity remains the authority through every layout version.
fn uuid() -> NativeUuid {
    NativeUuid::parse("70000000-0000-4000-8000-000000000001").unwrap()
}

/// Copy only public inputs and introduce a supported managed dialog before either source or base is accepted.
fn native(test: &TestDir) -> PathBuf {
    let root = test.0.join("native");
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    let inventory = files::Inventory::read(&input).unwrap();
    fs::create_dir(&root).unwrap();
    for path in inventory.directories {
        fs::create_dir_all(root.join(path.as_path())).unwrap();
    }
    for path in inventory.files.keys() {
        fs::copy(input.join(path.as_path()), root.join(path.as_path())).unwrap();
    }
    fs::remove_file(root.join("Ext/ParentConfigurations.bin")).unwrap();
    fs::write(root.join(DIALOG), XML).unwrap();
    let module = root.join("Catalogs/Sample/Forms/Main/Ext/Form/Module.bsl");
    fs::create_dir_all(module.parent().unwrap()).unwrap();
    fs::copy(root.join("CommonModules/Tools/Ext/Module.bsl"), module).unwrap();
    root
}

/// Source and accepted trees begin with the same original evidence; current edits are made only afterwards.
fn legacy(test: &TestDir, version: u32, frozen: &[NativeUuid]) -> (PathBuf, AcceptedBase) {
    let input = native(test);
    let source = test.0.join("source");
    let import = match version {
        1 => import_designer,
        2 => import_designer_separated,
        3 => import_designer_resources,
        4 => import_designer_readable,
        _ => import_designer_forms,
    };
    import(&input, &source).unwrap();
    let base = test.0.join("accepted");
    let pin = accept_source(&source, &base, frozen).unwrap();
    (source, AcceptedBase::open(&base, &pin).unwrap())
}

/// Address native identity through the actual input map rather than assuming a payload's physical path.
fn dialog(source: &Path) -> PathBuf {
    let reader = SourceReader::open(source).unwrap();
    let file = reader
        .manifest()
        .files
        .iter()
        .find(|file| file.path.as_str() == DIALOG)
        .unwrap();
    source.join(file.source_path().as_path())
}

/// All older layouts retain current dialog edits and separately upgrade frozen accepted authority.
#[test]
fn source_and_accepted_form_migrations_preserve_independent_values_from_every_old_layout() {
    for version in 1..=4 {
        let test = TestDir::new();
        let (source, base) = legacy(&test, version, &[uuid()]);
        fs::write(
            dialog(&source),
            XML.replace("Принятая форма", "Рабочая форма"),
        )
        .unwrap();
        let before = files::Inventory::read(&source).unwrap();
        let source_plan = prepare_form_layout(&source).unwrap();
        let accepted_plan = prepare_accepted_form_layout(&base).unwrap();
        assert_eq!(source_plan.manifest().schema_version, 4);
        assert_eq!(
            source_plan.manifest().input_revision,
            accepted_plan.source_manifest().input_revision
        );
        assert_eq!(accepted_plan.previous_reference(), base.reference());
        assert_ne!(accepted_plan.accepted_reference(), base.reference());
        let target = test.0.join("new-source");
        let accepted = test.0.join("new-base");
        assert_eq!(
            source_plan.publish(&target).unwrap(),
            LayoutPublication::Created
        );
        assert_eq!(
            source_plan.publish(&target).unwrap(),
            LayoutPublication::Unchanged
        );
        assert_eq!(
            accepted_plan.publish(&accepted).unwrap(),
            LayoutPublication::Created
        );
        assert_eq!(
            accepted_plan.publish(&accepted).unwrap(),
            LayoutPublication::Unchanged
        );
        assert_eq!(files::Inventory::read(&source).unwrap(), before);
        let migrated = AcceptedBase::open(&accepted, accepted_plan.accepted_reference()).unwrap();
        let mut expected =
            serde_json::to_value(base.permission(&uuid(), Operation::Resource)).unwrap();
        expected["baseline"] = serde_json::Value::String(migrated.reference().sha256.clone());
        assert_eq!(
            serde_json::to_value(migrated.permission(&uuid(), Operation::Resource)).unwrap(),
            expected
        );
        let restored = test.0.join("restored");
        migrated.restore(&restored).unwrap();
        assert!(
            fs::read_to_string(restored.join(OWNER))
                .unwrap()
                .contains("Принятая форма")
        );
        assert!(
            !fs::read_to_string(restored.join(OWNER))
                .unwrap()
                .contains("Рабочая форма")
        );
        let working = SourceReader::open(&target)
            .unwrap()
            .read_owner(&SourcePath::parse(OWNER).unwrap())
            .unwrap();
        assert!(
            working
                .source
                .form
                .unwrap()
                .to_xml()
                .unwrap()
                .contains("Рабочая форма")
        );
        assert!(!target.join("Catalogs/Sample/Forms/Main/Form.xml").exists());
        let old_xml = test.0.join("old-xml");
        let new_xml = test.0.join("new-xml");
        export_designer(&source, &old_xml).unwrap();
        export_designer(&target, &new_xml).unwrap();
        assert_eq!(
            files::Inventory::read(&old_xml).unwrap(),
            files::Inventory::read(&new_xml).unwrap()
        );
        let selected =
            prepare_protected(&target, &migrated, &test.0.join("generations"), false).unwrap();
        assert_eq!(
            fs::read_to_string(selected.xml().join(DIALOG)).unwrap(),
            XML
        );
    }
}

/// Existing map 4 is a byte/pin no-op for both new explicit entry points and older nondowngrading targets.
#[test]
fn every_migration_preserves_existing_unified_forms_without_downgrading() {
    let test = TestDir::new();
    let (source, base) = legacy(&test, 5, &[]);
    let before = files::Inventory::read(&source).unwrap();
    for prepare in [
        prepare_form_layout,
        prepare_descriptor_layout,
        prepare_resource_layout,
        prepare_readable_layout,
    ] {
        let plan = prepare(&source).unwrap();
        assert!(plan.changes().is_empty());
        assert_eq!(plan.manifest().schema_version, 4);
    }
    let plan = prepare_accepted_form_layout(&base).unwrap();
    assert!(plan.is_empty());
    assert_eq!(plan.accepted_reference(), base.reference());
    assert_eq!(files::Inventory::read(&source).unwrap(), before);
}

/// Exact receipts resume source/base publication and still reject foreign edits or changed source inputs.
#[test]
fn form_layout_publication_resumes_and_refuses_conflicting_output_or_input() {
    let test = TestDir::new();
    let (source, base) = legacy(&test, 3, &[]);
    let plan = prepare_form_layout(&source).unwrap();
    let accepted = prepare_accepted_form_layout(&base).unwrap();
    let target = test.0.join("new-source");
    let new_base = test.0.join("new-base");
    super::super::publication::interrupt(plan.publication_view(), &target);
    super::super::publication::interrupt(accepted.publication_view(), &new_base);
    assert_eq!(plan.publish(&target).unwrap(), LayoutPublication::Resumed);
    assert_eq!(
        accepted.publish(&new_base).unwrap(),
        LayoutPublication::Resumed
    );
    fs::write(target.join(OWNER), "foreign bytes").unwrap();
    assert!(plan.publish(&target).is_err());
    fs::write(
        dialog(&source),
        XML.replace("Принятая форма", "Concurrent edit"),
    )
    .unwrap();
    let absent = test.0.join("absent");
    assert_eq!(
        plan.publish(&absent).unwrap_err().code,
        "source_input_changed"
    );
    assert!(!absent.exists());
}

/// Unverified dialogs and mixed authored YAML refuse complete plans before output or new trust is created.
#[test]
fn unsupported_form_profiles_refuse_source_and_base_migration() {
    let test = TestDir::new();
    let (source, base) = legacy(&test, 4, &[]);
    fs::write(dialog(&source), XML.replace("2.20", "2.19")).unwrap();
    let before = files::Inventory::read(&source).unwrap();
    assert_eq!(
        prepare_form_layout(&source).unwrap_err().code,
        "source_form_profile_unverified"
    );
    assert_eq!(files::Inventory::read(&source).unwrap(), before);
    assert!(prepare_accepted_form_layout(&base).is_ok());
}

/// Standalone settings and current body values are copied while the independently accepted form stays original.
#[test]
fn standalone_form_project_migration_retains_settings_and_independent_baseline() {
    let test = TestDir::new();
    let input = native(&test);
    let imported = creation::import_project(
        &input,
        &test.0.join("old-project"),
        ProjectName::parse("demo".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let project = &imported.project;
    fs::write(
        dialog(project.source()),
        XML.replace("Принятая форма", "Рабочая форма"),
    )
    .unwrap();
    let config = project.root().join("eska.toml");
    let settings = format!(
        "# keep settings\r\n{}",
        fs::read_to_string(&config).unwrap()
    );
    fs::write(&config, &settings).unwrap();
    fs::write(project.root().join("user-notes.md"), "unrelated bytes").unwrap();
    let before = files::Inventory::read(project.root()).unwrap();
    let plan = prepare_form_project(
        project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &test.0.join("new-project"),
    )
    .unwrap();
    assert!(!plan.destination().exists());
    assert!(plan.layout_changed());
    assert_eq!(plan.previous_reference(), &imported.accepted);
    assert_ne!(plan.accepted_reference(), &imported.accepted);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
    assert_eq!(files::Inventory::read(project.root()).unwrap(), before);
    let copied = discover(plan.destination()).unwrap();
    assert_eq!(copied.configuration(), project.configuration());
    assert_eq!(
        fs::read_to_string(copied.root().join("eska.toml")).unwrap(),
        settings
    );
    assert!(!copied.root().join("user-notes.md").exists());
    let base = AcceptedBase::open(
        &copied.root().join(".eska/accepted"),
        plan.accepted_reference(),
    )
    .unwrap();
    let restored = test.0.join("restored");
    base.restore(&restored).unwrap();
    assert!(
        !fs::read_to_string(restored.join(OWNER))
            .unwrap()
            .contains("Рабочая форма")
    );
    let selected =
        prepare_protected(copied.source(), &base, &test.0.join("generations"), true).unwrap();
    assert!(
        fs::read_to_string(selected.xml().join(DIALOG))
            .unwrap()
            .contains("Рабочая форма")
    );
}

/// Different member layouts and inherited settings are captured before whole-workspace publication.
fn workspace_fixture(test: &TestDir) -> (Workspace, WorkspaceAcceptedRefs) {
    let input = native(test);
    let root = test.0.join("old-workspace");
    fs::create_dir_all(root.join("projects")).unwrap();
    let mut pins = BTreeMap::new();
    for (name, readable) in [("one", false), ("two", true)] {
        let import = if readable {
            creation::import_readable_project
        } else {
            creation::import_project
        };
        let imported = import(
            &input,
            &root.join("projects").join(name),
            ProjectName::parse(name.into()).unwrap(),
            None,
            &[],
        )
        .unwrap();
        pins.insert(name.to_owned(), imported.accepted.sha256);
        let path = imported.project.root().join("eska.toml");
        let config = crate::project_core::config::ProjectConfig::load(&path).unwrap();
        fs::write(
            path,
            config
                .to_workspace_member_toml()
                .unwrap()
                .replace("\n[build]\nplatform_version = \"\"\n", "\n"),
        )
        .unwrap();
        fs::write(
            dialog(imported.project.source()),
            XML.replace("Принятая форма", &format!("Working {name}")),
        )
        .unwrap();
    }
    let settings = "# workspace settings\r\n[workspace]\r\nmembers = ['projects/one','projects/two']\r\n[build]\r\nplatform_version = '8.3.27.2325'\r\nartifacts_directory = 'dist'\r\n";
    fs::write(root.join("eska.toml"), settings).unwrap();
    let DiscoveryContext::Workspace { workspace, .. } = discover_context(&root).unwrap() else {
        panic!("workspace fixture")
    };
    let expected = WorkspaceAcceptedRefs::from_json(
        &serde_json::json!({"schema_version":1,"members":pins}).to_string(),
    )
    .unwrap();
    (workspace, expected)
}

/// Mixed legacy/readable members preserve root/member settings and each independently selected old pin.
#[test]
fn whole_workspace_form_migration_converts_all_members_without_accepting_working_values() {
    let test = TestDir::new();
    let (workspace, expected) = workspace_fixture(&test);
    let root = workspace.root();
    let settings = fs::read_to_string(root.join("eska.toml")).unwrap();
    let before = files::Inventory::read(root).unwrap();
    let destination = test.0.join("new-workspace");
    let plan = prepare_form_workspace(&workspace, &expected, &destination).unwrap();
    assert!(!destination.exists());
    assert_eq!(plan.members().len(), 2);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
    assert_eq!(files::Inventory::read(root).unwrap(), before);
    assert_eq!(
        fs::read_to_string(destination.join("eska.toml")).unwrap(),
        settings
    );
    let DiscoveryContext::Workspace {
        workspace: copied, ..
    } = discover_context(&destination).unwrap()
    else {
        panic!("workspace copy")
    };
    for (member, preview) in copied.members().iter().zip(plan.members()) {
        assert_eq!(
            member.project().configuration(),
            workspace
                .members()
                .iter()
                .find(|old| old.name() == member.name())
                .unwrap()
                .project()
                .configuration()
        );
        assert_eq!(
            preview.previous_reference(),
            expected.reference(member.name()).unwrap()
        );
        let reader = SourceReader::open(member.project().source()).unwrap();
        assert_eq!(reader.manifest().schema_version, 4);
        let working = reader
            .read_owner(&SourcePath::parse(OWNER).unwrap())
            .unwrap();
        assert!(
            working
                .source
                .form
                .unwrap()
                .to_xml()
                .unwrap()
                .contains(&format!("Working {}", member.name().as_str()))
        );
        let base = AcceptedBase::open(
            &member.project().root().join(".eska/accepted"),
            preview.accepted_reference(),
        )
        .unwrap();
        let restored = test.0.join(format!("accepted-{}", member.name().as_str()));
        base.restore(&restored).unwrap();
        assert!(
            !fs::read_to_string(restored.join(OWNER))
                .unwrap()
                .contains("Working")
        );
    }
    let missing = WorkspaceAcceptedRefs::from_json(
        &serde_json::json!({"schema_version":1,"members":{"one":expected.reference(workspace.members()[0].name()).unwrap().sha256}}).to_string(),
    )
    .unwrap();
    let absent = test.0.join("untrusted");
    assert_eq!(
        prepare_form_workspace(&workspace, &missing, &absent)
            .unwrap_err()
            .code,
        "source_workspace_pins_mismatch"
    );
    assert!(!absent.exists());
}

/// Attachment retains authored metadata, comments and native XML values through block and flow roots.
#[test]
fn form_attachment_preserves_owner_trivia_and_native_bytes_in_block_and_flow_syntax() {
    let test = TestDir::new();
    let (source, _base) = legacy(&test, 4, &[]);
    let original = fs::read_to_string(source.join(OWNER)).unwrap();
    let body = ManagedFormSource::from_xml(XML).unwrap();
    let value = crate::project_core::source_format::yaml::decode(&original).unwrap();
    let flow = format!("{} # flow owner\n", serde_json::to_string(&value).unwrap());
    for input in [
        format!("# keep header\n{original}# keep tail\n"),
        original.replace('\n', "\r\n"),
        flow,
    ] {
        let updated = super::super::cst::attach_form(&input, &body).unwrap();
        let old = crate::project_core::source_format::ObjectSource::from_yaml(&input).unwrap();
        let new = crate::project_core::source_format::ObjectSource::from_yaml(&updated).unwrap();
        assert_eq!(new.document, old.document);
        assert_eq!(new.nested, old.nested);
        assert_eq!(new.form.unwrap().to_xml().unwrap(), XML);
        if !input.starts_with('{') {
            assert!(updated.starts_with(&input));
        }
        if input.contains("\r\n") {
            assert!(!updated.replace("\r\n", "").contains('\n'));
        }
    }
}

/// A mixed authored descriptor transport refuses attachment while mixed native XML remains lossless.
#[test]
fn form_attachment_refuses_mixed_yaml_transport_but_keeps_native_transport() {
    let test = TestDir::new();
    let (source, _base) = legacy(&test, 4, &[]);
    let input = fs::read_to_string(source.join(OWNER)).unwrap();
    let mixed = input.replacen('\n', "\r\n", 1);
    let body = ManagedFormSource::from_xml(XML).unwrap();
    assert_eq!(
        super::super::cst::attach_form(&mixed, &body)
            .unwrap_err()
            .code,
        "source_layout_trivia_unsupported"
    );
    let xml = XML.replacen("\r\n", "\n", 1);
    fs::write(dialog(&source), &xml).unwrap();
    let plan = prepare_form_layout(&source).unwrap();
    let output = test.0.join("new-source");
    plan.publish(&output).unwrap();
    let object = SourceReader::open(&output)
        .unwrap()
        .read_owner(&SourcePath::parse(OWNER).unwrap())
        .unwrap();
    assert_eq!(object.source.form.unwrap().to_xml().unwrap(), xml);
}
