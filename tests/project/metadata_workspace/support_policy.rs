use crate::support::TestDir;
use eska::project::{
    metadata_model::ProjectScope,
    metadata_workspace::MetadataWorkspace,
    support::{Reason, State},
};
use std::{fs, path::Path};
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
    assert!(!broken.diagnostics.is_empty());
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
    assert!(!missing.diagnostics.is_empty());
}

#[test]
fn background_reuses_loaded_descriptors_and_refresh_checks_rule_contents() {
    use eska::project::{configurator::TreeOptions, metadata_model::NodeId};
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
    let priority: eska::project::metadata_model::ObjectId =
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
    assert!(unchanged.affected.is_empty());
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
