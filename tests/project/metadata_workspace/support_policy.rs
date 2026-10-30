use crate::backend::project::{
    metadata_model::{ObjectId, ProjectScope},
    metadata_workspace::MetadataWorkspace,
    support::{Reason, State},
};
use crate::support::TestDir;
use std::{fmt::Write, fs, path::Path};
const ROOT: &str = "11111111-1111-1111-1111-111111111111";
const CHILD: &str = "22222222-2222-2222-2222-222222222222";

/// Build a small configuration with an inline child and a separately stored module.
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("src/Catalogs/Items/Ext")).unwrap();
    fs::create_dir_all(root.join("src/Ext")).unwrap();
    fs::write(root.join("eska.toml"), "[project]\ntype='configuration'\n").unwrap();
    fs::write(
        root.join("src/Configuration.xml"),
        xml("Configuration", "Test", "<Catalog>Items</Catalog>"),
    )
    .unwrap();
    fs::write(
        root.join("src/Catalogs/Items.xml"),
        xml(
            "Catalog",
            "Items",
            &format!(
                "<Attribute uuid=\"{CHILD}\"><Properties><Name>Code</Name></Properties></Attribute>"
            ),
        ),
    )
    .unwrap();
    fs::write(
        root.join("src/Catalogs/Items/Ext/ObjectModule.bsl"),
        "// unchanged",
    )
    .unwrap();
}
/// Generate only metadata needed for source ownership assertions.
fn xml(kind: &str, name: &str, children: &str) -> String {
    format!(
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><{kind} uuid=\"{ROOT}\"><Properties><Name>{name}</Name></Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>"
    )
}
/// Keep supplier rules explicit; parent editing cannot unlock its inline child.
fn rules(mode: u8, rule: u8) -> String {
    format!(
        "{{6,0,1,{ROOT},{mode},{ROOT},\"1\",\"Vendor\",\"Test\",2,{rule},0,{ROOT},{ROOT},0,0,{CHILD},{CHILD},0,0,0,1,0,0,0,1,0,1,0,1,1,1,1}}"
    )
}

