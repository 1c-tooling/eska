use super::*;
use crate::backend::project::{
    metadata_edit::snapshot,
    metadata_workspace::{QueryDocument, QueryDocumentError, QueryRevision},
    query::{QueryError, QueryIdentity},
};

const INPUT: &str = "\u{feff}Процедура Тест()\r\n\tТекст = \"ВЫБРАТЬ 1\r\n\t\t|КАК Значение\"; // keep\r\nКонецПроцедуры\r\n";

/// The queried module is a real declared root source inside a uniquely owned playground project.
fn setup() -> (TestDir, MetadataWorkspace, NodeId, PathBuf) {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let path = PathBuf::from("Ext/ManagedApplicationModule.bsl");
    fs::write(directory.0.join("src").join(&path), INPUT).unwrap();
    fs::write(
        directory.0.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let NodeId::Object(owner) = workspace.projects()[0].root() else {
        panic!("root object")
    };
    let node = NodeId::Module {
        owner: owner.clone(),
        role: ModuleRole::ManagedApplication,
    };
    (directory, workspace, node, path)
}

/// The revision belongs to the captured editor buffer rather than to the disk snapshot.
fn revision(document: &QueryDocument) -> QueryRevision<'_> {
    QueryRevision {
        version: document.version(),
        snapshot: document.snapshot(),
    }
}

/// Find the complete literal using the backend's byte-position contract.
fn identity(document: &QueryDocument) -> QueryIdentity {
    document
        .bsl_query(document.input().find('"').unwrap())
        .unwrap()
        .identity
}

/// Planning over a dirty owner leaves its unrelated changes, file bytes and metadata generation intact.
#[test]
fn query_plans_use_unsaved_owner_without_writing_or_refreshing_the_project() {
    let (directory, mut workspace, node, path) = setup();
    let dirty = INPUT.replace("// keep", "// dirty owner");
    let before = bytes(&directory.0);
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let generation = project.generation();
    let document = project
        .query_document(&node, &path, 7, Some(&dirty))
        .unwrap();
    assert!(document.writable());
    assert_eq!(document.snapshot(), snapshot(&dirty));
    let result = project
        .plan_query(
            &document,
            revision(&document),
            &identity(&document),
            "ВЫБРАТЬ 2\nКАК Значение",
            &[],
        )
        .unwrap();
    assert_eq!(result.output(), dirty.replacen("ВЫБРАТЬ 1", "ВЫБРАТЬ 2", 1));
    assert_eq!(project.generation(), generation);
    assert_eq!(bytes(&directory.0), before);
}

/// Metadata generation, editor version and full owner hash independently invalidate a plan.
#[test]
fn query_rejects_stale_editor_and_metadata_revisions() {
    let (_directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 1, None).unwrap();
    for stale in [
        QueryRevision {
            version: 2,
            snapshot: document.snapshot(),
        },
        QueryRevision {
            version: 1,
            snapshot: "stale",
        },
    ] {
        assert!(matches!(
            project.check_query_document(&document, stale),
            Err(QueryDocumentError::StaleVersion)
        ));
    }
    project.changed_paths(&[path]).unwrap();
    assert!(matches!(
        project.check_query_document(&document, revision(&document)),
        Err(QueryDocumentError::OwnerChanged)
    ));
}

/// Watcher delivery is not required to detect an external source edit or replacement of its metadata UUID.
#[test]
fn query_checks_current_disk_bytes_and_owner_uuid_without_events() {
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 1, None).unwrap();
    fs::write(
        directory.0.join("src").join(&path),
        INPUT.replace("keep", "external"),
    )
    .unwrap();
    assert!(matches!(
        project.check_query_document(&document, revision(&document)),
        Err(QueryDocumentError::Query(QueryError::Conflict))
    ));
    fs::write(directory.0.join("src").join(&path), INPUT).unwrap();
    let root = directory.0.join("src/Configuration.xml");
    fs::write(
        &root,
        fs::read_to_string(&root).unwrap().replace(
            "11111111-1111-1111-1111-111111111111",
            "33333333-3333-3333-3333-333333333333",
        ),
    )
    .unwrap();
    assert!(matches!(
        project.check_query_document(&document, revision(&document)),
        Err(QueryDocumentError::OwnerChanged)
    ));
}

