//! These fixtures distinguish accepted permissions, independent input authorities and exact byte coordinates.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::*;
use crate::{
    project_core::source_format::{
        FileOrigin, FormPropertyName, FormPropertyValue, InputAuthority, NativeQueryDocument,
        NativeUuid, ObjectSource, OriginTexts, PermissionReason, PreparedSource,
        QUERY_SOURCE_MAP_VERSION, QuerySlot, SourceMap, accept_source, files::Inventory,
        import_designer_queries, prepare_protected, queries::test_support::*,
    },
    test_support::TestDir,
};

/// Native metadata identities remain unrelated to overlapping decimal UI and Attribute IDs.
fn uuid(prefix: char) -> NativeUuid {
    NativeUuid::parse(&format!("{prefix}0000000-0000-4000-8000-000000000001")).unwrap()
}

/// The optional native policy uses opaque test CF bytes and locks the form's accepted metadata identity.
fn accepted(directory: &TestDir, frozen: &[NativeUuid], locked: bool) -> (PathBuf, AcceptedBase) {
    let native = fixture(directory);
    fs::write(
        native.join(DIALOG),
        form().replace("</Form>", "<ReadOnly>false</ReadOnly></Form>"),
    )
    .unwrap();
    if locked {
        let original = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/eska-format/designer/Ext/ParentConfigurations.bin");
        let text = fs::read_to_string(original).unwrap().replace(
            "\"SampleConfiguration\",3,",
            &format!("\"SampleConfiguration\",4,0,0,{0},{0},", uuid('7').as_str()),
        );
        fs::write(native.join("Ext/ParentConfigurations.bin"), text).unwrap();
        fs::create_dir_all(native.join("Ext/ParentConfigurations")).unwrap();
        fs::write(
            native.join("Ext/ParentConfigurations/SampleConfiguration.cf"),
            b"synthetic opaque supplier CF",
        )
        .unwrap();
    }
    let root = directory.0.join("source");
    import_designer_queries(&native, &root).unwrap();
    let reference = accept_source(&root, &directory.0.join("accepted"), frozen).unwrap();
    let base = AcceptedBase::open(&directory.0.join("accepted"), &reference).unwrap();
    (root, base)
}

/// Every addressed native span is evaluated against the same immutable prepared file fingerprint.
fn plain_span(
    prepared: &PreparedSource,
    native_path: &str,
    needle: &str,
) -> crate::project_core::source_format::SourceLocation {
    let bytes = fs::read(prepared.xml().join(native_path)).unwrap();
    let native = std::str::from_utf8(&bytes).unwrap();
    let start = native.find(needle).unwrap();
    prepared
        .source_map()
        .file(&path(native_path))
        .unwrap()
        .locate(&bytes, start..start + needle.len(), None)
        .unwrap()
}

