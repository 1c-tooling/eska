//! Project-level tests prove sole form ownership, exact inverse bytes and explicit format boundaries.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::*;
use crate::{
    project_core::source_format::{
        AcceptedBase, Descriptor, FormPropertyName, FormPropertyValue, Property, PropertyName,
        PropertyValue, Section, SourceReader, accept_source, export_designer, files::Inventory,
        import_designer_forms, import_designer_readable, prepare_protected,
        prepare_readable_layout,
    },
    test_support::TestDir,
};

const FORM: &str = "Catalogs/Sample/Forms/Main/form.yaml";
const DIALOG: &str = "Catalogs/Sample/Forms/Main/Ext/Form.xml";
const XML: &str = "\u{feff}<Form xmlns=\"http://v8.1c.ru/8.3/xcf/logform\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" version=\"2.20\">\r\n<AutoCommandBar name=\"\" id=\"-1\"/><Title><v8:item><v8:lang>ru</v8:lang><v8:content>Заголовок</v8:content></v8:item></Title><ChildItems><UsualGroup name=\"First\" id=\"1\"><ChildItems/></UsualGroup><UsualGroup name=\"Second\" id=\"2\"><ChildItems/></UsualGroup></ChildItems><ReadOnly>true</ReadOnly>\r\n</Form>\r\n";

/// Test directories are owned playground subtrees; the repository fixture is read only.
fn fixture(directory: &TestDir) -> PathBuf {
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    let native = directory.0.join("native");
    fs::create_dir(&native).unwrap();
    let inventory = Inventory::read(&original).unwrap();
    for path in &inventory.directories {
        fs::create_dir_all(native.join(path.as_path())).unwrap();
    }
    for path in inventory.files.keys() {
        fs::copy(original.join(path.as_path()), native.join(path.as_path())).unwrap();
    }
    fs::write(native.join(DIALOG), XML).unwrap();
    let module = native.join("Catalogs/Sample/Forms/Main/Ext/Form/Module.bsl");
    fs::create_dir_all(module.parent().unwrap()).unwrap();
    fs::copy(native.join("CommonModules/Tools/Ext/Module.bsl"), module).unwrap();
    // This fixture has no supplier CF, so the focused form tests use an unsupported-free base.
    fs::remove_file(native.join("Ext/ParentConfigurations.bin")).unwrap();
    native
}

/// Checked paths address exact declared authoring locations rather than short form names.
fn path(value: &str) -> SourcePath {
    SourcePath::parse(value).unwrap()
}

/// Create a new map-v4 source while retaining its original native input for independent checks.
fn source() -> (TestDir, PathBuf, PathBuf) {
    let directory = TestDir::new();
    let native = fixture(&directory);
    let source = directory.0.join("source");
    import_designer_forms(&native, &source).unwrap();
    (directory, native, source)
}

/// Fragmented command text retains its native bytes through map-v4 import, saved/overlay reading and export.
#[test]
fn unified_form_references_preserve_xml_trivia_and_reject_dangling_overlays() {
    let directory = TestDir::new();
    let native = fixture(&directory);
    let input = XML
        .replace(
            "<UsualGroup name=\"Second\" id=\"2\"><ChildItems/></UsualGroup>",
            "<Button name=\"Run\" id=\"2\"><CommandName>Form.<!-- keep -->Command.<?note keep?>Action</CommandName></Button>",
        )
        .replace(
            "</Form>",
            "<Commands><Command name=\"Action\" id=\"1\"/></Commands></Form>",
        );
    fs::write(native.join(DIALOG), &input).unwrap();
    let original = Inventory::read(&native).unwrap();
    let root = directory.0.join("source");
    import_designer_forms(&native, &root).unwrap();
    let before = Inventory::read(&root).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let model = reader.read_form_layout(&path(FORM), 1_000_000).unwrap();
    let commands: Vec<_> = model
        .local_commands
        .iter()
        .map(|(source, target)| (source.as_str(), target.as_str()))
        .collect();
    assert_eq!(commands, [("2", "1")]);
    let yaml = fs::read_to_string(root.join(FORM)).unwrap();
    assert_eq!(reader.parse_form_layout(&path(FORM), &yaml).unwrap(), model);
    assert_eq!(
        reader
            .parse_form_layout(&path(FORM), &yaml.replace("?>Action", "?>Missing"))
            .unwrap_err()
            .code,
        "source_form_reference_missing"
    );
    let output = directory.0.join("output");
    export_designer(&root, &output).unwrap();
    assert_eq!(fs::read(output.join(DIALOG)).unwrap(), input.as_bytes());
    assert_eq!(Inventory::read(&native).unwrap(), original);
    assert_eq!(Inventory::read(&root).unwrap(), before);
}

