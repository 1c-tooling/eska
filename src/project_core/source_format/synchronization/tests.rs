//! Ancestry is exact source history; tests keep support approval and editable source independent.

use super::*;
use crate::{
    project_core::source_format::{
        AcceptedBase, NativeUuid, Operation, accept_source, import_designer_readable,
    },
    test_support::TestDir,
};

const MODULE: &str = "CommonModules/Tools/Module.bsl";

/// A complete small native configuration contains a module, a binary payload and an empty folder.
pub(super) fn source(test: &TestDir) -> PathBuf {
    let native = test.0.join("native");
    fs::create_dir_all(native.join("CommonModules/Tools/Ext")).unwrap();
    fs::create_dir(native.join("Empty")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        concat!(
            "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'>",
            "<Configuration uuid='10000000-0000-4000-8000-000000000001'><Properties>",
            "<Name>Sample</Name><Version>1.0.0</Version></Properties><ChildObjects>",
            "<CommonModule>Tools</CommonModule></ChildObjects></Configuration></MetaDataObject>"
        ),
    )
    .unwrap();
    fs::write(native.join("CommonModules/Tools.xml"), concat!(
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'>",
        "<CommonModule uuid='20000000-0000-4000-8000-000000000001'>",
        "<Properties><Name>Tools</Name><Server>true</Server></Properties></CommonModule></MetaDataObject>"
    )).unwrap();
    fs::write(
        native.join("CommonModules/Tools/Ext/Module.bsl"),
        b"\xef\xbb\xbf// original\r\n",
    )
    .unwrap();
    fs::write(native.join("Payload.bin"), [0, 255, 10, 13, 0]).unwrap();
    let source = test.0.join("source");
    import_designer_readable(&native, &source).unwrap();
    source
}

/// Restore preserves complete paths, binary bytes, BOM/CRLF and empty folders; repeats are exact.
#[test]
fn captures_and_restores_complete_source_without_support_approval() {
    let test = TestDir::new();
    let source = source(&test);
    let original = Inventory::read(&source).unwrap();
    let store = test.0.join("ancestor");
    let reference = capture_import_ancestor(&source, &store).unwrap();
    assert_eq!(capture_import_ancestor(&source, &store).unwrap(), reference);
    let ancestor = ImportAncestor::open(&store, &reference).unwrap();
    assert_eq!(ancestor.revision(), original.revision);
    assert_eq!(ancestor.reference(), &reference);
    let output = test.0.join("restored");
    ancestor.restore(&output).unwrap();
    ancestor.restore(&output).unwrap();
    assert_eq!(Inventory::read(&output).unwrap(), original);
    assert_eq!(
        fs::read(output.join(MODULE)).unwrap(),
        b"\xef\xbb\xbf// original\r\n"
    );
    assert!(!store.join("accepted.json").exists());
    let marker: serde_json::Value =
        serde_json::from_slice(&fs::read(store.join(MARKER)).unwrap()).unwrap();
    assert!(marker.get("frozen").is_none());
    assert!(marker.get("suppliers").is_none());
    assert_eq!(Inventory::read(&source).unwrap(), original);
}

/// Editing working code never changes the confirmed ancestor or loosens separately approved freeze.
#[test]
fn working_changes_leave_ancestor_and_approved_baseline_intact() {
    let test = TestDir::new();
    let source = source(&test);
    let frozen = NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap();
    let approved = test.0.join("approved");
    let pin = accept_source(&source, &approved, std::slice::from_ref(&frozen)).unwrap();
    let accepted = AcceptedBase::open(&approved, &pin).unwrap();
    let baseline = Inventory::read(&approved).unwrap();
    let store = test.0.join("ancestor");
    let reference = capture_import_ancestor(&source, &store).unwrap();
    let ancestor = ImportAncestor::open(&store, &reference).unwrap();
    fs::write(source.join(MODULE), "// local changes\n").unwrap();
    ancestor.verify().unwrap();
    accepted.verify().unwrap();
    assert!(!accepted.permission(&frozen, Operation::Resource).allowed);
    ancestor.restore(&test.0.join("restored")).unwrap();
    assert_eq!(
        fs::read(test.0.join("restored").join(MODULE)).unwrap(),
        b"\xef\xbb\xbf// original\r\n"
    );
    assert_eq!(
        fs::read(source.join(MODULE)).unwrap(),
        b"// local changes\n"
    );
    assert_eq!(Inventory::read(&approved).unwrap(), baseline);
}

/// A directory cannot advertise its own trust token; a different independent reference is refused.
#[test]
fn requires_the_exact_confirmed_reference() {
    let test = TestDir::new();
    let source = source(&test);
    let store = test.0.join("ancestor");
    capture_import_ancestor(&source, &store).unwrap();
    let wrong = ImportAncestorRef::from_sha256(&"f".repeat(64)).unwrap();
    assert_eq!(
        ImportAncestor::open(&store, &wrong).err().unwrap().code,
        "source_ancestor_untrusted"
    );
    for text in ["", "1234", &"F".repeat(64)] {
        assert!(ImportAncestorRef::from_sha256(text).is_err());
    }
}

/// Missing, modified and extra storage data cannot pass either new reads or an old handle's recheck.
#[test]
fn rejects_modified_missing_and_foreign_storage() {
    for mode in ["modified", "missing", "file", "directory"] {
        let test = TestDir::new();
        let source = source(&test);
        let store = test.0.join("ancestor");
        let reference = capture_import_ancestor(&source, &store).unwrap();
        let ancestor = ImportAncestor::open(&store, &reference).unwrap();
        let blob = fs::read_dir(store.join("blobs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        match mode {
            "modified" => fs::write(blob, "changed").unwrap(),
            "missing" => fs::remove_file(blob).unwrap(),
            "file" => fs::write(store.join("foreign"), "keep").unwrap(),
            _ => fs::create_dir(store.join("foreign")).unwrap(),
        }
        assert!(ancestor.verify().is_err());
        assert!(ImportAncestor::open(&store, &reference).is_err());
        assert!(ancestor.restore(&test.0.join("output")).is_err());
        assert!(!test.0.join("output").exists());
    }
}

/// Foreign output and cancelled verification preserve all original data and produce no new store.
#[test]
fn refuses_foreign_output_overlaps_and_cancelled_reads() {
    let test = TestDir::new();
    let source = source(&test);
    assert!(capture_import_ancestor(&source, &source.join("nested")).is_err());
    assert!(!source.join("nested").exists());
    let store = test.0.join("ancestor");
    let reference = capture_import_ancestor(&source, &store).unwrap();
    let ancestor = ImportAncestor::open(&store, &reference).unwrap();
    let foreign = test.0.join("foreign");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("keep"), "user data").unwrap();
    assert!(ancestor.restore(&foreign).is_err());
    assert!(capture_import_ancestor(&source, &foreign).is_err());
    assert_eq!(
        fs::read_to_string(foreign.join("keep")).unwrap(),
        "user data"
    );
    assert_eq!(
        ImportAncestor::open_cancellable(&store, &reference, &|| true)
            .err()
            .unwrap()
            .code,
        "source_read_cancelled"
    );
    ancestor.verify().unwrap();
}
