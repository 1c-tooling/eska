//! Complete created descriptors, managed bodies and opaque resources retain a single working source.

use std::{fs, path::Path};

use super::*;
use crate::project_core::source_format::{
    FileOrigin, InputAuthority, OriginTexts, import_designer_queries,
    queries::test_support::{DIALOG, FORM, LIST, fixture},
};

/// Clone a supplied native subtree with a fresh descriptor identity and unchanged complete payloads.
pub(super) fn clone_owner(native: &Path, previous: &str, next: &str, uuid: &str) {
    let previous = native.join(previous);
    let next = native.join(next);
    let descriptor = fs::read_to_string(previous.with_extension("xml")).unwrap();
    let descriptor = crate::project_core::source_format::Descriptor::from_xml(&descriptor).unwrap();
    let old_uuid = descriptor.object.uuid.as_str();
    let text = fs::read_to_string(previous.with_extension("xml")).unwrap();
    fs::write(
        next.with_extension("xml"),
        text.replace(old_uuid, uuid)
            .replace(">Main<", ">Second<")
            .replace(">Opaque<", ">NewOpaque<"),
    )
    .unwrap();
    let snapshot = Inventory::read(&previous).unwrap();
    fs::create_dir(&next).unwrap();
    for path in snapshot.directories {
        fs::create_dir_all(next.join(path.as_path())).unwrap();
    }
    for path in snapshot.files.keys() {
        fs::copy(previous.join(path.as_path()), next.join(path.as_path())).unwrap();
    }
}

/// Legacy family YAML can hold accepted and created descriptors without confusing their evidence.
#[test]
fn created_nested_descriptor_has_no_accepted_origin_in_a_shared_family_file() {
    let directory = TestDir::new();
    let native = fixture(&directory);
    let working = directory.0.join("source");
    crate::project_core::source_format::import_designer(&native, &working).unwrap();
    let base_path = directory.0.join("accepted");
    let pin = accept_source(&working, &base_path, &[]).unwrap();
    let base = AcceptedBase::open(&base_path, &pin).unwrap();
    let old_path = directory.0.join("ancestor");
    let old_pin = capture_import_ancestor(&working, &old_path).unwrap();
    let old = ImportAncestor::open(&old_path, &old_pin).unwrap();
    clone_owner(
        &native,
        "Catalogs/Sample/Forms/Main",
        "Catalogs/Sample/Forms/Second",
        "90000000-0000-4000-8000-000000000001",
    );
    let path = native.join("Catalogs/Sample.xml");
    let mut text = fs::read_to_string(&path).unwrap();
    let index = text.rfind("</ChildObjects>").unwrap();
    text.insert_str(index, "<Form>Second</Form>");
    fs::write(path, text).unwrap();
    let incoming_source = directory.0.join("incoming-source");
    crate::project_core::source_format::import_designer(&native, &incoming_source).unwrap();
    let incoming_path = directory.0.join("incoming");
    let incoming_pin = capture_import_ancestor(&incoming_source, &incoming_path).unwrap();
    let incoming = ImportAncestor::open(&incoming_path, &incoming_pin).unwrap();
    let plan = prepare_source_import(&working, &old, &incoming, &base).unwrap();
    assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
    let output = directory.0.join("output");
    plan.publish_source(&output).unwrap();
    let prepared =
        prepare_protected(&output, &base, &directory.0.join("generations"), true).unwrap();
    let mapped = prepared
        .source_map()
        .file(&SourcePath::parse("Catalogs/Sample/Forms/Second.xml").unwrap())
        .unwrap();
    let FileOrigin::CreatedMetadata { input, .. } = &mapped.origin else {
        panic!("working-only nested origin");
    };
    assert_eq!(input.path.as_str(), "Catalogs/Sample/object.yaml");
    assert_eq!(prepared.source_map().schema_version, 7);
    let native = fs::read(prepared.xml().join(mapped.path.as_path())).unwrap();
    let text = fs::read_to_string(output.join(input.path.as_path())).unwrap();
    let start = String::from_utf8_lossy(&native).find("Second").unwrap();
    let location = mapped
        .locate(
            &native,
            start..start + 6,
            Some(OriginTexts {
                accepted: "",
                working: Some(&text),
            }),
        )
        .unwrap();
    assert_eq!(location.authority, InputAuthority::Working);
    assert!(text[location.range.unwrap()].contains("Second"));
}