/// A single form YAML generates two native files while module/resource bytes stay independent.
#[test]
fn unified_form_import_reader_export_and_repeat_preserve_exact_dialog_bytes() {
    let (directory, native, root) = source();
    let before = Inventory::read(&native).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    assert_eq!(
        reader.manifest().schema_version,
        MANAGED_FORM_LAYOUT_VERSION
    );
    assert_eq!(
        reader.form_dialog_paths().collect::<Vec<_>>(),
        [&path(FORM)]
    );
    assert_eq!(reader.form_dialog_owner(&path(FORM)), Some(&path(FORM)));
    assert_eq!(
        reader
            .form_for_dialog(&path(FORM))
            .unwrap()
            .native_descriptor,
        &path("Catalogs/Sample/Forms/Main.xml")
    );
    assert!(!root.join(DIALOG).exists());
    assert!(!root.join("Catalogs/Sample/Forms/Main/Form.xml").exists());
    assert!(root.join("Catalogs/Sample/Forms/Main/Module.bsl").is_file());
    assert_eq!(
        reader
            .read_form_dialog_text(&path(FORM), 1_000_000)
            .unwrap(),
        XML
    );
    let body = reader.read_form_source(&path(FORM), 1_000_000).unwrap();
    assert_eq!(
        reader.read_form_layout(&path(FORM), 1_000_000).unwrap(),
        body.layout_snapshot().unwrap()
    );
    let text = fs::read_to_string(root.join(FORM)).unwrap();
    assert_eq!(reader.parse_form_source(&path(FORM), &text).unwrap(), body);
    let output = directory.0.join("output");
    export_designer(&root, &output).unwrap();
    let emitted = Inventory::read(&output).unwrap();
    assert_eq!(fs::read(output.join(DIALOG)).unwrap(), XML.as_bytes());
    let control = directory.0.join("control");
    let old = directory.0.join("old");
    import_designer_readable(&native, &old).unwrap();
    export_designer(&old, &control).unwrap();
    assert_eq!(Inventory::read(&control).unwrap(), emitted);
    export_designer(&root, &output).unwrap();
    assert_eq!(Inventory::read(&output).unwrap(), emitted);
    assert_eq!(Inventory::read(&native).unwrap(), before);
}

