//! Project/workspace settings and independently supplied member pins retain their old boundaries.

use super::*;
use crate::project_core::{
    ProjectName, creation,
    discovery::{DiscoveryContext, discover, discover_context},
    metadata_model::ProjectScope,
    source_format::{WorkspaceAcceptedRefs, prepare_query_project, prepare_query_workspace},
};

/// Settings and working values are copied, while the independently accepted list/schema stay original.
#[test]
fn standalone_query_project_retains_exact_settings_and_never_accepts_working_values() {
    let test = TestDir::new();
    let native = fixture(&test);
    let imported = creation::import_project(
        &native,
        &test.0.join("old-project"),
        ProjectName::parse("demo".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let project = &imported.project;
    edit_inline(project.source(), DIALOG, "Working project list");
    edit_inline(project.source(), TEMPLATE, "Working project schema");
    let config = project.root().join("eska.toml");
    let settings = format!(
        "# keep settings\r\n{}",
        fs::read_to_string(&config).unwrap()
    );
    fs::write(&config, &settings).unwrap();
    fs::write(project.root().join("user-notes.md"), "unrelated bytes").unwrap();
    let before = files::Inventory::read(project.root()).unwrap();
    let destination = test.0.join("new-project");
    let plan = prepare_query_project(
        project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &destination,
    )
    .unwrap();
    assert!(!destination.exists());
    assert_eq!(plan.previous_reference(), &imported.accepted);
    assert_ne!(plan.accepted_reference(), &imported.accepted);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
    assert_eq!(files::Inventory::read(project.root()).unwrap(), before);
    let copied = discover(&destination).unwrap();
    assert_eq!(copied.configuration(), project.configuration());
    assert_eq!(
        fs::read_to_string(destination.join("eska.toml")).unwrap(),
        settings
    );
    assert!(!destination.join("user-notes.md").exists());
    let reader = SourceReader::open(copied.source()).unwrap();
    assert_eq!(reader.manifest().schema_version, 5);
    assert_eq!(reader.query_paths().count(), 4);
    let base = AcceptedBase::open(
        &destination.join(".eska/accepted"),
        plan.accepted_reference(),
    )
    .unwrap();
    let restored = test.0.join("restored");
    base.restore(&restored).unwrap();
    let pinned = SourceReader::open(&restored).unwrap();
    for path in reader.query_paths() {
        let current = reader.read_query_text(path, u64::MAX).unwrap();
        let previous = pinned.read_query_text(path, u64::MAX).unwrap();
        if previous.is_empty() {
            assert_eq!(current, previous);
        } else {
            assert!(current.contains("Working project"));
        }
        assert!(
            !pinned
                .read_query_text(path, u64::MAX)
                .unwrap()
                .contains("Working project")
        );
    }
    let old_output = test.0.join("old-output");
    export_designer(project.source(), &old_output).unwrap();
    let selected =
        prepare_protected(copied.source(), &base, &test.0.join("generations"), true).unwrap();
    assert_eq!(
        files::Inventory::read(&old_output).unwrap(),
        files::Inventory::read(selected.xml()).unwrap()
    );
}

const WORKSPACE_SETTINGS: &str = "# workspace settings\r\n[workspace]\r\nmembers = ['projects/one','projects/two']\r\n[build]\r\nplatform_version = '8.3.27.2325'\r\nartifacts_directory = 'dist'\r\n";

/// Mixed member layouts are imported only into this test's owned playground tree.
fn workspace_fixture(
    test: &TestDir,
) -> (
    crate::project_core::Workspace,
    WorkspaceAcceptedRefs,
    BTreeMap<String, String>,
) {
    let native = fixture(test);
    let root = test.0.join("workspace");
    fs::create_dir_all(root.join("projects")).unwrap();
    let mut pins = BTreeMap::new();
    let mut settings = BTreeMap::new();
    for (name, readable) in [("one", false), ("two", true)] {
        let import = if readable {
            creation::import_readable_project
        } else {
            creation::import_project
        };
        let imported = import(
            &native,
            &root.join("projects").join(name),
            ProjectName::parse(name.into()).unwrap(),
            None,
            &[],
        )
        .unwrap();
        pins.insert(name, imported.accepted.sha256);
        let config = crate::project_core::config::ProjectConfig::load(
            &imported.project.root().join("eska.toml"),
        )
        .unwrap();
        let member_config = config
            .to_workspace_member_toml()
            .unwrap()
            .replace("\n[build]\nplatform_version = \"\"\n", "\n");
        fs::write(imported.project.root().join("eska.toml"), &member_config).unwrap();
        settings.insert(name.to_owned(), member_config);
        edit_inline(
            imported.project.source(),
            DIALOG,
            &format!("Working {name}"),
        );
        edit_inline(
            imported.project.source(),
            TEMPLATE,
            &format!("Working {name}"),
        );
    }
    fs::write(root.join("eska.toml"), WORKSPACE_SETTINGS).unwrap();
    let DiscoveryContext::Workspace { workspace, .. } = discover_context(&root).unwrap() else {
        panic!("workspace fixture")
    };
    let expected = WorkspaceAcceptedRefs::from_json(
        &serde_json::json!({"schema_version":1,"members":pins}).to_string(),
    )
    .unwrap();
    (workspace, expected, settings)
}

/// Whole-workspace publication retains mixed source layouts, inheritance and every member's explicit pin.
#[test]
fn query_workspace_migrates_all_members_and_refuses_missing_trust_before_publication() {
    let test = TestDir::new();
    let (workspace, expected, settings) = workspace_fixture(&test);
    let root = workspace.root();
    let before = files::Inventory::read(root).unwrap();
    let destination = test.0.join("new-workspace");
    let plan = prepare_query_workspace(&workspace, &expected, &destination).unwrap();
    assert!(!destination.exists());
    assert_eq!(plan.members().len(), 2);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
    assert_eq!(files::Inventory::read(root).unwrap(), before);
    assert_eq!(
        fs::read_to_string(destination.join("eska.toml")).unwrap(),
        WORKSPACE_SETTINGS
    );
    let DiscoveryContext::Workspace {
        workspace: copied, ..
    } = discover_context(&destination).unwrap()
    else {
        panic!("workspace copy")
    };
    for (member, preview) in copied.members().iter().zip(plan.members()) {
        assert_eq!(
            preview.previous_reference(),
            expected.reference(member.name()).unwrap()
        );
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
            fs::read_to_string(member.project().root().join("eska.toml")).unwrap(),
            settings[member.name().as_str()]
        );
        let reader = SourceReader::open(member.project().source()).unwrap();
        assert_eq!(reader.manifest().schema_version, 5);
        let base = AcceptedBase::open(
            &member.project().root().join(".eska/accepted"),
            preview.accepted_reference(),
        )
        .unwrap();
        let restored = test.0.join(format!("accepted-{}", member.name().as_str()));
        base.restore(&restored).unwrap();
        let pinned = SourceReader::open(&restored).unwrap();
        for path in reader.query_paths() {
            let current = reader.read_query_text(path, u64::MAX).unwrap();
            let previous = pinned.read_query_text(path, u64::MAX).unwrap();
            if previous.is_empty() {
                assert_eq!(current, previous);
            } else {
                assert!(current.contains(&format!("Working {}", member.name().as_str())));
            }
            assert!(
                !pinned
                    .read_query_text(path, u64::MAX)
                    .unwrap()
                    .contains("Working")
            );
        }
    }
    let missing = WorkspaceAcceptedRefs::from_json(
        &serde_json::json!({"schema_version":1,"members":{"one":expected.reference(workspace.members()[0].name()).unwrap().sha256}}).to_string(),
    )
    .unwrap();
    let absent = test.0.join("untrusted");
    assert_eq!(
        prepare_query_workspace(&workspace, &missing, &absent)
            .unwrap_err()
            .code,
        "source_workspace_pins_mismatch"
    );
    assert!(!absent.exists());
}
