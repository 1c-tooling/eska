//! These owned fixtures separate policy, selected values and captured authoring coordinates.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::*;
use crate::{
    project_core::source_format::{
        FileOrigin, FormId, InputAuthority, MANAGED_FORM_SOURCE_MAP_VERSION, OriginTexts,
        PermissionReason, SourceMap, SourcePath, accept_source, export_designer, files::Inventory,
        import_designer_forms, prepare_protected, yaml,
    },
    test_support::TestDir,
};

const OWNER: &str = "Catalogs/Sample/Forms/Main/form.yaml";
const DESCRIPTOR: &str = "Catalogs/Sample/Forms/Main.xml";
const DIALOG: &str = "Catalogs/Sample/Forms/Main/Ext/Form.xml";
const XML: &str = "\u{feff}<Form xmlns=\"http://v8.1c.ru/8.3/xcf/logform\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" version=\"2.20\">\r\n<AutoCommandBar name=\"\" id=\"-1\"/><Title><v8:item><v8:lang>ru</v8:lang><v8:content>Форма</v8:content></v8:item><v8:item><v8:lang>en</v8:lang><v8:content>Form</v8:content></v8:item></Title><ChildItems><UsualGroup name=\"First\" id=\"1\"><ChildItems><InputField name=\"Value\" id=\"3\"><Title formatted=\"true\"><v8:item><v8:lang>ru</v8:lang><v8:content>Поле</v8:content></v8:item></Title></InputField></ChildItems></UsualGroup><UsualGroup name=\"Second\" id=\"2\"><ChildItems/></UsualGroup></ChildItems><ReadOnly>true</ReadOnly>\r\n</Form>\r\n";

/// Native UUID is unrelated to decimal UI IDs and persists through every candidate corruption.
fn uuid(prefix: char) -> NativeUuid {
    NativeUuid::parse(&format!("{prefix}0000000-0000-4000-8000-000000000001")).unwrap()
}

/// Every fixture is an owned playground subtree; supplier CF bytes are only opaque test sentinels.
fn fixture(directory: &TestDir, frozen: &[NativeUuid], locked: bool) -> (PathBuf, AcceptedBase) {
    let native = directory.0.join("native");
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    fs::create_dir(&native).unwrap();
    let inventory = Inventory::read(&original).unwrap();
    for path in inventory.directories {
        fs::create_dir_all(native.join(path.as_path())).unwrap();
    }
    for path in inventory.files.keys() {
        fs::copy(original.join(path.as_path()), native.join(path.as_path())).unwrap();
    }
    fs::write(native.join(DIALOG), XML).unwrap();
    fs::create_dir_all(native.join("Catalogs/Sample/Forms/Main/Ext/Form")).unwrap();
    fs::copy(
        native.join("CommonModules/Tools/Ext/Module.bsl"),
        native.join("Catalogs/Sample/Forms/Main/Ext/Form/Module.bsl"),
    )
    .unwrap();
    fs::create_dir_all(native.join("Ext/ParentConfigurations")).unwrap();
    fs::write(
        native.join("Ext/ParentConfigurations/SampleConfiguration.cf"),
        b"synthetic opaque CF sentinel",
    )
    .unwrap();
    if locked {
        let path = native.join("Ext/ParentConfigurations.bin");
        let text = fs::read_to_string(&path).unwrap().replace(
            "\"SampleConfiguration\",3,",
            &format!("\"SampleConfiguration\",4,0,0,{0},{0},", uuid('7').as_str()),
        );
        fs::write(path, text).unwrap();
    }
    let root = directory.0.join("source");
    import_designer_forms(&native, &root).unwrap();
    let accepted = directory.0.join("accepted");
    let reference = accept_source(&root, &accepted, frozen).unwrap();
    (root, AcceptedBase::open(&accepted, &reference).unwrap())
}

/// Raw JSON-shaped edits exercise restoration independently of source-model constructor validation.
fn edit(root: &Path, change: impl FnOnce(&mut Value)) -> String {
    let text = fs::read_to_string(root.join(OWNER)).unwrap();
    let mut value = yaml::decode(&text).unwrap();
    change(&mut value);
    let text = yaml::encode(&value).unwrap();
    fs::write(root.join(OWNER), &text).unwrap();
    text
}