/// Both descriptor properties and dialog layout/values come from the same current YAML overlay.
#[test]
fn current_metadata_and_body_edits_export_without_a_raw_dialog_fallback() {
    let (directory, _native, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let mut object = reader.read_owner(&path(FORM)).unwrap().source;
    let old = object.form.clone().unwrap();
    for section in &mut object.document.descriptor.object.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    name: PropertyName::Comment,
                    value,
                } = property
                {
                    *value = PropertyValue::Text("Metadata edit".into());
                }
            }
        }
    }
    let body = object.form.as_mut().unwrap();
    body.layout.roots.reverse();
    body.properties
        .insert(FormPropertyName::ReadOnly, FormPropertyValue::Bool(false));
    let FormPropertyValue::Localized(title) =
        body.properties.get_mut(&FormPropertyName::Title).unwrap()
    else {
        panic!("localized Title");
    };
    title.text.insert("ru".into(), "Новый 😀 & <".into());
    body.validate_change(&old).unwrap();
    let expected = body.to_xml().unwrap();
    let text = object.to_yaml().unwrap();
    assert_eq!(
        reader
            .parse_form_source(&path(FORM), &text)
            .unwrap()
            .to_xml()
            .unwrap(),
        expected
    );
    assert_eq!(
        reader.parse_owner(&path(FORM), &text).unwrap().source,
        object
    );
    let metadata = reader
        .parse_metadata(
            &path(FORM),
            &text,
            crate::project_core::metadata_parser::PropertiesMode::All,
        )
        .unwrap();
    assert_eq!(
        metadata.source_sha256,
        reader
            .parse_owner(&path(FORM), &text)
            .unwrap()
            .source_sha256
    );
    fs::write(root.join(FORM), text).unwrap();
    let output = directory.0.join("edited");
    export_designer(&root, &output).unwrap();
    assert_eq!(fs::read_to_string(output.join(DIALOG)).unwrap(), expected);
    let descriptor = Descriptor::from_xml(
        &fs::read_to_string(output.join("Catalogs/Sample/Forms/Main.xml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        descriptor.object.property(PropertyName::Comment),
        Some(&PropertyValue::Text("Metadata edit".into()))
    );
}

/// Path aliasing is allowed exclusively for one declared descriptor and its exact dialog.
#[test]
fn forged_map_versions_owners_and_dialog_paths_are_rejected() {
    let (_directory, _native, root) = source();
    let manifest = SourceReader::open(&root).unwrap().manifest().clone();
    for mutation in 0..5 {
        let mut forged = manifest.clone();
        if mutation == 0 {
            forged.schema_version = 3;
        }
        let file = forged
            .files
            .iter_mut()
            .find(|file| matches!(file.role, FileRole::ManagedForm { .. }))
            .unwrap();
        match mutation {
            1 => {
                if let FileRole::ManagedForm { source, .. } = &mut file.role {
                    *source = path("Catalogs/Sample/object.yaml");
                }
            }
            2 => {
                if let FileRole::ManagedForm { descriptor, .. } = &mut file.role {
                    *descriptor = path("Catalogs/Sample.xml");
                }
            }
            3 => {
                file.path = path("Catalogs/Sample/Forms/Main/Ext/Other.xml");
            }
            4 => {
                file.role = FileRole::Relocated {
                    source: path("Catalogs/Sample/Forms/Main/Form.xml"),
                };
            }
            _ => {}
        }
        assert_eq!(
            forged.validate().unwrap_err().code,
            "source_layout_payload_invalid"
        );
    }
}

/// Missing bodies, stray second XML and unsupported profiles cannot produce a partial export.
#[test]
fn incomplete_or_double_authored_form_cannot_be_exported() {
    let (directory, _native, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let mut object = reader.read_owner(&path(FORM)).unwrap().source;
    let original = object.to_yaml().unwrap();
    object.form = None;
    let missing = object.to_yaml().unwrap();
    assert_eq!(
        reader.parse_owner(&path(FORM), &missing).unwrap_err().code,
        "source_form_schema_invalid"
    );
    fs::write(root.join(FORM), missing).unwrap();
    let output = directory.0.join("refused");
    assert_eq!(
        export_designer(&root, &output).unwrap_err().code,
        "source_form_schema_invalid"
    );
    assert!(!output.exists());
    fs::write(root.join(FORM), original).unwrap();
    fs::write(root.join("Catalogs/Sample/Forms/Main/Form.xml"), XML).unwrap();
    assert_eq!(
        export_designer(&root, &output).unwrap_err().code,
        "source_file_map_mismatch"
    );
    assert!(!output.exists());
}

/// Combined schema is reserved for standalone readable form descriptors; old imports reject injected bodies.
#[test]
fn body_cannot_be_injected_into_other_owners_or_legacy_maps() {
    let (directory, native, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let form = reader.read_owner(&path(FORM)).unwrap().source.form.unwrap();
    let mut catalog = reader
        .read_owner(&path("Catalogs/Sample/object.yaml"))
        .unwrap()
        .source;
    catalog.form = Some(form.clone());
    assert_eq!(
        catalog.validate().unwrap_err().code,
        "source_form_schema_invalid"
    );
    let old = directory.0.join("old");
    import_designer_readable(&native, &old).unwrap();
    let old_reader = SourceReader::open(&old).unwrap();
    let mut owner = old_reader.read_owner(&path(FORM)).unwrap().source;
    owner.form = Some(form);
    fs::write(old.join(FORM), owner.to_yaml().unwrap()).unwrap();
    assert_eq!(
        old_reader.read_owner(&path(FORM)).unwrap_err().code,
        "source_form_schema_invalid"
    );
    assert_eq!(
        export_designer(&old, &directory.0.join("refused"))
            .unwrap_err()
            .code,
        "source_form_schema_invalid"
    );
}

/// Root opening remains lazy while malformed, missing and oversized addressed forms report current errors.
#[test]
fn reader_does_not_open_neighbor_forms_or_reinterpret_xml_as_yaml() {
    let (_directory, _native, root) = source();
    fs::write(root.join(FORM), "form: [").unwrap();
    let reader = SourceReader::open(&root).unwrap();
    assert_eq!(
        reader
            .read_owner(&path("Catalogs/Sample/object.yaml"))
            .unwrap()
            .source
            .document
            .descriptor
            .object
            .kind,
        MetadataKind::Catalog
    );
    assert_eq!(
        reader
            .read_form_source(&path(FORM), 1_000_000)
            .unwrap_err()
            .code,
        "source_yaml_invalid"
    );
    assert!(reader.parse_form_source(&path(FORM), XML).is_err());
    assert_eq!(
        reader.read_form_source(&path(FORM), 1).unwrap_err().code,
        "source_document_too_large"
    );
    fs::remove_file(root.join(FORM)).unwrap();
    assert_eq!(
        reader
            .read_form_source(&path(FORM), 1_000_000)
            .unwrap_err()
            .code,
        "source_file_missing"
    );
    assert_eq!(
        reader
            .parse_form_source(&path(DIALOG), XML)
            .unwrap_err()
            .code,
        "source_form_dialog_undeclared"
    );
    assert_eq!(
        reader.read_form_layout(&path(DIALOG), 1).unwrap_err().code,
        "source_form_dialog_undeclared"
    );
}

/// Unsupported native dialogs refuse import before a ready marker or destination is published.
#[test]
fn unknown_native_profile_is_preserved_by_legacy_import_but_refused_by_forms_layout() {
    let directory = TestDir::new();
    let native = fixture(&directory);
    fs::write(native.join(DIALOG), "<Form/>").unwrap();
    let refused = directory.0.join("refused");
    assert!(import_designer_forms(&native, &refused).is_err());
    assert!(!refused.exists());
    import_designer_readable(&native, &directory.0.join("legacy")).unwrap();
}

/// Common/nested equal names remain separate identities; a descriptor without a dialog stays absent.
#[test]
fn same_named_common_and_nested_forms_and_absent_dialog_keep_independent_bindings() {
    let directory = TestDir::new();
    let native = fixture(&directory);
    let configuration = native.join("Configuration.xml");
    let text = fs::read_to_string(&configuration).unwrap().replace(
        "</ChildObjects>",
        "<CommonForm>Main</CommonForm><CommonForm>Absent</CommonForm></ChildObjects>",
    );
    fs::write(configuration, text).unwrap();
    fs::create_dir_all(native.join("CommonForms/Main/Ext")).unwrap();
    for (name, suffix) in [("Main", 1), ("Absent", 2)] {
        fs::write(native.join(format!("CommonForms/{name}.xml")), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><CommonForm uuid="a0000000-0000-4000-8000-{suffix:012}"><Properties><Name>{name}</Name></Properties></CommonForm></MetaDataObject>"#)).unwrap();
    }
    let common_xml = XML.replace("Заголовок", "Общая форма");
    fs::write(native.join("CommonForms/Main/Ext/FORM.XML"), &common_xml).unwrap();
    let root = directory.0.join("source");
    import_designer_forms(&native, &root).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let common = path("CommonForms/Main/form.yaml");
    assert_eq!(reader.form_dialog_paths().count(), 2);
    assert_eq!(
        reader.read_form_dialog_text(&common, 1_000_000).unwrap(),
        common_xml
    );
    assert_eq!(
        reader
            .read_form_dialog_text(&path(FORM), 1_000_000)
            .unwrap(),
        XML
    );
    assert_ne!(
        reader.form_for_dialog(&common).unwrap().uuid,
        reader.form_for_dialog(&path(FORM)).unwrap().uuid
    );
    assert!(
        reader
            .form_dialog_path(&path("CommonForms/Absent/form.yaml"))
            .is_none()
    );
    assert!(
        reader
            .read_owner(&path("CommonForms/Absent/form.yaml"))
            .unwrap()
            .source
            .form
            .is_none()
    );
    let output = directory.0.join("output");
    export_designer(&root, &output).unwrap();
    assert_eq!(
        fs::read_to_string(output.join("CommonForms/Main/Ext/FORM.XML")).unwrap(),
        common_xml
    );
}

/// Pending protected selection and migration cannot silently discard the new authoritative body.
#[test]
fn unsupported_protected_and_migration_consumers_refuse_map_v4() {
    let (directory, _native, root) = source();
    let accepted = directory.0.join("accepted");
    let reference = accept_source(&root, &accepted, &[]).unwrap();
    let base = AcceptedBase::open(&accepted, &reference).unwrap();
    let output = directory.0.join("generations");
    assert_eq!(
        prepare_protected(&root, &base, &output, false)
            .unwrap_err()
            .error
            .code,
        "source_schema_unsupported"
    );
    assert!(!output.exists());
    assert_eq!(
        prepare_readable_layout(&root).unwrap_err().code,
        "source_schema_unsupported"
    );
}
