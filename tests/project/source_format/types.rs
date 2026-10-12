//! Type authoring shares accepted ancestry, protection, semantic metadata and source coordinates.

use std::{
    fs,
    path::{Path, PathBuf},
};

use eska::project::source_format::{
    AcceptedBase, ExactUnsigned, InputAuthority, ObjectSource, Operation, OriginTexts,
    PropertyName, PropertyValue, SourcePath, SourceReader, TypeAllowedSign, TypeDescription,
    accept_source, import_designer, import_designer_readable, prepare_property_edit,
    prepare_protected,
};

use super::baseline::{tree, uuid};
use crate::support::TestDir;

/// Independent native inputs make the type editable without promoting a legacy opaque fragment.
fn prepare(root: &Path, readable: bool, supported: bool, frozen: bool) -> (PathBuf, AcceptedBase) {
    let input = super::baseline::fixture(root);
    if supported {
        let rules = input.join("Ext/ParentConfigurations.bin");
        let old = format!("1,0,{0},{0},", uuid('3').as_str());
        let locked = format!("0,0,{0},{0},", uuid('3').as_str());
        let native = fs::read_to_string(&rules).unwrap();
        assert!(native.contains(&old));
        fs::write(rules, native.replace(&old, &locked)).unwrap();
    } else {
        fs::remove_file(input.join("Ext/ParentConfigurations.bin")).unwrap();
        fs::remove_dir_all(input.join("Ext/ParentConfigurations")).unwrap();
    }
    let file = input.join("Catalogs/Sample.xml");
    let native = fs::read_to_string(&file).unwrap()
        .replace("xmlns:xsi=", "xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=")
        .replace("<Comment>Editable part</Comment>", "<Comment>Editable part</Comment><Type><v8:Type>xs:decimal</v8:Type><v8:NumberQualifiers><v8:Digits>9</v8:Digits><v8:FractionDigits>2</v8:FractionDigits><v8:AllowedSign>Any</v8:AllowedSign></v8:NumberQualifiers></Type>");
    fs::write(file, native).unwrap();
    let source = root.join("source");
    if readable {
        import_designer_readable(&input, &source).unwrap();
    } else {
        import_designer(&input, &source).unwrap();
    }
    let storage = root.join("accepted");
    let frozen = if frozen { vec![uuid('2')] } else { Vec::new() };
    let reference = accept_source(&source, &storage, &frozen).unwrap();
    (source, AcceptedBase::open(&storage, &reference).unwrap())
}

/// Select the inline attribute's actual property rather than a coincidental YAML string.
pub(super) fn description(source: &Path) -> Box<TypeDescription> {
    let owner = ObjectSource::from_yaml(
        &fs::read_to_string(source.join("Catalogs/Sample/object.yaml")).unwrap(),
    )
    .unwrap();
    let section = owner
        .document
        .descriptor
        .object
        .sections
        .iter()
        .find_map(|section| {
            if let eska::project::source_format::Section::Children(children) = section {
                Some(children)
            } else {
                None
            }
        })
        .unwrap();
    let attribute = section
        .iter()
        .find_map(|child| {
            if let eska::project::source_format::Child::Inline { object } = child
                && object.uuid == uuid('3')
            {
                Some(object)
            } else {
                None
            }
        })
        .unwrap();
    let Some(PropertyValue::TypeDescription(value)) = attribute.property(PropertyName::Type) else {
        panic!("typed description")
    };
    value.clone()
}

