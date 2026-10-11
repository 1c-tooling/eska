//! Scalar edits use the same accepted ancestry, publication and origins as existing text edits.

mod platform;

use std::{
    fs,
    path::{Path, PathBuf},
};

use eska::project::source_format::{
    AcceptedBase, ExactUnsigned, InputAuthority, ObjectSource, Operation, OriginTexts,
    PermissionReason, Property, PropertyName, PropertyValue, Section, SourcePath, SourceReader,
    XmlFragment, accept_source, import_designer, prepare_property_edit, prepare_protected,
};

use super::baseline::{tree, uuid};
use crate::support::TestDir;

/// Add independent native scalar examples only to a private fixture, retaining its BOM and CRLF.
fn prepare(root: &Path, supported: bool, frozen: &[char]) -> (PathBuf, AcceptedBase) {
    let input = super::baseline::fixture(root);
    if !supported {
        fs::remove_file(input.join("Ext/ParentConfigurations.bin")).unwrap();
        fs::remove_dir_all(input.join("Ext/ParentConfigurations")).unwrap();
    }
    let catalog = input.join("Catalogs/Sample.xml");
    let text = fs::read_to_string(&catalog)
        .unwrap()
        .replace(
            "<CodeLength>9</CodeLength>",
            "<CodeLength>9</CodeLength><CodeType>String</CodeType>",
        )
        .replace(
            "<Comment>Editable part</Comment>",
            "<Comment>Editable part</Comment><MultiLine>false</MultiLine>",
        )
        .replace(
            "<Comment xsi:nil=\"true\"/>",
            "<Comment xsi:nil=\"true\"/><LineNumberLength>5</LineNumberLength>",
        );
    fs::write(catalog, text).unwrap();
    let source = root.join("source");
    import_designer(&input, &source).unwrap();
    let accepted = root.join("accepted");
    let frozen = frozen.iter().copied().map(uuid).collect::<Vec<_>>();
    let pin = accept_source(&source, &accepted, &frozen).unwrap();
    (source, AcceptedBase::open(&accepted, &pin).unwrap())
}

/// Exact scalar substitutions preserve surrounding comments, source paths and every other file.
#[test]
fn bool_integer_enum_and_inline_edits_keep_cst_and_noop_bytes() {
    for (id, property, value, before, after) in [
        (
            '2',
            PropertyName::Hierarchical,
            PropertyValue::Boolean(true),
            "\"value\": FALSE # keep",
            "\"value\": true # keep",
        ),
        (
            '2',
            PropertyName::CodeLength,
            PropertyValue::Integer(ExactUnsigned::parse("012").unwrap()),
            "\"value\": '9' # keep",
            "\"value\": '012' # keep",
        ),
        (
            '2',
            PropertyName::CodeType,
            PropertyValue::Enumeration("Number".into()),
            "\"value\": 'String' # keep",
            "\"value\": 'Number' # keep",
        ),
        (
            '3',
            PropertyName::MultiLine,
            PropertyValue::Boolean(true),
            "\"value\": FALSE # keep",
            "\"value\": true # keep",
        ),
        (
            '4',
            PropertyName::LineNumberLength,
            PropertyValue::Integer(ExactUnsigned::parse("6").unwrap()),
            "\"value\": '5' # keep",
            "\"value\": '6' # keep",
        ),
    ] {
        let temporary = TestDir::new();
        let (source, base) = prepare(&temporary.0, false, &[]);
        let path = source.join("Catalogs/Sample/object.yaml");
        let yaml = fs::read_to_string(&path).unwrap();
        let quoted = match property {
            PropertyName::CodeLength => "\"value\": \"9\"",
            PropertyName::CodeType => "\"value\": \"String\"",
            PropertyName::LineNumberLength => "\"value\": \"5\"",
            _ => "\"value\": false",
        };
        let name = format!("\"name\": \"{}\"", property.as_str());
        let start = yaml.find(&name).unwrap();
        let offset = start + yaml[start..].find(quoted).unwrap();
        let mut decorated = yaml.clone();
        decorated.replace_range(offset..offset + quoted.len(), before);
        let original = format!("# Я😀\r\n{}", decorated.replace('\n', "\r\n"));
        fs::write(&path, &original).unwrap();
        let initial = tree(&source);
        let plan =
            prepare_property_edit(&source, &base, &uuid(id), property, value.clone()).unwrap();
        assert_eq!(plan.updated(), original.replacen(before, after, 1));
        plan.publish(&base).unwrap();
        let mut expected = initial;
        expected.insert(
            "Catalogs/Sample/object.yaml".into(),
            Some(plan.updated().as_bytes().to_vec()),
        );
        assert_eq!(tree(&source), expected);
        let no_op = prepare_property_edit(&source, &base, &uuid(id), property, value).unwrap();
        assert!(no_op.is_empty());
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        no_op.publish(&base).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        let result =
            prepare_protected(&source, &base, &temporary.0.join("generations"), true).unwrap();
        assert!(result.diagnostics().is_empty());
    }
}