/// Display batches reuse the declared owner chain but never substitute for a mixed-file permission check.
#[test]
fn object_support_batches_are_current_lazy_and_independent_of_file_permissions() {
    let dir = TestDir::new();
    fixture(&dir.0);
    let policy = dir.0.join("src/Ext/ParentConfigurations.bin");
    fs::write(&policy, rules(0, 1)).unwrap();
    let before = super::bytes(&dir.0);
    let ids: Vec<ObjectId> = [
        "catalog:Items".to_owned(),
        "catalog:Items/attribute:Code".to_owned(),
        "catalog:NotDeclared".to_owned(),
        "form:Deep/".repeat(1024),
    ]
    .into_iter()
    .map(|id| serde_json::from_value(serde_json::json!(id)).unwrap())
    .collect();
    let mut workspace = MetadataWorkspace::open(&dir.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let reads = session.source_io_stats().xml_reads;
    let display = session.support_objects(&ids);
    assert_eq!(session.source_io_stats().xml_reads - reads, 2);
    assert_eq!(display.objects.len(), ids.len());
    assert_eq!(display.objects[0].state, State::EditableWithSupport);
    assert_eq!(display.objects[1].state, State::Locked);
    assert!(
        display.objects[2..]
            .iter()
            .all(|object| object.state == State::Unknown)
    );
    assert_eq!(
        session.search_progress().state,
        crate::backend::project::metadata_workspace::search::IndexState::NotStarted
    );
    let file = session
        .support_files(&["Catalogs/Items.xml".into()])
        .unwrap();
    assert!(file.files[0].read_only && file.files[0].mixed);
    assert_eq!(before, super::bytes(&dir.0));
    for rule in [0, 2, 1] {
        fs::write(&policy, rules(0, rule)).unwrap();
        let actual = session.support_objects(&ids[..1]);
        assert_eq!(
            actual.objects[0].state,
            [
                State::Locked,
                State::EditableWithSupport,
                State::Unrestricted
            ][usize::from(rule)]
        );
    }
    fs::write(&policy, "broken").unwrap();
    assert_eq!(
        session.support_objects(&ids[..1]).objects[0].state,
        State::Unknown
    );
    fs::write(&policy, rules(0, 1)).unwrap();
    fs::write(
        dir.0.join("src/Configuration.xml"),
        xml("Configuration", "Test", ""),
    )
    .unwrap();
    assert_eq!(
        session.support_objects(&ids[..1]).objects[0].state,
        State::Unknown
    );
}

#[test]
fn targeted_permissions_follow_current_bytes_without_scan_or_file_events() {
    let dir = TestDir::new();
    fixture(&dir.0);
    let rules_path = dir.0.join("src/Ext/ParentConfigurations.bin");
    fs::write(&rules_path, rules(0, 1)).unwrap();
    // An unrelated corrupt descriptor must not prevent checking this module.
    fs::write(dir.0.join("src/Catalogs/Unrelated.xml"), "broken").unwrap();
    let original = super::bytes(&dir.0);
    let mut workspace = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let paths = [
        "Catalogs/Items/Ext/ObjectModule.bsl".into(),
        "Catalogs/Items.xml".into(),
    ];
    let reads = session.source_io_stats().xml_reads;
    let result = session.support_files(&paths).unwrap();
    assert!(!result.files[0].read_only);
    assert!(result.files[1].read_only && result.files[1].mixed);
    assert_eq!(session.source_io_stats().xml_reads - reads, 2);
    assert!(result.next_offset.is_none());
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    // A -> B -> A: no generation notification or timestamp-based permission reuse.
    for rule in [0, 1, 0, 2] {
        fs::write(&rules_path, rules(0, rule)).unwrap();
        let result = session.support_files(&paths).unwrap();
        assert_eq!(result.files[0].read_only, rule == 0);
        assert!(result.files[1].read_only);
    }
    // Same rules, different ancestry: disappearance never grants permission.
    fs::write(
        dir.0.join("src/Configuration.xml"),
        xml("Configuration", "Test", ""),
    )
    .unwrap();
    assert!(
        session
            .support_files(&paths)
            .unwrap()
            .files
            .iter()
            .all(|file| file.read_only && file.unknown)
    );
    fs::write(
        dir.0.join("src/Configuration.xml"),
        &original[Path::new("src/Configuration.xml")],
    )
    .unwrap();
    fs::write(&rules_path, "broken").unwrap();
    assert!(
        session
            .support_files(&paths)
            .unwrap()
            .files
            .iter()
            .all(|file| file.read_only && file.unknown)
    );
    assert!(session.support_files(&["../elsewhere.bsl".into()]).is_err());
    assert!(
        session
            .support_files(&["Ext/ParentConfigurations.bin".into()])
            .unwrap()
            .files[0]
            .read_only
    );
}

/// Every batch needs the root; filling the bounded cache with children must not evict it.
#[test]
fn support_object_batches_keep_the_shared_root_parse() {
    let dir = TestDir::new();
    fixture(&dir.0);
    fs::write(dir.0.join("src/Ext/ParentConfigurations.bin"), rules(0, 0)).unwrap();
    let names: Vec<_> = (0..129).map(|index| format!("Item{index:03}")).collect();
    let mut declarations = String::new();
    for name in &names {
        write!(declarations, "<Catalog>{name}</Catalog>").unwrap();
    }
    fs::write(
        dir.0.join("src/Configuration.xml"),
        xml("Configuration", "Test", &declarations),
    )
    .unwrap();
    for name in &names {
        fs::write(
            dir.0.join(format!("src/Catalogs/{name}.xml")),
            xml("Catalog", name, ""),
        )
        .unwrap();
    }
    let ids: Vec<ObjectId> = names
        .iter()
        .map(|name| serde_json::from_value(serde_json::json!(format!("catalog:{name}"))).unwrap())
        .collect();
    let mut workspace = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    assert_eq!(session.support_objects(&ids[..128]).objects.len(), 128);
    let hits = session.disk_cache_stats().unwrap().hits;
    assert_eq!(
        session.support_objects(&ids[128..]).objects[0].state,
        State::Locked
    );
    assert_eq!(session.disk_cache_stats().unwrap().hits, hits);
    // Retention never bypasses the current-byte check after a missed filesystem event.
    fs::write(
        dir.0.join("src/Configuration.xml"),
        xml("Configuration", "Test", ""),
    )
    .unwrap();
    assert_eq!(
        session.support_objects(&ids[128..]).objects[0].state,
        State::Unknown
    );
}

#[test]
fn targeted_policy_matches_source_ownership_across_all_project_types() {
    for kind in ["configuration", "extension", "processing", "report"] {
        let directory = TestDir::new();
        super::fixture(&directory.0, kind);
        fs::create_dir_all(directory.0.join("src/Ext")).unwrap();
        fs::write(
            directory.0.join("src/Ext/ParentConfigurations.bin"),
            "{6,0,0,0,0,0}",
        )
        .unwrap();
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let inventory = session.support_page(0).unwrap();
        assert!(inventory.next_offset.is_none());
        let paths: Vec<_> = inventory
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect();
        let targeted = session.support_files(&paths).unwrap();
        let ids: Vec<_> = inventory
            .objects
            .iter()
            .map(|object| object.object_id.clone())
            .collect();
        let display = session.support_objects(&ids);
        assert_eq!(
            serde_json::to_value(&display.objects).unwrap(),
            serde_json::to_value(&inventory.objects).unwrap(),
            "{kind}: {:?}",
            display.diagnostics
        );
        for (expected, actual) in inventory.files.iter().zip(&targeted.files) {
            assert_eq!(
                (expected.read_only, expected.unknown, &expected.objects),
                (actual.read_only, actual.unknown, &actual.objects),
                "{kind}: {:?} {:?}",
                expected.path,
                targeted.diagnostics
            );
        }
    }
}

/// EPF/ERF need no supplier file, but broken ownership must still deny editing.
#[test]
fn external_artifacts_need_no_configuration_support_file() {
    for kind in ["processing", "report"] {
        let directory = TestDir::new();
        super::fixture(&directory.0, kind);
        let before = super::bytes(&directory.0);
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let inventory = session.support_page(0).unwrap();
        assert!(inventory.next_offset.is_none());
        assert!(
            inventory.diagnostics.is_empty(),
            "{kind}: {:?}",
            inventory.diagnostics
        );
        assert!(!inventory.files.is_empty());
        let paths: Vec<_> = inventory
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect();
        let targeted = session.support_files(&paths).unwrap();
        for snapshot in [&*inventory, &targeted] {
            assert!(
                snapshot.diagnostics.is_empty(),
                "{kind}: {:?}",
                snapshot.diagnostics
            );
            assert!(
                snapshot
                    .objects
                    .iter()
                    .all(|object| object.state == State::Unrestricted
                        && object.reason == Reason::OwnObject)
            );
            assert!(
                snapshot
                    .files
                    .iter()
                    .all(|file| !file.read_only && !file.unknown)
            );
        }
        assert_eq!(before, super::bytes(&directory.0));
        let module = paths
            .iter()
            .find(|path| path.ends_with("Form/Module.bsl"))
            .unwrap();
        let descriptor = if kind == "processing" {
            "Выгрузка/Forms/Основная.xml"
        } else {
            "Forms/Основная.xml"
        };
        fs::write(directory.0.join("src").join(descriptor), "broken").unwrap();
        let unknown = session.support_files(std::slice::from_ref(module)).unwrap();
        assert!(unknown.files[0].read_only && unknown.files[0].unknown);
    }
}

#[test]
fn targeted_permissions_follow_descriptor_uuid_with_unchanged_mtime() {
    let directory = TestDir::new();
    fixture(&directory.0);
    fs::write(
        directory.0.join("src/Ext/ParentConfigurations.bin"),
        rules(0, 0),
    )
    .unwrap();
    let descriptor = directory.0.join("src/Catalogs/Items.xml");
    let input = fs::read_to_string(&descriptor).unwrap();
    let modified = fs::metadata(&descriptor).unwrap().modified().unwrap();
    let mut workspace = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    for uuid in [ROOT, "33333333-3333-3333-3333-333333333333", ROOT] {
        fs::write(&descriptor, input.replacen(ROOT, uuid, 1)).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&descriptor)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let result = session
            .support_files(&["Catalogs/Items/Ext/ObjectModule.bsl".into()])
            .unwrap();
        assert_eq!(result.files[0].read_only, uuid == ROOT);
        assert!(!result.files[0].unknown);
    }
}

