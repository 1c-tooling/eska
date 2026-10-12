//! Reference edits resolve declared targets while protection restores immutable native bindings.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::baseline::{tree, uuid};
use crate::support::TestDir;
use eska::project::source_format::{
    AcceptedBase, InputAuthority, MetadataReference, MetadataReferenceList, ObjectSource,
    OriginTexts, PropertyName, PropertyValue, SourcePath, SourceReader, accept_source,
    import_designer, import_designer_readable, prepare_property_edit, prepare_protected,
};

/// Independent declared targets cover owner-local forms, named scalars and reference list domains.
fn prepare(root: &Path, readable: bool, frozen: bool) -> (PathBuf, AcceptedBase) {
    let input = super::fixture(root);
    fs::remove_file(input.join("Ext/ParentConfigurations.bin")).unwrap();
    let root_file = input.join("Configuration.xml");
    let configuration=fs::read_to_string(&root_file).unwrap()
        .replace(" version=\"2.20\""," xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" version=\"2.20\"")
        .replace("<CompatibilityMode>","<DefaultLanguage>Language.Russian</DefaultLanguage><DefaultRoles><xr:Item xsi:type=\"xr:MDObjectRef\">Role.Full</xr:Item></DefaultRoles><CompatibilityMode>")
        .replace("<CommonModule>Tools</CommonModule>","<Language>Russian</Language><Language>English</Language><Role>Full</Role><Role>Reader</Role><Catalog>Other</Catalog><CommonModule>Tools</CommonModule>");
    fs::write(&root_file, configuration).unwrap();
    for (folder, kind, name, id) in [
        ("Languages", "Language", "Russian", '8'),
        ("Languages", "Language", "English", '9'),
        ("Roles", "Role", "Full", 'a'),
        ("Roles", "Role", "Reader", 'b'),
        ("Catalogs", "Catalog", "Other", 'c'),
    ] {
        fs::create_dir_all(input.join(folder)).unwrap();
        let children = if kind == "Catalog" {
            "<ChildObjects><Form>Foreign</Form></ChildObjects>"
        } else {
            ""
        };
        let text = format!(
            "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><{kind} uuid=\"{}\"><Properties><Name>{name}</Name></Properties>{children}</{kind}></MetaDataObject>",
            uuid(id).as_str()
        );
        fs::write(input.join(folder).join(format!("{name}.xml")), text).unwrap();
    }
    let other = input.join("Catalogs/Other/Forms");
    fs::create_dir_all(&other).unwrap();
    let form = fs::read_to_string(input.join("Catalogs/Sample/Forms/Main.xml"))
        .unwrap()
        .replace(uuid('7').as_str(), uuid('d').as_str())
        .replace("<Name>Main</Name>", "<Name>Foreign</Name>");
    fs::write(other.join("Foreign.xml"), form).unwrap();
    let file = input.join("Catalogs/Sample.xml");
    let native=fs::read_to_string(&file).unwrap()
        .replace("xmlns:xsi=","xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" xmlns:xsi=")
        .replace("<CodeLength>","<DefaultObjectForm>Catalog.Sample.Form.Main</DefaultObjectForm><Owners><xr:Item xsi:type=\"xr:MDObjectRef\">Catalog.Sample</xr:Item><xr:Item xsi:type=\"xr:MDObjectRef\">Catalog.Other</xr:Item></Owners><CodeLength>");
    fs::write(file, native).unwrap();
    let source = root.join("source");
    if readable {
        import_designer_readable(&input, &source).unwrap();
    } else {
        import_designer(&input, &source).unwrap();
    }
    let storage = root.join("accepted");
    let freeze = if frozen { vec![uuid('2')] } else { Vec::new() };
    let pin = accept_source(&source, &storage, &freeze).unwrap();
    (source, AcceptedBase::open(&storage, &pin).unwrap())
}

/// Read the single property authority without searching for a coincidental reference spelling.
fn owners(source: &Path) -> MetadataReferenceList {
    let value = ObjectSource::from_yaml(
        &fs::read_to_string(source.join("Catalogs/Sample/object.yaml")).unwrap(),
    )
    .unwrap();
    let Some(PropertyValue::ReferenceList(value)) = value
        .document
        .descriptor
        .object
        .property(PropertyName::Owners)
    else {
        panic!("owners")
    };
    (**value).clone()
}

/// Scalar and list edits retain authored syntax, reject missing/foreign targets and preserve sources.
#[test]
fn reference_edits_validate_targets_keep_cst_and_map_current_values() {
    for readable in [false, true] {
        let directory = TestDir::new();
        let (source, base) = prepare(&directory.0, readable, false);
        edit_scalar_references(&source, &base);
        edit_reference_list(&source, &base);
        let prepared =
            prepare_protected(&source, &base, &directory.0.join("generations"), true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        assert_origin(&directory.0, &source, &base, &prepared);
        assert_semantic_parity(&source, &prepared);
    }
}

/// Exact scalar edits validate existence, class and the form's own declared parent.
fn edit_scalar_references(source: &Path, base: &AcceptedBase) {
    let root = source.join("configuration.yaml");
    let original = fs::read_to_string(&root)
        .unwrap()
        .replace("\"Language.Russian\"", "'Language.Russian' # chosen")
        .replace('\n', "\r\n");
    fs::write(&root, &original).unwrap();
    let next = PropertyValue::Reference(MetadataReference::parse("Language.English").unwrap());
    let edit = prepare_property_edit(
        source,
        base,
        &uuid('1'),
        PropertyName::DefaultLanguage,
        next.clone(),
    )
    .unwrap();
    assert_eq!(
        edit.updated(),
        original.replace("'Language.Russian'", "'Language.English'")
    );
    edit.publish(base).unwrap();
    assert!(
        prepare_property_edit(
            source,
            base,
            &uuid('1'),
            PropertyName::DefaultLanguage,
            next
        )
        .unwrap()
        .is_empty()
    );
    let before = tree(source);
    for (property, raw, code) in [
        (
            PropertyName::DefaultLanguage,
            "Language.Missing",
            "source_metadata_reference_missing",
        ),
        (
            PropertyName::DefaultLanguage,
            "Role.Full",
            "source_metadata_reference_domain_invalid",
        ),
        (
            PropertyName::DefaultObjectForm,
            "Catalog.Other.Form.Foreign",
            "source_metadata_reference_owner_invalid",
        ),
    ] {
        let id = if property == PropertyName::DefaultLanguage {
            uuid('1')
        } else {
            uuid('2')
        };
        let error = prepare_property_edit(
            source,
            base,
            &id,
            property,
            PropertyValue::Reference(MetadataReference::parse(raw).unwrap()),
        )
        .unwrap_err();
        assert_eq!(error.code, code);
    }
    assert_eq!(tree(source), before);
}

/// Existing quoted list members remain intact when reordered and extended.
fn edit_reference_list(source: &Path, base: &AcceptedBase) {
    let owner = SourcePath::parse("Catalogs/Sample/object.yaml").unwrap();
    let file = source.join(owner.as_path());
    let original = fs::read_to_string(&file)
        .unwrap()
        .replace("\"Catalog.Sample\"", "'Catalog.Sample' # first")
        .replace("\"Catalog.Other\"", "'Catalog.Other' # second")
        .replace('\n', "\r\n");
    fs::write(&file, &original).unwrap();
    let mut value = owners(source);
    value.items.swap(0, 1);
    let edit = prepare_property_edit(
        source,
        base,
        &uuid('2'),
        PropertyName::Owners,
        PropertyValue::ReferenceList(Box::new(value.clone())),
    )
    .unwrap();
    assert_eq!(
        edit.updated(),
        original
            .replace("'Catalog.Sample' # first", "'Catalog.Other' # first")
            .replace("'Catalog.Other' # second", "'Catalog.Sample' # second")
    );
    edit.publish(base).unwrap();
    value
        .items
        .push(MetadataReference::parse("Catalog.Other").unwrap());
    let edit = prepare_property_edit(
        source,
        base,
        &uuid('2'),
        PropertyName::Owners,
        PropertyValue::ReferenceList(Box::new(value.clone())),
    )
    .unwrap();
    edit.publish(base).unwrap();
    assert!(
        prepare_property_edit(
            source,
            base,
            &uuid('2'),
            PropertyName::Owners,
            PropertyValue::ReferenceList(Box::new(value))
        )
        .unwrap()
        .is_empty()
    );
}

/// The final native property maps to the current YAML list rather than accepted source values.
fn assert_origin(
    root: &Path,
    source: &Path,
    base: &AcceptedBase,
    prepared: &eska::project::source_format::PreparedSource,
) {
    let owner = SourcePath::parse("Catalogs/Sample/object.yaml").unwrap();
    let file = source.join(owner.as_path());
    let native = SourcePath::parse("Catalogs/Sample.xml").unwrap();
    let bytes = fs::read(prepared.xml().join(native.as_path())).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    let start = text.find("<Owners>").unwrap();
    let restored = root.join("restored");
    base.restore(&restored).unwrap();
    let accepted = fs::read_to_string(restored.join(owner.as_path())).unwrap();
    let working = fs::read_to_string(&file).unwrap();
    let location = prepared
        .source_map()
        .file(&native)
        .unwrap()
        .locate(
            &bytes,
            start..start + 8,
            Some(OriginTexts {
                accepted: &accepted,
                working: Some(&working),
            }),
        )
        .unwrap();
    assert_eq!(location.authority, InputAuthority::Working);
    let span = &working[location.range.unwrap()];
    assert!(span.contains("# first") && span.contains("# second"));
}

/// Native parser and direct source projection retain their complete qualified property model.
fn assert_semantic_parity(source: &Path, prepared: &eska::project::source_format::PreparedSource) {
    let reader = SourceReader::open(source).unwrap();
    for owner in reader.owner_paths() {
        let snapshot = reader
            .read_metadata(owner, eska::project::metadata_parser::PropertiesMode::All)
            .unwrap();
        for part in &snapshot.documents {
            let native = fs::read_to_string(prepared.xml().join(part.path.as_path())).unwrap();
            let parsed = eska::project::metadata_parser::parse(
                &native,
                part.descriptor.objects[0].metadata.parent().cloned(),
                eska::project::metadata_parser::PropertiesMode::All,
            )
            .unwrap();
            for (native, source) in parsed.objects.iter().zip(&part.descriptor.objects) {
                assert_eq!(native.metadata, source.metadata);
                assert_eq!(
                    native
                        .properties
                        .as_ref()
                        .unwrap()
                        .iter()
                        .map(|p| &p.property)
                        .collect::<Vec<_>>(),
                    source
                        .properties
                        .as_ref()
                        .unwrap()
                        .iter()
                        .map(|p| &p.property)
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

/// Selection restores forbidden raw references before checking targets and pins list XML bindings.
#[test]
fn protected_references_restore_frozen_values_and_refuse_allowed_dangling_targets() {
    for readable in [false, true] {
        for frozen in [false, true] {
            let directory = TestDir::new();
            let (source, base) = prepare(&directory.0, readable, frozen);
            let file = source.join("Catalogs/Sample/object.yaml");
            let before = fs::read_to_string(&file).unwrap();
            let raw = before.replace("\"Catalog.Other\"", "\"Catalog.Missing\"");
            fs::write(&file, &raw).unwrap();
            let output = directory.0.join("generations");
            if frozen {
                let prepared = prepare_protected(&source, &base, &output, false).unwrap();
                assert!(!prepared.diagnostics().is_empty());
                let native =
                    fs::read_to_string(prepared.xml().join("Catalogs/Sample.xml")).unwrap();
                assert!(native.contains("Catalog.Other") && !native.contains("Catalog.Missing"));
                assert_eq!(
                    prepare_protected(&source, &base, &output, true)
                        .unwrap_err()
                        .error
                        .code,
                    "source_changes_excluded"
                );
            } else {
                assert_eq!(
                    prepare_protected(&source, &base, &output, false)
                        .unwrap_err()
                        .error
                        .code,
                    "source_metadata_reference_missing"
                );
                assert!(!output.exists());
            }
            assert_eq!(fs::read_to_string(&file).unwrap(), raw);
            if !frozen {
                let raw = before
                    .replace("\"Catalog.Other\"", "\"Catalog.Sample\"")
                    .replace("\"item_prefix\": \"xr\"", "\"item_prefix\": \"unknown\"");
                fs::write(&file, &raw).unwrap();
                let prepared = prepare_protected(&source, &base, &output, false).unwrap();
                assert!(!prepared.diagnostics().is_empty());
                let native =
                    fs::read_to_string(prepared.xml().join("Catalogs/Sample.xml")).unwrap();
                assert!(!native.contains("unknown:Item"));
                assert_eq!(
                    native
                        .matches("<xr:Item xsi:type=\"xr:MDObjectRef\">Catalog.Sample</xr:Item>")
                        .count(),
                    2
                );
                assert_eq!(
                    prepare_protected(&source, &base, &output, true)
                        .unwrap_err()
                        .error
                        .code,
                    "source_changes_excluded"
                );
                assert_eq!(fs::read_to_string(&file).unwrap(), raw);
            }
        }
    }
}