/// Exact leaf edits preserve comments and CRLF, and map native qualifier spans to current YAML.
#[test]
fn composite_type_edits_preserve_cst_and_working_native_origins() {
    for readable in [false, true] {
        let directory = TestDir::new();
        let (source, base) = prepare(&directory.0, readable, false, false);
        let owner = SourcePath::parse("Catalogs/Sample/object.yaml").unwrap();
        let file = source.join(owner.as_path());
        let original = format!(
            "# Я😀\n{}",
            fs::read_to_string(&file)
                .unwrap()
                .replace("\"digits\": \"9\"", "\"digits\": '9' # precision")
                .replace(
                    "\"fraction_digits\": \"2\"",
                    "\"fraction_digits\": '2' # scale"
                )
        )
        .replace('\n', "\r\n");
        fs::write(&file, &original).unwrap();
        let mut value = description(&source);
        let number = value.data.number.as_mut().unwrap();
        number.digits = Some(ExactUnsigned::parse("012").unwrap());
        number.fraction_digits = Some(ExactUnsigned::parse("0003").unwrap());
        number.allowed_sign = Some(TypeAllowedSign::Nonnegative);
        let next = PropertyValue::TypeDescription(value);
        let edit =
            prepare_property_edit(&source, &base, &uuid('3'), PropertyName::Type, next.clone())
                .unwrap();
        assert_eq!(
            edit.updated(),
            original
                .replace("'9' # precision", "'012' # precision")
                .replace("'2' # scale", "'0003' # scale")
                .replace("\"Any\"", "\"Nonnegative\"")
        );
        edit.publish(&base).unwrap();
        assert!(
            prepare_property_edit(&source, &base, &uuid('3'), PropertyName::Type, next)
                .unwrap()
                .is_empty()
        );
        let prepared =
            prepare_protected(&source, &base, &directory.0.join("generations"), true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        let native = SourcePath::parse("Catalogs/Sample.xml").unwrap();
        let bytes = fs::read(prepared.xml().join(native.as_path())).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("<v8:Digits>012</v8:Digits>"));
        assert!(text.contains("<v8:FractionDigits>0003</v8:FractionDigits>"));
        let accepted_source = directory.0.join("restored");
        base.restore(&accepted_source).unwrap();
        let accepted = fs::read_to_string(accepted_source.join(owner.as_path())).unwrap();
        let working = fs::read_to_string(&file).unwrap();
        let offset = text.find("<v8:Digits>012").unwrap();
        let location = prepared
            .source_map()
            .file(&native)
            .unwrap()
            .locate(
                &bytes,
                offset..offset + 16,
                Some(OriginTexts {
                    accepted: &accepted,
                    working: Some(&working),
                }),
            )
            .unwrap();
        assert_eq!(location.authority, InputAuthority::Working);
        assert_eq!(location.uuid, Some(uuid('3')));
        let range = &working[location.range.unwrap()];
        assert!(range.contains("'012' # precision") && range.contains("'0003' # scale"));
        let reader = SourceReader::open(&source).unwrap();
        let snapshot = reader
            .read_metadata(&owner, eska::project::metadata_parser::PropertiesMode::All)
            .unwrap();
        let projected = &snapshot.documents[0].descriptor;
        let parsed = eska::project::metadata_parser::parse(
            text,
            None,
            eska::project::metadata_parser::PropertiesMode::All,
        )
        .unwrap();
        for (native, source) in parsed.objects.iter().zip(&projected.objects) {
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

/// Native protection and inherited freeze restore raw type edits without authorizing reconstruction changes.
#[test]
fn protected_type_values_and_native_layout_are_anchored_to_accepted_evidence() {
    for readable in [false, true] {
        for (supported, frozen) in [(true, false), (false, true)] {
            let test = TestDir::new();
            let (source, base) = prepare(&test.0, readable, supported, frozen);
            assert!(
                !base
                    .permission(&uuid('3'), Operation::Property(PropertyName::Type))
                    .allowed,
                "readable={readable}, supported={supported}, frozen={frozen}"
            );
            let mut value = description(&source);
            value.data.number.as_mut().unwrap().digits = Some(ExactUnsigned::parse("12").unwrap());
            assert!(
                prepare_property_edit(
                    &source,
                    &base,
                    &uuid('3'),
                    PropertyName::Type,
                    PropertyValue::TypeDescription(value)
                )
                .is_err()
            );
            let file = source.join("Catalogs/Sample/object.yaml");
            let original = fs::read_to_string(&file).unwrap();
            fs::write(
                &file,
                original.replace("\"digits\": \"9\"", "\"digits\": \"12\""),
            )
            .unwrap();
            let restored =
                prepare_protected(&source, &base, &test.0.join("generations"), false).unwrap();
            assert!(
                restored
                    .diagnostics()
                    .iter()
                    .any(|item| item.uuid == Some(uuid('3'))
                        && item.operation == Operation::Property(PropertyName::Type))
            );
            assert!(
                fs::read_to_string(restored.xml().join("Catalogs/Sample.xml"))
                    .unwrap()
                    .contains("<v8:Digits>9</v8:Digits>")
            );
            assert!(prepare_protected(&source, &base, &test.0.join("strict"), true).is_err());
        }
        let test = TestDir::new();
        let (source, base) = prepare(&test.0, readable, false, false);
        let before = tree(&source);
        let mut value = description(&source);
        value.native.parts.reverse();
        assert_eq!(
            prepare_property_edit(
                &source,
                &base,
                &uuid('3'),
                PropertyName::Type,
                PropertyValue::TypeDescription(value)
            )
            .unwrap_err()
            .code,
            "source_edit_value_unsupported"
        );
        assert_eq!(tree(&source), before);
        let file = source.join("Catalogs/Sample/object.yaml");
        let original = fs::read_to_string(&file).unwrap();
        let raw = original
            .replace("\"digits\": \"9\"", "\"digits\": \"12\"")
            .replace("\"prefix\": \"v8\"", "\"prefix\": \"xs\"");
        fs::write(file, raw).unwrap();
        let restored =
            prepare_protected(&source, &base, &test.0.join("generations"), false).unwrap();
        assert!(!restored.diagnostics().is_empty());
        let native = fs::read_to_string(restored.xml().join("Catalogs/Sample.xml")).unwrap();
        assert!(native.contains("<v8:Digits>12</v8:Digits>") && !native.contains("<xs:Digits>"));
        assert!(prepare_protected(&source, &base, &test.0.join("strict"), true).is_err());
    }
}