/// A new form's plain query and binary XML-named template survive protected generation and repeat.
#[test]
fn created_form_and_binary_template_keep_working_only_origins() {
    let directory = TestDir::new();
    let native = fixture(&directory);
    let working = directory.0.join("source");
    import_designer_queries(&native, &working).unwrap();
    let base_path = directory.0.join("accepted");
    let pin = accept_source(&working, &base_path, &[]).unwrap();
    let base = AcceptedBase::open(&base_path, &pin).unwrap();
    let old_path = directory.0.join("ancestor");
    let old_pin = capture_import_ancestor(&working, &old_path).unwrap();
    let old = ImportAncestor::open(&old_path, &old_pin).unwrap();
    clone_owner(
        &native,
        "Catalogs/Sample/Forms/Main",
        "Catalogs/Sample/Forms/Second",
        "90000000-0000-4000-8000-000000000001",
    );
    clone_owner(
        &native,
        "Catalogs/Sample/Templates/Opaque",
        "Catalogs/Sample/Templates/NewOpaque",
        "90000000-0000-4000-8000-000000000002",
    );
    let path = native.join("Catalogs/Sample.xml");
    let mut text = fs::read_to_string(&path).unwrap();
    let index = text.rfind("</ChildObjects>").unwrap();
    text.insert_str(index, "<Form>Second</Form><Template>NewOpaque</Template>");
    fs::write(path, text).unwrap();
    let incoming_source = directory.0.join("incoming-source");
    import_designer_queries(&native, &incoming_source).unwrap();
    let incoming_path = directory.0.join("incoming");
    let incoming_pin = capture_import_ancestor(&incoming_source, &incoming_path).unwrap();
    let incoming = ImportAncestor::open(&incoming_path, &incoming_pin).unwrap();
    fs::write(working.join(LIST), "SELECT 'local'\r\n").unwrap();
    let plan = prepare_source_import(&working, &old, &incoming, &base).unwrap();
    assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
    let output = directory.0.join("output");
    plan.publish_source(&output).unwrap();
    assert_eq!(
        fs::read_to_string(output.join(LIST)).unwrap(),
        "SELECT 'local'\r\n"
    );
    let prepared =
        prepare_protected(&output, &base, &directory.0.join("generations"), true).unwrap();
    let map = prepared.source_map();
    assert_eq!(map.schema_version, 7);
    let path = SourcePath::parse(&DIALOG.replace("/Main/", "/Second/")).unwrap();
    let mapped = map.file(&path).unwrap();
    assert!(
        matches!(&mapped.origin, FileOrigin::CreatedManagedForm { queries, .. } if queries.len() == 2)
    );
    let native = fs::read(prepared.xml().join(path.as_path())).unwrap();
    let offset = String::from_utf8_lossy(&native).find("SELECT").unwrap();
    let location = mapped.locate(&native, offset..offset + 6, None).unwrap();
    assert_eq!(location.authority, InputAuthority::Working);
    assert_eq!(
        location.input.path.as_str(),
        LIST.replace("/Main/", "/Second/")
    );
    let source = fs::read_to_string(output.join(FORM.replace("/Main/", "/Second/"))).unwrap();
    let offset = String::from_utf8_lossy(&native).find("Grid").unwrap();
    let location = mapped
        .locate(
            &native,
            offset..offset + 4,
            Some(OriginTexts {
                accepted: "",
                working: Some(&source),
            }),
        )
        .unwrap();
    assert_eq!(location.authority, InputAuthority::Working);
    assert!(source[location.range.unwrap()].contains("Grid"));
    assert_eq!(
        fs::read(
            prepared
                .xml()
                .join("Catalogs/Sample/Templates/NewOpaque/Ext/Template.xml")
        )
        .unwrap(),
        [0, 255, 10, 13]
    );
    let repeat = prepare_source_import(&output, &incoming, &incoming, &base).unwrap();
    assert!(repeat.conflicts().is_empty(), "{:?}", repeat.conflicts());
    assert_eq!(repeat.working_revision(), repeat.output_revision());
}