#[cfg(unix)]
#[test]
fn targeted_policy_rejects_file_and_descriptor_aliases() {
    use std::os::unix::fs::symlink;
    let directory = TestDir::new();
    fixture(&directory.0);
    fs::write(
        directory.0.join("src/Ext/ParentConfigurations.bin"),
        rules(0, 1),
    )
    .unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let path = "Catalogs/Items/Ext/ObjectModule.bsl".into();
    let original = directory.0.join("src").join(&path);
    fs::rename(&original, directory.0.join("module.bsl")).unwrap();
    symlink(directory.0.join("module.bsl"), &original).unwrap();
    assert!(
        session
            .support_files(std::slice::from_ref(&path))
            .unwrap()
            .files[0]
            .unknown
    );
    fs::remove_file(&original).unwrap();
    fs::rename(directory.0.join("module.bsl"), &original).unwrap();
    let descriptor = directory.0.join("src/Catalogs/Items.xml");
    fs::rename(&descriptor, directory.0.join("src/alias.xml")).unwrap();
    symlink(directory.0.join("src/alias.xml"), descriptor).unwrap();
    assert!(session.support_files(&[path]).unwrap().files[0].unknown);
}
#[test]
fn support_separates_mixed_xml_from_module_and_invalidates_without_writes() {
    let dir = TestDir::new();
    fixture(&dir.0);
    let path = dir.0.join("src/Ext/ParentConfigurations.bin");
    fs::write(&path, rules(0, 1)).unwrap();
    fs::write(dir.0.join("src/Catalogs/Items/Ext/Predefined.xml"), format!("<PredefinedData xmlns='http://v8.1c.ru/8.3/xcf/predef' version='2.20'><Item id='{CHILD}'><Name>Fixed</Name></Item></PredefinedData>")).unwrap();
    let original = super::bytes(&dir.0);
    let mut workspace = MetadataWorkspace::open(&dir.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let first = session.support_page(0).unwrap();
    assert!(first.next_offset.is_none());
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    let xml = first
        .files
        .iter()
        .find(|f| f.path == Path::new("Catalogs/Items.xml"))
        .unwrap();
    assert!(xml.read_only && xml.mixed);
    let module = first
        .files
        .iter()
        .find(|f| f.path.extension().is_some_and(|e| e == "bsl"))
        .unwrap();
    assert!(!module.read_only);
    assert!(
        first
            .objects
            .iter()
            .any(|o| o.uuid == ROOT && o.state == State::EditableWithSupport)
    );
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &session.support_page(0).unwrap()
    ));
    assert_eq!(original, super::bytes(&dir.0));
    assert!(
        first
            .files
            .iter()
            .any(|file| file.path.ends_with("Predefined.xml") && file.read_only && file.mixed)
    );
    assert!(first.objects.iter().any(
        |object| object.object_id.as_str().contains("predefined-item")
            && object.state == State::Locked
    ));
    let ids: Vec<_> = first
        .objects
        .iter()
        .map(|object| object.object_id.clone())
        .collect();
    let display = session.support_objects(&ids);
    assert_eq!(
        serde_json::to_value(&display.objects).unwrap(),
        serde_json::to_value(&first.objects).unwrap()
    );
    fs::write(
        dir.0.join("src/Catalogs/Items/Ext/ObjectModule.bsl"),
        "// edited",
    )
    .unwrap();
    session
        .changed_paths(&["Catalogs/Items/Ext/ObjectModule.bsl".into()])
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &session.support_page(0).unwrap()
    ));

    fs::write(&path, "damaged").unwrap();
    session
        .changed_paths(&["Ext/ParentConfigurations.bin".into()])
        .unwrap();
    let broken = session.support_page(0).unwrap();
    assert_ne!(broken.diagnostics, [] as [String; 0]);
    assert!(broken.objects.iter().all(|o| o.state == State::Unknown));
    assert!(broken.files.iter().all(|f| f.read_only));
    fs::write(&path, rules(0, 2)).unwrap();
    session
        .changed_paths(&["Ext/ParentConfigurations.bin".into()])
        .unwrap();
    let removed = session.support_page(0).unwrap();
    assert!(
        removed
            .objects
            .iter()
            .any(|o| o.uuid == ROOT && o.reason == Reason::SupportRemoved)
    );
    assert!(
        !removed
            .files
            .iter()
            .find(|f| f.path.extension().is_some_and(|e| e == "bsl"))
            .unwrap()
            .read_only
    );
}
#[test]
fn identical_uuids_in_different_sessions_do_not_share_policy() {
    let first = TestDir::new();
    let second = TestDir::new();
    for dir in [&first, &second] {
        fixture(&dir.0);
    }
    fs::write(
        first.0.join("src/Ext/ParentConfigurations.bin"),
        rules(1, 1),
    )
    .unwrap();
    let mut locked = MetadataWorkspace::open(&first.0, &[], false).unwrap();
    let mut missing = MetadataWorkspace::open(&second.0, &[], false).unwrap();
    let locked = locked
        .project_mut(&ProjectScope::Standalone)
        .unwrap()
        .support_page(0)
        .unwrap();
    let missing = missing
        .project_mut(&ProjectScope::Standalone)
        .unwrap()
        .support_page(0)
        .unwrap();
    assert!(locked.files.iter().all(|f| f.read_only));
    assert!(missing.objects.iter().all(|o| o.state == State::Unknown));
    assert!(missing.files.iter().all(|f| !f.read_only));
    assert_ne!(missing.diagnostics, [] as [String; 0]);
}