/// Working scalar origins address exact values while preserved legacy properties keep XML ranges.
#[test]
fn selected_scalar_values_have_precise_working_source_coordinates() {
    for (property, value, xml_value, yaml_value) in [
        (
            PropertyName::Hierarchical,
            PropertyValue::Boolean(true),
            "true",
            "true",
        ),
        (
            PropertyName::CodeLength,
            PropertyValue::Integer(ExactUnsigned::parse("12").unwrap()),
            "12",
            "\"12\"",
        ),
        (
            PropertyName::CodeType,
            PropertyValue::Enumeration("Number".into()),
            "Number",
            "\"Number\"",
        ),
    ] {
        let temporary = TestDir::new();
        let (source, base) = prepare(&temporary.0, false, &[]);
        let owner = source.join("Catalogs/Sample/object.yaml");
        let accepted = fs::read_to_string(&owner).unwrap();
        prepare_property_edit(&source, &base, &uuid('2'), property, value)
            .unwrap()
            .publish(&base)
            .unwrap();
        let working = fs::read_to_string(&owner).unwrap();
        let prepared =
            prepare_protected(&source, &base, &temporary.0.join("generations"), true).unwrap();
        let native = fs::read(prepared.xml().join("Catalogs/Sample.xml")).unwrap();
        let text = std::str::from_utf8(&native).unwrap();
        let tag = format!("<{}>{xml_value}</{}>", property.as_str(), property.as_str());
        let start = text.find(&tag).unwrap();
        let origin = prepared
            .source_map()
            .file(&SourcePath::parse("Catalogs/Sample.xml").unwrap())
            .unwrap();
        let location = origin
            .locate(
                &native,
                start..start + tag.len(),
                Some(OriginTexts {
                    accepted: &accepted,
                    working: Some(&working),
                }),
            )
            .unwrap();
        assert_eq!(location.authority, InputAuthority::Working);
        assert_eq!(location.uuid, Some(uuid('2')));
        assert_eq!(&working[location.range.unwrap()], yaml_value);
    }
}

/// Neither a typed UI edit nor a raw YAML bypass can change a locked or inherited-frozen scalar.
#[test]
fn native_support_and_owner_freeze_restore_new_scalar_changes() {
    for (supported, frozen, id, property, reason) in [
        (
            true,
            vec![],
            '2',
            PropertyName::Hierarchical,
            PermissionReason::NativeRestriction,
        ),
        (
            false,
            vec!['2'],
            '3',
            PropertyName::MultiLine,
            PermissionReason::EskaFrozen,
        ),
    ] {
        let temporary = TestDir::new();
        let (source, base) = prepare(&temporary.0, supported, &frozen);
        assert_eq!(
            base.permission(&uuid(id), Operation::Property(property))
                .reason,
            reason
        );
        let initial = tree(&source);
        assert!(
            prepare_property_edit(
                &source,
                &base,
                &uuid(id),
                property,
                PropertyValue::Boolean(true)
            )
            .is_err()
        );
        assert_eq!(tree(&source), initial);
        let path = source.join("Catalogs/Sample/object.yaml");
        let mut object = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
        change(
            &mut object.document.descriptor.object,
            id,
            property,
            PropertyValue::Boolean(true),
        );
        fs::write(&path, object.to_yaml().unwrap()).unwrap();
        let raw = tree(&source);
        let strict =
            prepare_protected(&source, &base, &temporary.0.join("strict"), true).unwrap_err();
        assert!(
            strict
                .diagnostics
                .iter()
                .any(|issue| issue.operation == Operation::Property(property)
                    && issue.uuid == Some(uuid(id))
                    && issue.reason == reason)
        );
        assert!(!temporary.0.join("strict").exists());
        let restored =
            prepare_protected(&source, &base, &temporary.0.join("restored"), false).unwrap();
        assert_eq!(tree(&source), raw);
        assert!(restored.diagnostics().iter().any(|issue| issue.operation
            == Operation::Property(property)
            && issue.reason == reason));
        let xml = fs::read_to_string(restored.xml().join("Catalogs/Sample.xml")).unwrap();
        assert!(xml.contains(&format!(
            "<{}>false</{}>",
            property.as_str(),
            property.as_str()
        )));
    }
}