/// Each resource and plain value can select a different input while retaining exact native scopes.
#[test]
fn allowed_query_edits_have_composite_origins_and_exact_entity_unicode_and_cr_coordinates() {
    let directory = TestDir::new();
    let (root, base) = accepted(&directory, &[], false);
    let text = "SELECT \"<&>\";\r\nВЫБРАТЬ 1";
    fs::write(root.join(QUERY), text).unwrap();
    fs::write(root.join(LIST), "SELECT 'list';\n// список").unwrap();
    let resource = fs::read_to_string(root.join(RESOURCE)).unwrap();
    fs::write(
        root.join(RESOURCE),
        resource.replace("<settingsVariant/>", "<!--tail--><settingsVariant/>"),
    )
    .unwrap();
    let before = Inventory::read(&root).unwrap();
    let prepared = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    assert!(prepared.diagnostics().is_empty());
    prepared.validate().unwrap();
    prepared.source_map().validate().unwrap();
    assert_eq!(
        prepared.source_map().schema_version,
        QUERY_SOURCE_MAP_VERSION
    );
    let encoded = serde_json::to_string(prepared.source_map()).unwrap();
    assert_eq!(
        serde_json::from_str::<SourceMap>(&encoded).unwrap(),
        *prepared.source_map()
    );
    for (needle, expected) in [
        ("&lt;", "<"),
        ("&amp;", "&"),
        ("&#xD;", "\r"),
        ("ВЫБРАТЬ", "ВЫБРАТЬ"),
    ] {
        let location = plain_span(&prepared, TEMPLATE, needle);
        assert_eq!(location.authority, InputAuthority::Working);
        assert_eq!(location.input.path, path(QUERY));
        assert_eq!(location.uuid, Some(uuid('8')));
        assert_eq!(&text[location.range.unwrap()], expected);
    }
    let location = plain_span(&prepared, DIALOG, "// список");
    assert_eq!(location.input.path, path(LIST));
    assert_eq!(location.uuid, Some(uuid('7')));
    assert_eq!(location.authority, InputAuthority::Working);
    assert_eq!(
        &fs::read_to_string(root.join(LIST)).unwrap()[location.range.unwrap()],
        "// список"
    );
    let bytes = fs::read(prepared.xml().join(TEMPLATE)).unwrap();
    let native = std::str::from_utf8(&bytes).unwrap();
    let start = native.find("<!--tail-->").unwrap();
    let working = fs::read_to_string(root.join(RESOURCE)).unwrap();
    let location = prepared
        .source_map()
        .file(&path(TEMPLATE))
        .unwrap()
        .locate(
            &bytes,
            start..start + "<!--tail-->".len(),
            Some(OriginTexts {
                accepted: &resource,
                working: Some(&working),
            }),
        )
        .unwrap();
    assert_eq!(location.input.path, path(RESOURCE));
    assert_eq!(&working[location.range.unwrap()], "<!--tail-->");
    let repeated = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    assert_eq!(repeated.generation(), prepared.generation());
    assert_eq!(Inventory::read(&root).unwrap(), before);
}

/// Restored form bindings do not erase independently permitted body properties or plain query values.
#[test]
fn form_query_values_preserve_property_coordinates_after_queries_and_restore_spoofed_bindings() {
    let directory = TestDir::new();
    let (root, base) = accepted(&directory, &[], false);
    let original = fs::read_to_string(root.join(FORM)).unwrap();
    let mut object = ObjectSource::from_yaml(&original).unwrap();
    *object
        .form
        .as_mut()
        .unwrap()
        .properties
        .get_mut(&FormPropertyName::ReadOnly)
        .unwrap() = FormPropertyValue::Bool(true);
    let working = object
        .to_yaml()
        .unwrap()
        .replace("urn:eska:query:1", "urn:eska:query:99")
        .replace(uuid('7').as_str(), uuid('2').as_str());
    fs::write(root.join(FORM), &working).unwrap();
    fs::write(root.join(LIST), "SELECT \"<new>\";\n// longer query value").unwrap();
    let prepared =
        prepare_protected(&root, &base, &directory.0.join("generations"), false).unwrap();
    assert!(!prepared.diagnostics().is_empty());
    let location = plain_span(&prepared, DIALOG, "&lt;new&gt;");
    assert_eq!(location.input.path, path(LIST));
    assert_eq!(location.uuid, Some(uuid('7')));
    let bytes = fs::read(prepared.xml().join(DIALOG)).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    let start = text.find("<ReadOnly>true</ReadOnly>").unwrap() + "<ReadOnly>".len();
    let location = prepared
        .source_map()
        .file(&path(DIALOG))
        .unwrap()
        .locate(
            &bytes,
            start..start + 4,
            Some(OriginTexts {
                accepted: &original,
                working: Some(&working),
            }),
        )
        .unwrap();
    assert_eq!(location.input.path, path(FORM));
    assert_eq!(location.authority, InputAuthority::Working);
    assert_eq!(&working[location.range.unwrap()], "true");
    let saved = fs::read(prepared.xml().join(DIALOG)).unwrap();
    assert!(prepare_protected(&root, &base, &directory.0.join("generations"), true).is_err());
    assert_eq!(fs::read(prepared.xml().join(DIALOG)).unwrap(), saved);
}