/// Assert the exact native span and the corresponding scalar in byte-verified source text.
fn mapped_value(
    prepared: &super::super::PreparedSource,
    accepted: &str,
    working: &str,
    needle: &str,
) -> (InputAuthority, String) {
    let path = SourcePath::parse(DIALOG).unwrap();
    let native = fs::read(prepared.xml().join(DIALOG)).unwrap();
    let text = std::str::from_utf8(&native).unwrap();
    let start = text.find(needle).unwrap();
    let location = prepared
        .source_map()
        .file(&path)
        .unwrap()
        .locate(
            &native,
            start..start + needle.len(),
            Some(OriginTexts {
                accepted,
                working: Some(working),
            }),
        )
        .unwrap();
    assert_eq!(location.uuid, Some(uuid('7')));
    assert_eq!(location.input.path.as_str(), OWNER);
    let source = if location.authority == InputAuthority::Accepted {
        accepted
    } else {
        working
    };
    (location.authority, source[location.range.unwrap()].into())
}

/// Metadata and dialog share exact input bytes and UUID while their selected-value flags stay independent.
#[test]
fn no_op_protected_form_matches_control_and_has_a_valid_composite_map() {
    let directory = TestDir::new();
    let (root, base) = fixture(&directory, &[], false);
    let before = Inventory::read(&root).unwrap();
    let control = directory.0.join("control");
    export_designer(&root, &control).unwrap();
    let prepared = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    assert!(prepared.diagnostics().is_empty());
    assert_eq!(
        Inventory::read(prepared.xml()).unwrap(),
        Inventory::read(&control).unwrap()
    );
    assert_eq!(
        fs::read_to_string(prepared.xml().join(DIALOG)).unwrap(),
        XML
    );
    assert_eq!(
        prepared.source_map().schema_version,
        MANAGED_FORM_SOURCE_MAP_VERSION
    );
    assert_eq!(prepared.source_map().source_schema_version, 2);
    assert!(
        matches!(&prepared.source_map().file(&SourcePath::parse(DIALOG).unwrap()).unwrap().origin,
        FileOrigin::ManagedForm { uuid: owner, descriptor, uses_working_values: false, .. }
        if *owner == uuid('7') && descriptor.as_str() == DESCRIPTOR)
    );
    prepared.validate().unwrap();
    prepared.source_map().validate().unwrap();
    let map: SourceMap =
        serde_json::from_str(&serde_json::to_string(prepared.source_map()).unwrap()).unwrap();
    assert_eq!(
        map.checksum().unwrap(),
        prepared.source_map().checksum().unwrap()
    );
    let repeated = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    assert_eq!(prepared.generation(), repeated.generation());
    assert_eq!(Inventory::read(&root).unwrap(), before);
}

/// Changed texts/flags and a move into an existing empty slot map to their sole working YAML values.
#[test]
fn selected_properties_and_layout_have_exact_working_coordinates() {
    let directory = TestDir::new();
    let (root, base) = fixture(&directory, &[], false);
    let accepted = fs::read_to_string(root.join(OWNER)).unwrap();
    let working = edit(&root, |value| {
        value["document"]["descriptor"]["object"]["properties"]["Comment"] =
            Value::String("Changed metadata".into());
        value["form"]["properties"]["ReadOnly"] = Value::Bool(false);
        value["form"]["properties"]["Title"]["text"]["ru"] = Value::String("Новая 😀 форма".into());
        value["form"]["elements"]["3"]["properties"]["Title"]["formatted"] = Value::Bool(false);
        value["form"]["layout"]["roots"] = serde_json::json!(["2", "1"]);
        value["form"]["layout"]["children"] = serde_json::json!({"1": [], "2": ["3"]});
    });
    let before = Inventory::read(&root).unwrap();
    let prepared = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    assert!(prepared.diagnostics().is_empty());
    let native = fs::read_to_string(prepared.xml().join(DIALOG)).unwrap();
    let body = ManagedFormSource::from_xml(&native).unwrap();
    assert_eq!(
        body.layout.children[&FormId::parse("2").unwrap()],
        [FormId::parse("3").unwrap()]
    );
    assert_eq!(
        mapped_value(&prepared, &accepted, &working, "Новая 😀 форма"),
        (InputAuthority::Working, "\"Новая 😀 форма\"".into())
    );
    assert_eq!(
        mapped_value(&prepared, &accepted, &working, "<ReadOnly>false</ReadOnly>"),
        (InputAuthority::Working, "false".into())
    );
    assert_eq!(
        mapped_value(&prepared, &accepted, &working, "formatted=\"false\""),
        (InputAuthority::Working, "false".into())
    );
    assert_eq!(
        mapped_value(&prepared, &accepted, &working, "Form</v8:content>"),
        (InputAuthority::Accepted, "\"Form\"".into())
    );
    assert_eq!(
        mapped_value(&prepared, &accepted, &working, "<InputField"),
        (InputAuthority::Working, "\"3\"".into())
    );
    assert_eq!(Inventory::read(&root).unwrap(), before);
}

