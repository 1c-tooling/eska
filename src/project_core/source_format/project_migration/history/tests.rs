//! History participates in migration ownership without becoming a new common ancestor or support pin.

mod integrity;
mod profiles;
mod publication;
mod workspace;

use super::*;
use crate::{
    project_core::{
        ProjectName, creation,
        discovery::discover,
        metadata_model::ProjectScope,
        source_format::{
            ImportAncestor, ImportAncestorRef, LayoutPublication, capture_import_ancestor,
            prepare_project_source_import, prepare_query_project, project_import_ancestor,
        },
    },
    test_support::TestDir,
};

/// Every source, store and byte comparison belongs to this test's unique playground directory.
fn native(test: &TestDir) -> PathBuf {
    let input = test.0.join("native");
    fs::create_dir_all(input.join("CommonModules/Tools/Ext")).unwrap();
    fs::write(input.join("Configuration.xml"),
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'><Configuration uuid='10000000-0000-4000-8000-000000000001'><Properties><Name>Sample</Name><Comment>Accepted</Comment></Properties><ChildObjects><CommonModule>Tools</CommonModule></ChildObjects></Configuration></MetaDataObject>"
    ).unwrap();
    fs::write(input.join("CommonModules/Tools.xml"),
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'><CommonModule uuid='20000000-0000-4000-8000-000000000001'><Properties><Name>Tools</Name><Server>true</Server></Properties></CommonModule></MetaDataObject>"
    ).unwrap();
    fs::write(
        input.join("CommonModules/Tools/Ext/Module.bsl"),
        b"\xef\xbb\xbf// pristine\r\n",
    )
    .unwrap();
    fs::write(input.join("opaque.bin"), [0, 255, 13, 10]).unwrap();
    input
}

/// Publishing raw incoming data leaves local bytes distinct from the snapshot selected by state v1.
fn publish_incoming(imported: &creation::ImportedProject, test: &TestDir) -> ImportAncestorRef {
    let project = &imported.project;
    let reference = imported.initial_ancestor.as_ref().unwrap();
    let ancestor = project_import_ancestor(project, &ProjectScope::Standalone, reference).unwrap();
    let source = test.0.join("incoming source");
    ancestor.restore(&source).unwrap();
    let owner = source.join("configuration.yaml");
    fs::write(
        &owner,
        fs::read_to_string(&owner)
            .unwrap()
            .replace("Accepted", "Incoming"),
    )
    .unwrap();
    let store = test.0.join("incoming store");
    let reference = capture_import_ancestor(&source, &store).unwrap();
    let incoming = ImportAncestor::open(&store, &reference).unwrap();
    fs::write(
        project.source().join("CommonModules/Tools/Module.bsl"),
        b"\xef\xbb\xbf// local\r\n",
    )
    .unwrap();
    prepare_project_source_import(
        project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &ancestor,
        &incoming,
    )
    .unwrap()
    .publish()
    .unwrap();
    reference
}