/// Existing preserved scalar copies remain authoritative until a separate explicit migration.
#[test]
fn legacy_preserved_scalars_open_and_export_without_automatic_promotion() {
    let temporary = TestDir::new();
    let (source, _) = prepare(&temporary.0, false, &[]);
    let path = source.join("Catalogs/Sample/object.yaml");
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    for section in &mut owner.document.descriptor.object.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if matches!(
                    property,
                    Property::Typed {
                        name: PropertyName::CodeLength,
                        ..
                    }
                ) {
                    *property = Property::Preserved {
                        fragment: XmlFragment {
                            xml: "<CodeLength>9</CodeLength>".into(),
                            namespaces: vec![],
                        },
                    };
                }
            }
        }
    }
    let original = format!(
        "# legacy scalar\r\n{}",
        owner.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(&path, &original).unwrap();
    let before = tree(&source);
    let accepted = temporary.0.join("legacy-base");
    let pin = accept_source(&source, &accepted, &[]).unwrap();
    let base = AcceptedBase::open(&accepted, &pin).unwrap();
    let reader = SourceReader::open(&source).unwrap();
    assert_eq!(
        reader
            .read_owner(&SourcePath::parse("Catalogs/Sample/object.yaml").unwrap())
            .unwrap()
            .source,
        owner
    );
    let metadata = reader
        .read_metadata(
            &SourcePath::parse("Catalogs/Sample/object.yaml").unwrap(),
            eska::project::metadata_parser::PropertiesMode::All,
        )
        .unwrap();
    assert!(
        metadata.documents[0].descriptor.objects[0]
            .properties
            .as_ref()
            .unwrap()
            .iter()
            .any(|property| property.property.key.name == "CodeLength"
                && property.property.value
                    == eska::project::metadata_model::MetadataValue::Text("9".into()))
    );
    assert_eq!(
        prepare_property_edit(
            &source,
            &base,
            &uuid('2'),
            PropertyName::CodeLength,
            PropertyValue::Integer(ExactUnsigned::parse("12").unwrap())
        )
        .unwrap_err()
        .code,
        "source_edit_property_missing"
    );
    let prepared =
        prepare_protected(&source, &base, &temporary.0.join("legacy-generation"), true).unwrap();
    let native = fs::read(prepared.xml().join("Catalogs/Sample.xml")).unwrap();
    let text = std::str::from_utf8(&native).unwrap();
    let start = text.find("<CodeLength>9</CodeLength>").unwrap();
    let origin = prepared
        .source_map()
        .file(&SourcePath::parse("Catalogs/Sample.xml").unwrap())
        .unwrap();
    let location = origin
        .locate(
            &native,
            start..start + "<CodeLength>9</CodeLength>".len(),
            Some(OriginTexts {
                accepted: &original,
                working: None,
            }),
        )
        .unwrap();
    assert_eq!(location.authority, InputAuthority::Accepted);
    assert!(original[location.range.unwrap()].contains("<CodeLength>9</CodeLength>"));
    assert_eq!(tree(&source), before);
}

/// Find a native fixture identity recursively rather than relying on a child position.
fn change(
    node: &mut eska::project::source_format::MetadataNode,
    id: char,
    name: PropertyName,
    value: PropertyValue,
) {
    if node.uuid == uuid(id) {
        for section in &mut node.sections {
            if let Section::Properties(properties) = section {
                for property in properties {
                    if let Property::Typed {
                        name: current,
                        value: target,
                    } = property
                        && *current == name
                    {
                        *target = value;
                        return;
                    }
                }
            }
        }
        panic!("existing scalar");
    }
    for section in &mut node.sections {
        if let Section::Children(children) = section {
            for child in children {
                if let eska::project::source_format::Child::Inline { object } = child {
                    change(object, id, name, value.clone());
                }
            }
        }
    }
}
