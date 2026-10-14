//! Whole-workspace preflight retains member history and refuses incompatible layouts before output.

use super::*;
use crate::project_core::{
    Workspace,
    config::ProjectConfig,
    discovery::{DiscoveryContext, discover_context},
    platform_settings::PlatformVersion,
    source_format::{WorkspaceAcceptedRefs, WorkspaceLayoutMember, prepare_query_workspace},
};

/// Member configuration changes do not rewrite raw journal settings or independently retained pins.
fn fixture(
    test: &TestDir,
    legacy_second: bool,
) -> (
    Workspace,
    WorkspaceAcceptedRefs,
    BTreeMap<String, ImportAncestorRef>,
) {
    let input = native(test);
    let root = test.0.join("workspace");
    fs::create_dir_all(root.join("projects")).unwrap();
    let mut pins = BTreeMap::new();
    let mut ancestors = BTreeMap::new();
    for name in ["one", "two"] {
        let legacy = legacy_second && name == "two";
        let platform = (name == "two").then(|| PlatformVersion::parse("8.3.26.1000").unwrap());
        let import = if legacy {
            creation::import_readable_project
        } else {
            creation::import_query_project
        };
        let imported = import(
            &input,
            &root.join("projects").join(name),
            ProjectName::parse(name.into()).unwrap(),
            platform.as_ref(),
            &[],
        )
        .unwrap();
        let reference = if name == "one" {
            publish_incoming(&imported, test)
        } else if legacy {
            let store = test.0.join("legacy ancestor");
            let reference = capture_import_ancestor(imported.project.source(), &store).unwrap();
            let ancestor = ImportAncestor::open(&store, &reference).unwrap();
            prepare_project_source_import(
                &imported.project,
                &ProjectScope::Standalone,
                &imported.accepted,
                &ancestor,
                &ancestor,
            )
            .unwrap()
            .publish()
            .unwrap();
            reference
        } else {
            imported.initial_ancestor.as_ref().unwrap().clone()
        };
        pins.insert(name.to_owned(), imported.accepted.sha256);
        ancestors.insert(name.to_owned(), reference);
        let manifest = imported.project.root().join("eska.toml");
        let config = ProjectConfig::load(&manifest).unwrap();
        fs::write(
            manifest,
            format!(
                "# Exact member {name}\r\n{}",
                config
                    .to_workspace_member_toml()
                    .unwrap()
                    .replace("\n[build]\nplatform_version = \"\"\n", "\n")
            ),
        )
        .unwrap();
    }
    fs::write(root.join("eska.toml"),
        "# Exact root\r\n[workspace]\r\nmembers = ['projects/one', 'projects/two']\r\n[build]\r\nplatform_version = '8.3.27.2325'\r\nartifacts_directory = 'dist'\r\n[vcs.workflow]\r\npreset = 'trunk'\r\n"
    ).unwrap();
    fs::write(
        root.join(".gitattributes"),
        "# Root bytes\r\n*.yaml -text\r\n",
    )
    .unwrap();
    let pins = WorkspaceAcceptedRefs::from_json(
        &serde_json::json!({"schema_version":1,"members":pins}).to_string(),
    )
    .unwrap();
    (workspace(&root), pins, ancestors)
}

/// Discovery supplies the same exact inherited settings used by production migration preflight.
fn workspace(root: &Path) -> Workspace {
    let DiscoveryContext::Workspace { workspace, .. } = discover_context(root).unwrap() else {
        panic!("workspace fixture");
    };
    workspace
}

/// Both raw variants and independent references survive a whole workspace no-op with exact settings.
#[test]
fn workspace_noop_preserves_member_histories_and_inherited_settings() {
    let test = TestDir::new();
    let (old, pins, ancestors) = fixture(&test, false);
    let before = Inventory::read(old.root()).unwrap();
    let plan = prepare_query_workspace(&old, &pins, &test.0.join("copied workspace")).unwrap();
    assert!(!plan.layout_changed());
    assert!(
        plan.members()
            .iter()
            .all(WorkspaceLayoutMember::ancestry_preserved)
    );
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    let copied = workspace(plan.destination());
    assert_eq!(old.build_settings(), copied.build_settings());
    assert_eq!(old.workflow_settings(), copied.workflow_settings());
    assert_eq!(
        fs::read(old.root().join("eska.toml")).unwrap(),
        fs::read(copied.root().join("eska.toml")).unwrap()
    );
    for (original, migrated) in old.members().iter().zip(copied.members()) {
        assert_eq!(
            original.project().configuration(),
            migrated.project().configuration()
        );
        assert_eq!(
            Inventory::read(&original.root().join(PROJECT_SOURCE_IMPORT_DIRECTORY)).unwrap(),
            Inventory::read(&migrated.root().join(PROJECT_SOURCE_IMPORT_DIRECTORY)).unwrap()
        );
        assert_eq!(
            fs::read(original.root().join("eska.toml")).unwrap(),
            fs::read(migrated.root().join("eska.toml")).unwrap()
        );
        project_import_ancestor(
            migrated.project(),
            &ProjectScope::Member(migrated.name().clone()),
            ancestors.get(migrated.name().as_str()).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
    assert_eq!(Inventory::read(old.root()).unwrap(), before);
}

/// A later incompatible member blocks the complete workspace before even the first member is copied.
#[test]
fn historical_layout_change_refuses_the_whole_workspace_before_output() {
    let test = TestDir::new();
    let (old, pins, _) = fixture(&test, true);
    let before = Inventory::read(old.root()).unwrap();
    let destination = test.0.join("refused workspace");
    let error = prepare_query_workspace(&old, &pins, &destination).unwrap_err();
    assert_eq!(error.code, "source_migration_ancestry_unsupported");
    assert_eq!(error.location, vec!["two"]);
    assert!(!destination.exists());
    assert_eq!(Inventory::read(old.root()).unwrap(), before);
}