/// Freeze of a metadata ancestor and native support restrictions retain Resource semantics for plain files.
#[test]
fn frozen_and_native_locked_queries_restore_denied_bytes_without_trusting_working_manifest() {
    for locked in [false, true] {
        let directory = TestDir::new();
        let frozen = if locked { vec![] } else { vec![uuid('2')] };
        let (root, base) = accepted(&directory, &frozen, locked);
        fs::write(root.join(LIST), b"\xff invalid denied text").unwrap();
        if !locked {
            fs::remove_file(root.join(QUERY)).unwrap();
        }
        fs::write(root.join("source.json"), "{}").unwrap();
        let prepared =
            prepare_protected(&root, &base, &directory.0.join("generations"), false).unwrap();
        let list = prepared
            .diagnostics()
            .iter()
            .find(|entry| entry.path == path(LIST))
            .unwrap();
        assert_eq!(list.operation, Operation::Resource);
        assert_eq!(list.uuid, Some(uuid('7')));
        assert_eq!(
            list.reason,
            if locked {
                PermissionReason::NativeRestriction
            } else {
                PermissionReason::EskaFrozen
            }
        );
        let expected = base.read_file(&path(LIST)).unwrap();
        let actual = NativeQueryDocument::from_xml(
            &fs::read_to_string(prepared.xml().join(DIALOG)).unwrap(),
        )
        .unwrap();
        assert_eq!(actual.queries()[0].text, expected);
        let location = plain_span(&prepared, DIALOG, "// list");
        assert_eq!(location.authority, InputAuthority::Accepted);
        assert_eq!(location.input.path, path(LIST));
        if !locked {
            let missing = prepared
                .diagnostics()
                .iter()
                .find(|entry| entry.path == path(QUERY))
                .unwrap();
            assert_eq!(missing.operation, Operation::Resource);
            assert_eq!(missing.reason, PermissionReason::EskaFrozen);
            assert_eq!(
                plain_span(&prepared, TEMPLATE, "SELECT 2").authority,
                InputAuthority::Accepted
            );
        }
        assert!(prepare_protected(&root, &base, &directory.0.join("generations"), true).is_err());
        prepared.validate().unwrap();
    }
}

/// Missing permitted query files refuse before publication and never substitute an accepted inline value.
#[test]
fn missing_allowed_query_value_leaves_previous_generation_and_working_bytes_unchanged() {
    let directory = TestDir::new();
    let (root, base) = accepted(&directory, &[], false);
    let prepared = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    let output = Inventory::read(prepared.xml()).unwrap();
    fs::remove_file(root.join(QUERY)).unwrap();
    let before = Inventory::read(&root).unwrap();
    for strict in [false, true] {
        let failure =
            prepare_protected(&root, &base, &directory.0.join("generations"), strict).unwrap_err();
        assert_eq!(failure.error.code, "source_query_value_missing");
        assert_eq!(Inventory::read(prepared.xml()).unwrap(), output);
        assert_eq!(Inventory::read(&root).unwrap(), before);
    }
    prepared.validate().unwrap();
}

/// A complete declared set never permits a foreign reference, inline fallback or unsupported protocol version.
#[test]
fn invalid_allowed_query_reference_or_xml_characters_refuse_without_replacing_good_output() {
    let directory = TestDir::new();
    let (root, base) = accepted(&directory, &[], false);
    let prepared = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    let saved = Inventory::read(prepared.xml()).unwrap();
    let original = fs::read_to_string(root.join(RESOURCE)).unwrap();
    for malformed in [
        original.replace("urn:eska:query:1", "urn:eska:query:99"),
        original.replace("Query_Same.sdbl", "Query_Foreign.sdbl"),
        schema(),
    ] {
        fs::write(root.join(RESOURCE), malformed).unwrap();
        assert!(prepare_protected(&root, &base, &directory.0.join("generations"), false).is_err());
        assert_eq!(Inventory::read(prepared.xml()).unwrap(), saved);
    }
    fs::write(root.join(RESOURCE), original).unwrap();
    fs::write(root.join(QUERY), "SELECT \u{1}").unwrap();
    assert!(prepare_protected(&root, &base, &directory.0.join("generations"), false).is_err());
    assert_eq!(Inventory::read(prepared.xml()).unwrap(), saved);
}