/// Same-scope IDs from a different workspace or another module path cannot authorize a query edit.
#[test]
fn query_rejects_foreign_projects_and_unresolved_source_paths() {
    let (_directory, mut workspace, node, path) = setup();
    let (_other_directory, mut other, _, _) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 1, None).unwrap();
    assert!(matches!(
        other
            .project_mut(&ProjectScope::Standalone)
            .unwrap()
            .check_query_document(&document, revision(&document)),
        Err(QueryDocumentError::OwnerChanged)
    ));
    for path in [
        Path::new("../escape.bsl"),
        Path::new("/absolute.bsl"),
        Path::new("Ext/SessionModule.bsl"),
    ] {
        assert!(project.query_document(&node, path, 1, Some(INPUT)).is_err());
    }
    assert!(matches!(
        project.query_document(&node, &path, 0, None),
        Err(QueryDocumentError::StaleVersion)
    ));
}

/// A policy update is read directly from its current bytes and never inferred from a cached writable flag.
#[test]
fn query_cannot_edit_after_support_policy_locks_the_owner() {
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 1, None).unwrap();
    let root = "11111111-1111-1111-1111-111111111111";
    let rules = format!(
        "{{6,0,1,{root},0,{root},\"1\",\"Vendor\",\"Test\",1,0,0,{root},{root},0,0,0,1,0,0,0,1,0,1,0,1,1,1,1}}"
    );
    fs::write(directory.0.join("src/Ext/ParentConfigurations.bin"), rules).unwrap();
    let policy = project.support_files(std::slice::from_ref(&path)).unwrap();
    assert_eq!(policy.diagnostics, [] as [String; 0]);
    assert!(!policy.files[0].unknown);
    assert!(policy.files[0].read_only);
    assert!(matches!(
        project.plan_query(
            &document,
            revision(&document),
            &identity(&document),
            "ВЫБРАТЬ 2",
            &[]
        ),
        Err(QueryDocumentError::ReadOnly)
    ));
    let forbidden = project.query_document(&node, &path, 2, None).unwrap();
    assert!(!forbidden.writable());
    assert_eq!(
        forbidden
            .bsl_query(forbidden.input().find('"').unwrap())
            .unwrap()
            .text,
        "ВЫБРАТЬ 1\nКАК Значение"
    );
}