#[test]
fn background_reuses_loaded_descriptors_and_refresh_checks_rule_contents() {
    use crate::backend::project::{configurator::TreeOptions, metadata_model::NodeId};
    use std::fmt::Write;
    let dir = TestDir::new();
    fixture(&dir.0);
    fs::create_dir_all(dir.0.join("src/CommonModules")).unwrap();
    let mut references = String::new();
    for i in 0..80 {
        write!(references, "<CommonModule>M{i:03}</CommonModule>").unwrap();
    }
    fs::write(
        dir.0.join("src/Configuration.xml"),
        xml("Configuration", "Test", &references),
    )
    .unwrap();
    for i in 0..80 {
        fs::write(
            dir.0.join(format!("src/CommonModules/M{i:03}.xml")),
            xml("CommonModule", &format!("M{i:03}"), ""),
        )
        .unwrap();
    }
    fs::write(dir.0.join("src/Ext/ParentConfigurations.bin"), rules(0, 0)).unwrap();
    let mut workspace = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let priority: crate::backend::project::metadata_model::ObjectId =
        serde_json::from_value(serde_json::json!("common-module:M000")).unwrap();
    // Expanding this reference populates the regular lazy-tree descriptor cache.
    session
        .children(&NodeId::Object(priority.clone()), TreeOptions::default())
        .unwrap();
    let reads = session.source_io_stats().xml_reads;
    let first = session.support_page(0).unwrap();
    assert!(
        first
            .objects
            .iter()
            .any(|object| object.object_id == priority)
    );
    assert!(first.next_offset.is_some());
    // Both priority and root were already parsed; only 30 new descriptors need XML reads.
    assert_eq!(session.source_io_stats().xml_reads - reads, 30);
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &session.support_page(0).unwrap()
    ));
    let mut all: Vec<_> = first.objects.iter().map(|o| o.object_id.clone()).collect();
    let mut offset = first.next_offset;
    while let Some(next) = offset {
        let page = session.support_page(next).unwrap();
        all.extend(page.objects.iter().map(|o| o.object_id.clone()));
        offset = page.next_offset;
    }
    assert_eq!(all.len(), 81);
    assert_eq!(
        all.iter().collect::<std::collections::BTreeSet<_>>().len(),
        81
    );
    let mut reopened = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let reopened = reopened.project_mut(&ProjectScope::Standalone).unwrap();
    let reads = reopened.source_io_stats().xml_reads;
    let compact = reopened.support_page(0).unwrap();
    assert!(compact.next_offset.is_none());
    assert_eq!(compact.objects.len(), 81);
    assert_eq!(reopened.source_io_stats().xml_reads, reads);
    let generation = session.generation();
    fs::write(dir.0.join("src/Ext/ParentConfigurations.bin"), rules(0, 0)).unwrap();
    let unchanged = session
        .changed_paths(&["Ext/ParentConfigurations.bin".into()])
        .unwrap();
    assert_eq!(unchanged.generation, generation);
    assert_eq!(unchanged.affected, []);
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &session.support_page(0).unwrap()
    ));
    let root = session.root().clone();
    session.refresh(&root).unwrap();
    assert!(
        session
            .support_page(0)
            .unwrap()
            .objects
            .iter()
            .all(|object| object.state == State::Locked)
    );
    // Manual refresh must detect replacement even without a file event.
    fs::write(dir.0.join("src/Ext/ParentConfigurations.bin"), rules(0, 2)).unwrap();
    session.refresh(&root).unwrap();
    assert!(
        session
            .support_page(0)
            .unwrap()
            .objects
            .iter()
            .all(|object| object.reason == Reason::SupportRemoved)
    );
}

