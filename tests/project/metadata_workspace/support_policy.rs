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