/// Native stencils, UI names, unaccepted slots/languages and metadata identity never grant extra rights.
#[test]
fn prohibited_bindings_are_restored_without_discarding_independent_values() {
    let directory = TestDir::new();
    let (root, base) = fixture(&directory, &[], false);
    let accepted = fs::read_to_string(root.join(OWNER)).unwrap();
    let working = edit(&root, |value| {
        value["form"]["native"] = serde_json::json!({"forged": true});
        value["form"]["schema_version"] = Value::from(99);
        value["form"]["elements"]["3"]["name"] = Value::String("Forged identity".into());
        value["form"]["properties"]["Title"]["text"]["zz"] = Value::String("New language".into());
        value["form"]["properties"]["Visible"] = Value::Bool(true);
        value["form"]["properties"]["ReadOnly"] = Value::Bool(false);
        value["form"]["properties"]["Title"]["text"]["ru"] =
            Value::String("Разрешённая правка".into());
        value["document"]["descriptor"]["object"]["properties"]["Comment"] =
            Value::String("Independent metadata edit".into());
    });
    let before = Inventory::read(&root).unwrap();
    let output = directory.0.join("generations");
    let failure = prepare_protected(&root, &base, &output, true).unwrap_err();
    assert_eq!(failure.error.code, "source_changes_excluded");
    assert!(!output.exists());
    assert!(
        failure
            .diagnostics
            .iter()
            .filter(|entry| entry.coordinate.first().is_some_and(|key| key == "form"))
            .all(|entry| entry.uuid == Some(uuid('7')) && entry.operation == Operation::Resource)
    );
    let prepared = prepare_protected(&root, &base, &output, false).unwrap();
    let native = fs::read_to_string(prepared.xml().join(DIALOG)).unwrap();
    assert!(native.contains("Разрешённая правка"));
    assert!(native.contains("name=\"Value\""));
    assert!(!native.contains("New language"));
    assert!(!native.contains("Visible"));
    assert_eq!(
        mapped_value(&prepared, &accepted, &working, "Разрешённая правка"),
        (InputAuthority::Working, "\"Разрешённая правка\"".into())
    );
    let bytes = fs::read(prepared.xml().join(DESCRIPTOR)).unwrap();
    let offset = std::str::from_utf8(&bytes)
        .unwrap()
        .find("Independent metadata edit")
        .unwrap();
    let location = prepared
        .source_map()
        .file(&SourcePath::parse(DESCRIPTOR).unwrap())
        .unwrap()
        .locate(
            &bytes,
            offset..offset + 1,
            Some(OriginTexts {
                accepted: &accepted,
                working: Some(&working),
            }),
        )
        .unwrap();
    assert_eq!(location.authority, InputAuthority::Working);
    assert!(working[location.range.unwrap()].contains("Independent metadata edit"));
    assert_eq!(Inventory::read(&root).unwrap(), before);
}