#[test]
fn support_snapshot_survives_restart_and_validates_rules_xml_and_file_inventory() {
    let dir = TestDir::new();
    fixture(&dir.0);
    let rules_path = dir.0.join("src/Ext/ParentConfigurations.bin");
    fs::write(&rules_path, rules(0, 1)).unwrap();
    // A complete successful pass is reusable in an independent workspace session.
    let mut first = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let first = first.project_mut(&ProjectScope::Standalone).unwrap();
    let expected = first.support_page(0).unwrap();
    assert!(expected.next_offset.is_none());
    let mut reopened = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let reopened = reopened.project_mut(&ProjectScope::Standalone).unwrap();
    let reads = reopened.source_io_stats().xml_reads;
    let hits = reopened.disk_cache_stats().unwrap().hits;
    let page = reopened.support_page(0).unwrap();
    assert_eq!(reopened.source_io_stats().xml_reads, reads);
    assert_eq!(reopened.disk_cache_stats().unwrap().hits, hits + 2);
    assert_eq!(page.files.len(), expected.files.len());
    // A damaged derived file is discarded and reconstructed from the original source.
    for entry in fs::read_dir(dir.0.join(".eska/cache/metadata")).unwrap() {
        let path = entry.unwrap().path();
        if fs::read_to_string(&path)
            .unwrap()
            .contains("\"next_offset\"")
        {
            fs::write(path, "damaged cache").unwrap();
        }
    }
    let mut damaged = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let damaged = damaged.project_mut(&ProjectScope::Standalone).unwrap();
    let reads = damaged.source_io_stats().xml_reads;
    assert_eq!(
        damaged.support_page(0).unwrap().files.len(),
        expected.files.len()
    );
    assert!(damaged.source_io_stats().xml_reads > reads);
    // BSL content changes do not affect ownership or support rules.
    fs::write(
        dir.0.join("src/Catalogs/Items/Ext/ObjectModule.bsl"),
        "// edited",
    )
    .unwrap();
    let mut edited = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let edited = edited.project_mut(&ProjectScope::Standalone).unwrap();
    let reads = edited.source_io_stats().xml_reads;
    edited.support_page(0).unwrap();
    assert_eq!(edited.source_io_stats().xml_reads, reads);
    // Large form/template payloads do not own support UUIDs; their presence still matters.
    let payload = dir.0.join("src/Catalogs/Items/Ext/Form.xml");
    fs::write(&payload, "<Form/>").unwrap();
    let mut added = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let added = added.project_mut(&ProjectScope::Standalone).unwrap();
    let reads = added.source_io_stats().xml_reads;
    added.support_page(0).unwrap();
    assert!(added.source_io_stats().xml_reads > reads);
    fs::write(&payload, "<Form>changed payload</Form>").unwrap();
    let mut edited = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let edited = edited.project_mut(&ProjectScope::Standalone).unwrap();
    let reads = edited.source_io_stats().xml_reads;
    edited.support_page(0).unwrap();
    assert_eq!(edited.source_io_stats().xml_reads, reads);
    // Equal-size rule replacement must be detected from bytes, not size or timestamps.
    fs::write(&rules_path, rules(0, 2)).unwrap();
    let mut changed = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let changed = changed.project_mut(&ProjectScope::Standalone).unwrap();
    assert!(
        changed
            .support_page(0)
            .unwrap()
            .objects
            .iter()
            .any(|o| o.reason == Reason::SupportRemoved)
    );
    // Unchanged supplier rules do not prove unchanged UUID ownership.
    let descriptor = dir.0.join("src/Catalogs/Items.xml");
    fs::write(&descriptor, xml("Catalog", "Items", "")).unwrap();
    let mut changed = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let page = changed
        .project_mut(&ProjectScope::Standalone)
        .unwrap()
        .support_page(0)
        .unwrap();
    assert!(!page.objects.iter().any(|object| object.uuid == CHILD));
    fs::remove_file(dir.0.join("src/Catalogs/Items/Ext/ObjectModule.bsl")).unwrap();
    let mut changed = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
    let page = changed
        .project_mut(&ProjectScope::Standalone)
        .unwrap()
        .support_page(0)
        .unwrap();
    assert!(
        !page
            .files
            .iter()
            .any(|file| file.path.extension().is_some_and(|ext| ext == "bsl"))
    );
}