/// Both current initial and confirmed published ancestry survive a layout no-op byte-for-byte.
#[test]
fn noop_project_copy_keeps_initial_and_published_history_and_independent_references() {
    for published in [false, true] {
        let test = TestDir::new();
        let input = native(&test);
        let imported = creation::import_query_project(
            &input,
            &test.0.join("old"),
            ProjectName::parse("sample".into()).unwrap(),
            None,
            &[],
        )
        .unwrap();
        let reference = if published {
            publish_incoming(&imported, &test)
        } else {
            imported.initial_ancestor.as_ref().unwrap().clone()
        };
        let project = &imported.project;
        let service = project.root().join(PROJECT_SOURCE_IMPORT_DIRECTORY);
        let original = Inventory::read(&service).unwrap();
        let state = fs::read(service.join("state.json")).unwrap();
        let plan = prepare_query_project(
            project,
            &ProjectScope::Standalone,
            &imported.accepted,
            &test.0.join("new"),
        )
        .unwrap();
        assert!(!plan.layout_changed());
        assert!(plan.ancestry_preserved());
        assert_eq!(plan.accepted_reference(), &imported.accepted);
        assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
        let copied = discover(plan.destination()).unwrap();
        assert_eq!(
            Inventory::read(&copied.root().join(PROJECT_SOURCE_IMPORT_DIRECTORY)).unwrap(),
            original
        );
        assert_eq!(
            fs::read(
                copied
                    .root()
                    .join(PROJECT_SOURCE_IMPORT_DIRECTORY)
                    .join("state.json")
            )
            .unwrap(),
            state
        );
        let ancestor =
            project_import_ancestor(&copied, &ProjectScope::Standalone, &reference).unwrap();
        assert_eq!(ancestor.reference(), &reference);
        assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
        let before = Inventory::read(&service).unwrap();
        assert_eq!(before, original);
        let working = Inventory::read(copied.source()).unwrap();
        let imported_again = prepare_project_source_import(
            &copied,
            &ProjectScope::Standalone,
            &imported.accepted,
            &ancestor,
            &ancestor,
        )
        .unwrap();
        assert!(imported_again.source_plan().conflicts().is_empty());
        imported_again.publish().unwrap();
        assert_eq!(Inventory::read(copied.source()).unwrap(), working);
        project_import_ancestor(&copied, &ProjectScope::Standalone, &reference).unwrap();
    }
}

/// A changing authoring layout cannot drop the old raw snapshot or call a recalculated one confirmed.
#[test]
fn layout_change_with_history_refuses_before_output_and_preserves_old_data() {
    let test = TestDir::new();
    let input = native(&test);
    let imported = creation::import_readable_project(
        &input,
        &test.0.join("old"),
        ProjectName::parse("sample".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let project = &imported.project;
    let external = test.0.join("explicit ancestor");
    let reference = capture_import_ancestor(project.source(), &external).unwrap();
    let ancestor = ImportAncestor::open(&external, &reference).unwrap();
    prepare_project_source_import(
        project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &ancestor,
        &ancestor,
    )
    .unwrap()
    .publish()
    .unwrap();
    let before = Inventory::read(project.root()).unwrap();
    let destination = test.0.join("refused");
    let error = prepare_query_project(
        project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &destination,
    )
    .unwrap_err();
    assert_eq!(error.code, "source_migration_ancestry_unsupported");
    assert!(!destination.exists());
    assert_eq!(Inventory::read(project.root()).unwrap(), before);
}

/// History fingerprints detect intervening edits before copying even when source/base are unchanged.
#[test]
fn changed_history_invalidates_preview_without_publishing_output() {
    let test = TestDir::new();
    let input = native(&test);
    let imported = creation::import_query_project(
        &input,
        &test.0.join("old"),
        ProjectName::parse("sample".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let project = &imported.project;
    let destination = test.0.join("refused");
    let plan = prepare_query_project(
        project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &destination,
    )
    .unwrap();
    fs::write(
        project
            .root()
            .join(PROJECT_SOURCE_IMPORT_DIRECTORY)
            .join("state.json"),
        "{}",
    )
    .unwrap();
    assert_eq!(plan.publish().unwrap_err().code, "source_input_changed");
    assert!(!destination.exists());
}

/// Absence is an expected state; a newly established raw history cannot be dropped by an older preview.
#[test]
fn newly_established_history_invalidates_a_preview_that_had_no_history() {
    let test = TestDir::new();
    let imported = creation::import_query_project(
        &native(&test),
        &test.0.join("old"),
        ProjectName::parse("sample".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let service = imported
        .project
        .root()
        .join(PROJECT_SOURCE_IMPORT_DIRECTORY);
    let retained = test.0.join("temporarily retained raw history");
    fs::rename(&service, &retained).unwrap();
    let destination = test.0.join("refused");
    let plan = prepare_query_project(
        &imported.project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &destination,
    )
    .unwrap();
    assert!(!plan.ancestry_preserved());
    fs::rename(&retained, &service).unwrap();
    assert_eq!(plan.publish().unwrap_err().code, "source_input_changed");
    assert!(!destination.exists());
}