/// DTO validity cannot authorize a different native owner or manufacture coordinates from stale values.
#[test]
fn query_origin_forgery_and_stale_fingerprints_are_rejected() {
    let directory = TestDir::new();
    let (root, base) = accepted(&directory, &[], false);
    fs::write(root.join(QUERY), "ВЫБРАТЬ 1").unwrap();
    let prepared = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    let file = prepared.source_map().file(&path(TEMPLATE)).unwrap();
    let bytes = fs::read(prepared.xml().join(TEMPLATE)).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    let start = text.find("ВЫБРАТЬ").unwrap();
    assert!(file.locate(&bytes, start + 1..start + 2, None).is_err());
    for mutation in 0..4 {
        let mut map = prepared.source_map().clone();
        if mutation == 0 {
            map.schema_version = 3;
        }
        let file = map
            .files
            .iter_mut()
            .find(|file| file.path == path(TEMPLATE))
            .unwrap();
        let FileOrigin::QueryResource {
            uuid: owner,
            descriptor,
            queries,
            ..
        } = &mut file.origin
        else {
            panic!("query origin missing")
        };
        match mutation {
            1 => *owner = uuid('7'),
            2 => *descriptor = path("Catalogs/Sample/Forms/Main.xml"),
            3 => queries[1].input.path = queries[0].input.path.clone(),
            _ => {}
        }
        assert!(map.validate().is_err());
    }
    for mutation in 0..3 {
        let mut file = file.clone();
        let FileOrigin::QueryResource { queries, .. } = &mut file.origin else {
            panic!("query origin missing")
        };
        match mutation {
            0 => queries[1].input.sha256 = "f".repeat(64),
            1 => queries[1].native.start += 1,
            2 => {
                queries[1].slot = QuerySlot::Composition {
                    datasets: vec!["Other".into()],
                }
            }
            _ => unreachable!(),
        }
        assert!(
            file.locate(&bytes, start..start + "ВЫБРАТЬ".len(), None)
                .is_err()
        );
    }
}

/// Extracting a query cannot hide an accepted field dependency from the existing structural writer guard.
#[test]
fn separated_query_values_retain_conservative_metadata_rename_protection() {
    let directory = TestDir::new();
    let native = fixture(&directory);
    let catalog = native.join("Catalogs/Sample.xml");
    let original = fs::read_to_string(&catalog).unwrap();
    fs::write(&catalog, original.replace("version=\"2.20\"", "xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"").replace(
        "<Comment>Editable part</Comment>",
        "<Synonym/><Comment>Editable part</Comment><Type><v8:Type>xs:boolean</v8:Type></Type>",
    )).unwrap();
    fs::write(
        native.join(TEMPLATE),
        schema().replace("SELECT 2;", "SELECT Description;"),
    )
    .unwrap();
    let root = directory.0.join("source");
    import_designer_queries(&native, &root).unwrap();
    let reference = accept_source(&root, &directory.0.join("accepted"), &[]).unwrap();
    let base = AcceptedBase::open(&directory.0.join("accepted"), &reference).unwrap();
    let owner = root.join("Catalogs/Sample/object.yaml");
    let text = fs::read_to_string(&owner).unwrap();
    fs::write(owner, text.replace("Description", "ChangedDescription")).unwrap();
    let failure =
        prepare_protected(&root, &base, &directory.0.join("generations"), false).unwrap_err();
    assert_eq!(failure.error.code, "source_structural_reference_preserved");
    assert!(!directory.0.join("generations").exists());
}