/// Exact or inherited freeze restores even malformed candidate values without reading them as permissions.
#[test]
fn exact_and_inherited_freeze_restore_form_content_before_validation() {
    for prefix in ['7', '2', '1'] {
        let directory = TestDir::new();
        let (root, base) = fixture(&directory, &[uuid(prefix)], false);
        edit(&root, |value| {
            value["form"] = serde_json::json!({"properties":{"ReadOnly":"invalid"}, "layout":null});
        });
        let prepared =
            prepare_protected(&root, &base, &directory.0.join("generations"), false).unwrap();
        assert_eq!(
            fs::read_to_string(prepared.xml().join(DIALOG)).unwrap(),
            XML
        );
        assert!(
            prepared
                .diagnostics()
                .iter()
                .any(|entry| entry.uuid == Some(uuid('7'))
                    && entry.reason == PermissionReason::EskaFrozen)
        );
    }
}

/// Native supplier restrictions are pinned; editing the displayed UUID cannot change the body owner.
#[test]
fn native_locked_form_uses_accepted_identity_despite_working_uuid_substitution() {
    let directory = TestDir::new();
    let (root, base) = fixture(&directory, &[], true);
    assert_eq!(
        base.permission(&uuid('7'), Operation::Resource).reason,
        PermissionReason::NativeRestriction
    );
    edit(&root, |value| {
        value["document"]["descriptor"]["object"]["uuid"] =
            Value::String(uuid('2').as_str().into());
        value["form"]["properties"]["ReadOnly"] = Value::Bool(false);
    });
    let prepared =
        prepare_protected(&root, &base, &directory.0.join("generations"), false).unwrap();
    assert_eq!(
        fs::read_to_string(prepared.xml().join(DIALOG)).unwrap(),
        XML
    );
    assert!(
        prepared
            .diagnostics()
            .iter()
            .any(|entry| entry.uuid == Some(uuid('7'))
                && entry.reason == PermissionReason::NativeRestriction)
    );
}

/// A malformed permitted value or graph cannot produce a generation, rather than being silently coerced.
#[test]
fn invalid_permitted_properties_and_layout_refuse_publication() {
    for invalid in [
        serde_json::json!("invalid"),
        serde_json::json!(["1", "1"]),
        serde_json::json!(["999"]),
    ] {
        let directory = TestDir::new();
        let (root, base) = fixture(&directory, &[], false);
        edit(&root, |value| {
            if invalid.is_string() {
                value["form"]["properties"]["ReadOnly"] = invalid;
            } else {
                value["form"]["layout"]["roots"] = invalid;
            }
        });
        let output = directory.0.join("generations");
        assert!(prepare_protected(&root, &base, &output, false).is_err());
        assert!(!output.exists());
    }
}

/// Missing body/file restores accepted bytes and leaves an explicit diagnostic instead of treating absence as empty.
#[test]
fn missing_working_body_or_owner_is_restored_with_captured_origins() {
    for missing_file in [false, true] {
        let directory = TestDir::new();
        let (root, base) = fixture(&directory, &[], false);
        if missing_file {
            fs::remove_file(root.join(OWNER)).unwrap();
        } else {
            edit(&root, |value| {
                value.as_object_mut().unwrap().remove("form");
            });
        }
        let prepared =
            prepare_protected(&root, &base, &directory.0.join("generations"), false).unwrap();
        assert_eq!(
            fs::read_to_string(prepared.xml().join(DIALOG)).unwrap(),
            XML
        );
        assert!(!prepared.diagnostics().is_empty());
        prepared.source_map().validate().unwrap();
    }
}

/// A dialog origin must link its actual metadata descriptor and exact shared captured authoring inputs.
#[test]
fn forged_dialog_origin_version_owner_or_shared_input_is_rejected() {
    let directory = TestDir::new();
    let (root, base) = fixture(&directory, &[], false);
    let prepared = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    for mutation in 0..4 {
        let mut map = prepared.source_map().clone();
        if mutation == 0 {
            map.schema_version = 2;
        }
        let form = map
            .files
            .iter_mut()
            .find(|file| matches!(file.origin, FileOrigin::ManagedForm { .. }))
            .unwrap();
        let FileOrigin::ManagedForm {
            uuid: owner,
            descriptor,
            working,
            ..
        } = &mut form.origin
        else {
            unreachable!()
        };
        match mutation {
            1 => *owner = uuid('2'),
            2 => *descriptor = SourcePath::parse("Catalogs/Sample.xml").unwrap(),
            3 => *working = None,
            _ => {}
        }
        assert_eq!(
            map.validate().unwrap_err().code,
            "source_map_origin_invalid"
        );
    }
}