/// Real source mappings distinguish form and template payloads even when their buffers are dirty.
#[test]
fn query_xml_plans_bind_form_and_template_values_to_their_declared_owner() {
    let (directory, workspace, _, _) = setup();
    drop(workspace);
    let source = directory.0.join("src");
    let root = source.join("Configuration.xml");
    fs::write(&root, fs::read_to_string(&root).unwrap().replace("<ChildObjects>", "<ChildObjects><CommonForm>QueryForm</CommonForm><CommonTemplate>QuerySchema</CommonTemplate>")).unwrap();
    let form = "<Form xmlns='http://v8.1c.ru/8.3/xcf/logform' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance'><Attributes><Attribute name='Список' id='1'><Settings xsi:type='DynamicList'><ManualQuery>true</ManualQuery><QueryText>SELECT 1</QueryText><MainTable>Catalog.Test</MainTable></Settings></Attribute></Attributes></Form>";
    let schema = "<DataCompositionSchema xmlns='http://v8.1c.ru/8.1/data-composition-system/schema' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance'><dataSet xsi:type='DataSetQuery'><name>A</name><query>SELECT 1</query></dataSet></DataCompositionSchema>";
    for (kind, tag, name, content, suffix) in [
        (
            MetadataKind::CommonForm,
            "CommonForm",
            "QueryForm",
            form,
            "Form.xml",
        ),
        (
            MetadataKind::CommonTemplate,
            "CommonTemplate",
            "QuerySchema",
            schema,
            "Template.xml",
        ),
    ] {
        let folder = format!("{tag}s");
        let descriptor = source.join(&folder).join(format!("{name}.xml"));
        fs::create_dir_all(descriptor.parent().unwrap()).unwrap();
        fs::write(&descriptor, format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'><{tag} uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>{name}</Name></Properties><ChildObjects/></{tag}></MetaDataObject>")).unwrap();
        let path = PathBuf::from(folder).join(name).join("Ext").join(suffix);
        fs::create_dir_all(source.join(&path).parent().unwrap()).unwrap();
        fs::write(source.join(&path), content).unwrap();
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let node = NodeId::Object(object(kind, name, None));
        let document = project.query_document(&node, &path, 3, None).unwrap();
        let query = document.xml_queries().unwrap().remove(0);
        let result = project
            .plan_query(
                &document,
                revision(&document),
                &query.identity,
                "SELECT 2",
                &[],
            )
            .unwrap();
        assert_eq!(result.output(), content.replace("SELECT 1", "SELECT 2"));
        assert_eq!(fs::read_to_string(source.join(&path)).unwrap(), content);
        let wrong = if kind == MetadataKind::CommonForm {
            schema
        } else {
            form
        };
        let document = project
            .query_document(&node, &path, 4, Some(wrong))
            .unwrap();
        assert_eq!(
            document.xml_queries(),
            Err(QueryError::UnsupportedStructure)
        );
        let empty_wrong = if kind == MetadataKind::CommonForm {
            "<DataCompositionSchema xmlns='http://v8.1c.ru/8.1/data-composition-system/schema'/>"
        } else {
            "<Form xmlns='http://v8.1c.ru/8.3/xcf/logform'/>"
        };
        let document = project
            .query_document(&node, &path, 5, Some(empty_wrong))
            .unwrap();
        assert_eq!(
            document.xml_queries(),
            Err(QueryError::UnsupportedStructure)
        );
    }
}

/// Unreadable policy and symlink aliases never turn a previously writable query into an allowed edit.
#[test]
fn query_unknown_policy_and_aliased_sources_fail_closed() {
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 1, None).unwrap();
    fs::remove_file(directory.0.join("src/Ext/ParentConfigurations.bin")).unwrap();
    assert!(matches!(
        project.check_query_document(&document, revision(&document)),
        Err(QueryDocumentError::ReadOnly)
    ));
    #[cfg(unix)]
    {
        fs::write(
            directory.0.join("src/Ext/ParentConfigurations.bin"),
            "{6,0,0,0,0,0}",
        )
        .unwrap();
        let target = directory.0.join("src").join(&path);
        let owned = directory.0.join("owned.bsl");
        fs::rename(&target, &owned).unwrap();
        std::os::unix::fs::symlink(&owned, &target).unwrap();
        assert!(
            project
                .check_query_document(&document, revision(&document))
                .is_err()
        );
        assert!(project.query_document(&node, &path, 2, None).is_err());
        assert_eq!(fs::read_to_string(owned).unwrap(), INPUT);
    }
}

/// Both disk and unsaved inputs obey the same byte bound, including truncated UTF-8 at the read limit.
#[test]
fn query_bounds_disk_and_unsaved_owner_snapshots() {
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let oversized = "😀".repeat(crate::backend::project::query::MAX_SNAPSHOT_BYTES / 4 + 1);
    assert!(matches!(
        project.query_document(&node, &path, 1, Some(&oversized)),
        Err(QueryDocumentError::Query(QueryError::TooLarge))
    ));
    fs::write(directory.0.join("src").join(&path), &oversized).unwrap();
    assert!(matches!(
        project.query_document(&node, &path, 1, None),
        Err(QueryDocumentError::Query(QueryError::TooLarge))
    ));
}