/// An extension edits its own source independently of its base configuration's supplier support.
#[test]
fn extension_without_supplier_file_keeps_native_source_writable_but_honours_explicit_rules() {
    use crate::backend::project::metadata_workspace::SourceAccessRequest;
    let directory = TestDir::new();
    fixture(&directory.0);
    let path = Path::new("Catalogs/Items/Ext/ObjectModule.bsl").to_owned();
    let request = SourceAccessRequest {
        path,
        user_read_only: false,
    };
    let mut configuration = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let protection = configuration
        .project_mut(&ProjectScope::Standalone)
        .unwrap()
        .source_protection(std::slice::from_ref(&request), true)
        .unwrap();
    assert!(protection[0].support.unknown);
    assert!(!protection[0].writable());
    fs::write(
        directory.0.join("eska.toml"),
        "[project]\ntype='extension'\n",
    )
    .unwrap();
    let descriptor = directory.0.join("src/Configuration.xml");
    let xml = fs::read_to_string(&descriptor).unwrap().replace(
        "</Properties>",
        "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose></Properties>",
    );
    fs::write(descriptor, xml).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let protection = project
        .source_protection(std::slice::from_ref(&request), true)
        .unwrap();
    assert!(protection[0].writable(), "{protection:?}");
    let user = SourceAccessRequest {
        user_read_only: true,
        ..request.clone()
    };
    assert!(!project.source_protection(&[user], true).unwrap()[0].writable());
    let policy = directory.0.join("src/Ext/ParentConfigurations.bin");
    fs::write(&policy, rules(0, 0)).unwrap();
    assert!(
        !project
            .source_protection(std::slice::from_ref(&request), true)
            .unwrap()[0]
            .writable()
    );
    fs::write(&policy, "damaged").unwrap();
    let protection = project.source_protection(&[request], true).unwrap();
    assert!(protection[0].support.unknown);
    assert!(!protection[0].writable());
}