/// Captured Unicode bytes and char boundaries are verified before native/YAML coordinate interpretation.
#[test]
fn dialog_coordinates_reject_changed_bytes_and_split_utf8_spans() {
    let directory = TestDir::new();
    let (root, base) = fixture(&directory, &[], false);
    let accepted = fs::read_to_string(root.join(OWNER)).unwrap();
    let working = edit(&root, |value| {
        value["form"]["properties"]["Title"]["text"]["ru"] = Value::String("Новое 😀".into());
    });
    let prepared = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    let native = fs::read(prepared.xml().join(DIALOG)).unwrap();
    let text = std::str::from_utf8(&native).unwrap();
    let start = text.find("😀").unwrap();
    let file = prepared
        .source_map()
        .file(&SourcePath::parse(DIALOG).unwrap())
        .unwrap();
    let texts = Some(OriginTexts {
        accepted: &accepted,
        working: Some(&working),
    });
    assert_eq!(
        file.locate(&native, start + 1..start + 2, texts)
            .unwrap_err()
            .code,
        "source_map_span_invalid"
    );
    assert_eq!(
        file.locate(
            &native,
            start..start + 4,
            Some(OriginTexts {
                accepted: &accepted,
                working: Some("changed")
            })
        )
        .unwrap_err()
        .code,
        "source_map_input_changed"
    );
    assert_eq!(
        file.locate(&native, start..start + 4, None)
            .unwrap_err()
            .code,
        "source_map_origin_text_missing"
    );
    assert_eq!(
        file.locate(
            &native,
            start..start + 4,
            Some(OriginTexts {
                accepted: "changed",
                working: Some(&working)
            })
        )
        .unwrap_err()
        .code,
        "source_map_input_changed"
    );
    let mut changed = native;
    changed[0] ^= 1;
    assert_eq!(
        file.locate(&changed, 0..1, texts).unwrap_err().code,
        "source_map_input_changed"
    );
}

/// A preserved attribute title cannot acquire a working UI property coordinate through an equal ID.
#[test]
fn attribute_scope_id_does_not_borrow_ui_property_coordinates() {
    let directory = TestDir::new();
    let (root, _base) = fixture(&directory, &[], false);
    let mut owner = crate::project_core::source_format::ObjectSource::from_yaml(
        &fs::read_to_string(root.join(OWNER)).unwrap(),
    )
    .unwrap();
    let shadow = XML.replace("<ReadOnly>", "<Attributes><Attribute name=\"Shadow\" id=\"3\"><Title formatted=\"true\"><v8:item><v8:lang>ru</v8:lang><v8:content>Opaque shadow</v8:content></v8:item></Title></Attribute></Attributes><ReadOnly>");
    owner.form = Some(ManagedFormSource::from_xml(&shadow).unwrap());
    let accepted = owner.to_yaml().unwrap();
    fs::write(root.join(OWNER), &accepted).unwrap();
    let base_path = directory.0.join("shadow-base");
    let reference = accept_source(&root, &base_path, &[]).unwrap();
    let base = AcceptedBase::open(&base_path, &reference).unwrap();
    let working = edit(&root, |value| {
        value["form"]["elements"]["3"]["properties"]["Title"]["text"]["ru"] =
            Value::String("Working UI title".into());
    });
    let prepared = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    assert_eq!(
        mapped_value(&prepared, &accepted, &working, "Working UI title"),
        (InputAuthority::Working, "\"Working UI title\"".into())
    );
    let (authority, span) = mapped_value(&prepared, &accepted, &working, "Opaque shadow");
    assert_eq!(authority, InputAuthority::Accepted);
    assert!(span.contains("Opaque shadow"));
}
