use super::*;
use crate::project_core::source_format::{Descriptor, LocalizedText, NativeDocument, Section};

/// Parse the public owner family without creating a project or inventing platform acceptance.
fn owner() -> ObjectSource {
    ObjectSource {
        document: NativeDocument {
            path: SourcePath::parse("Catalogs/Sample.xml").expect("catalog path"),
            descriptor: Descriptor::from_xml(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/eska-format/designer/Catalogs/Sample.xml"
            )))
            .expect("catalog"),
        },
        nested: vec![NativeDocument {
            path: SourcePath::parse("Catalogs/Sample/Forms/Main.xml").expect("form path"),
            descriptor: Descriptor::from_xml(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/eska-format/designer/Catalogs/Sample/Forms/Main.xml"
            )))
            .expect("form"),
        }],
    }
}

/// Fingerprint exact authoring bytes, including comments and CRLF preceding modeled values.
fn input(text: &str) -> InputFile {
    InputFile {
        path: SourcePath::parse("Catalogs/Sample/object.yaml").expect("owner path"),
        bytes: u64::try_from(text.len()).expect("small owner"),
        sha256: files::hex(&Sha256::digest(text.as_bytes())),
    }
}

/// Construct a detached origin record; no source permission or protected proof is implied.
fn mapped(
    document: &NativeDocument,
    native: &str,
    accepted: &str,
    working: Option<&str>,
) -> MappedFile {
    MappedFile {
        path: document.path.clone(),
        bytes: u64::try_from(native.len()).expect("small native descriptor"),
        sha256: files::hex(&Sha256::digest(native.as_bytes())),
        origin: FileOrigin::Metadata {
            uuid: document.descriptor.object.uuid.clone(),
            accepted: input(accepted),
            working: working.map(input),
            uses_working_values: working.is_some(),
        },
    }
}

/// Locate an existing XML substring only to choose the native diagnostic span in this fixture.
fn span(native: &str, needle: &str) -> Range<usize> {
    let start = native.find(needle).expect("native diagnostic target");
    start..start + needle.len()
}

/// Change an existing typed property while retaining accepted structural positions.
fn comment(document: &mut NativeDocument, value: &str) {
    for section in &mut document.descriptor.object.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    name: PropertyName::Comment,
                    value: candidate,
                } = property
                {
                    *candidate = PropertyValue::Text(value.to_owned());
                }
            }
        }
    }
}

#[test]
/// Nested metadata and opaque `QName` payloads map to actual YAML nodes, not equal strings in comments.
fn accepted_locations_follow_inline_objects_preserved_properties_and_external_documents() {
    let owner = owner();
    let accepted = format!(
        "# Count: not the declared Name\r\n{}",
        owner.to_yaml().expect("YAML").replace('\n', "\r\n")
    );
    let native = owner.document.descriptor.to_xml().expect("native XML");
    let file = mapped(&owner.document, &native, &accepted, None);
    let texts = Some(OriginTexts {
        accepted: &accepted,
        working: None,
    });
    let nested = file
        .locate(
            native.as_bytes(),
            span(&native, "<Name>Count</Name>"),
            texts,
        )
        .expect("nested location");
    assert_eq!(nested.authority, InputAuthority::Accepted);
    assert_eq!(
        nested.uuid.expect("inline identity").as_str(),
        "50000000-0000-4000-8000-000000000001"
    );
    assert_eq!(
        accepted.get(nested.range.expect("Name coordinate")),
        Some("\"Count\"")
    );
    let opaque = file
        .locate(
            native.as_bytes(),
            span(&native, "123456789012345678901234567890.0001"),
            texts,
        )
        .expect("opaque coordinate");
    assert!(
        accepted
            .get(opaque.range.expect("fragment coordinate"))
            .expect("fragment text")
            .contains("FutureProperty")
    );
    let form = &owner.nested[0];
    let native = form.descriptor.to_xml().expect("form XML");
    let file = mapped(form, &native, &accepted, None);
    let location = file
        .locate(native.as_bytes(), span(&native, "<Name>Main</Name>"), texts)
        .expect("external descriptor coordinate");
    assert_eq!(
        accepted.get(location.range.expect("form Name")),
        Some("\"Main\"")
    );
    let object = file
        .locate(
            native.as_bytes(),
            span(&native, form.descriptor.object.uuid.as_str()),
            texts,
        )
        .expect("object coordinate");
    assert!(
        accepted
            .get(object.range.expect("object range"))
            .expect("object YAML")
            .contains(form.descriptor.object.uuid.as_str())
    );
}

#[test]
/// A selected Unicode working value follows its real reordered section, while structure stays accepted.
fn working_property_coordinates_use_exact_bytes_and_accepted_name_coordinates() {
    let accepted_owner = owner();
    let accepted = accepted_owner.to_yaml().expect("accepted YAML");
    let mut selected = accepted_owner;
    comment(&mut selected.document, "Комментарий Ё🦀");
    let mut working_owner = selected.clone();
    working_owner
        .document
        .descriptor
        .object
        .sections
        .rotate_left(1);
    let working = format!(
        "# Комментарий Ё🦀 is a comment\r\n{}",
        working_owner
            .to_yaml()
            .expect("working YAML")
            .replace('\n', "\r\n")
    );
    let native = selected.document.descriptor.to_xml().expect("native XML");
    let file = mapped(&selected.document, &native, &accepted, Some(&working));
    let texts = Some(OriginTexts {
        accepted: &accepted,
        working: Some(&working),
    });
    let location = file
        .locate(native.as_bytes(), span(&native, "Комментарий Ё🦀"), texts)
        .expect("working coordinate");
    assert_eq!(location.authority, InputAuthority::Working);
    assert_eq!(location.input, input(&working));
    assert_eq!(
        working.get(location.range.expect("working value range")),
        Some("\"Комментарий Ё🦀\"")
    );
    let location = file
        .locate(
            native.as_bytes(),
            span(&native, "<Name>Sample</Name>"),
            texts,
        )
        .expect("accepted identity coordinate");
    assert_eq!(location.authority, InputAuthority::Accepted);
    assert_eq!(
        accepted.get(location.range.expect("accepted Name")),
        Some("\"Sample\"")
    );
    let offset = native.find('🦀').expect("UTF-8 character");
    assert_eq!(
        file.locate(native.as_bytes(), offset + 1..offset + 2, texts)
            .expect_err("split UTF-8 scalar")
            .code,
        "source_map_span_invalid"
    );
}

#[test]
/// Restored identity never receives a fabricated coordinate in a differently identified working object.
fn changed_working_identity_yields_file_level_selected_origin() {
    let accepted_owner = owner();
    let accepted = accepted_owner.to_yaml().expect("accepted YAML");
    let mut selected = accepted_owner;
    comment(&mut selected.document, "Selected value");
    let mut working_owner = selected.clone();
    working_owner.document.descriptor.object.uuid =
        NativeUuid::parse("20000000-0000-4000-8000-000000000099").expect("changed identity");
    let working = working_owner.to_yaml().expect("working YAML");
    let native = selected.document.descriptor.to_xml().expect("native XML");
    let file = mapped(&selected.document, &native, &accepted, Some(&working));
    let location = file
        .locate(
            native.as_bytes(),
            span(&native, "Selected value"),
            Some(OriginTexts {
                accepted: &accepted,
                working: Some(&working),
            }),
        )
        .expect("selected origin");
    assert_eq!(location.authority, InputAuthority::Working);
    assert_eq!(location.range, None);
    assert_eq!(location.input, input(&working));
}

#[test]
/// Localized and nil values map to their YAML value mapping instead of a fabricated text scalar.
fn structured_values_keep_their_actual_yaml_mapping_range() {
    let original = owner();
    let accepted = original.to_yaml().expect("accepted YAML");
    let cases = [
        (
            PropertyName::Comment,
            PropertyValue::Nil,
            "<Comment ",
            "nil",
        ),
        (
            PropertyName::Synonym,
            PropertyValue::Localized(vec![LocalizedText {
                language: "ru".to_owned(),
                text: "Ё🦀 & первая\r\nвторая строка".to_owned(),
            }]),
            "<Synonym>",
            "localized",
        ),
    ];
    for (name, value, needle, kind) in cases {
        let mut selected = original.clone();
        for section in &mut selected.document.descriptor.object.sections {
            if let Section::Properties(properties) = section {
                for property in properties {
                    if let Property::Typed {
                        name: candidate,
                        value: target,
                    } = property
                        && *candidate == name
                    {
                        *target = value.clone();
                    }
                }
            }
        }
        let working = selected.to_yaml().expect("working YAML");
        let native = selected.document.descriptor.to_xml().expect("native XML");
        let file = mapped(&selected.document, &native, &accepted, Some(&working));
        let location = file
            .locate(
                native.as_bytes(),
                span(&native, needle),
                Some(OriginTexts {
                    accepted: &accepted,
                    working: Some(&working),
                }),
            )
            .expect("structured value coordinate");
        assert_eq!(location.authority, InputAuthority::Working);
        let range = location.range.expect("value mapping");
        let line_start = working[..range.start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let indent = range.start - line_start;
        let fragment = working.get(range).expect("mapping bytes");
        let mut lines = fragment.lines();
        let mut standalone = lines
            .next()
            .expect("mapping starts with a field")
            .to_owned();
        for line in lines {
            standalone.push('\n');
            standalone.push_str(line.get(indent..).expect("relative value indentation"));
        }
        let actual = yaml::decode(&standalone).expect("complete YAML value mapping");
        assert_eq!(actual["state"], kind);
        assert_eq!(
            actual,
            serde_json::to_value(&value).expect("selected value")
        );
    }
}

#[test]
/// Coordinates stop when either the native descriptor or requested authoring text changes.
fn changed_inputs_missing_selected_text_and_invalid_bounds_fail_without_fallback() {
    let accepted_owner = owner();
    let accepted = accepted_owner.to_yaml().expect("accepted YAML");
    let mut selected = accepted_owner;
    comment(&mut selected.document, "Selected value");
    let working = selected.to_yaml().expect("working YAML");
    let native = selected.document.descriptor.to_xml().expect("native XML");
    let file = mapped(&selected.document, &native, &accepted, Some(&working));
    let target = span(&native, "Selected value");
    let texts = Some(OriginTexts {
        accepted: &accepted,
        working: Some(&working),
    });
    assert_eq!(
        file.locate(b"stale XML", target.clone(), texts)
            .expect_err("changed native")
            .code,
        "source_map_input_changed"
    );
    assert_eq!(
        file.locate(
            native.as_bytes(),
            target.clone(),
            Some(OriginTexts {
                accepted: "changed YAML",
                working: Some(&working)
            })
        )
        .expect_err("changed accepted")
        .code,
        "source_map_input_changed"
    );
    assert_eq!(
        file.locate(
            native.as_bytes(),
            target.clone(),
            Some(OriginTexts {
                accepted: &accepted,
                working: Some("changed working YAML")
            })
        )
        .expect_err("changed working")
        .code,
        "source_map_input_changed"
    );
    assert_eq!(
        file.locate(
            native.as_bytes(),
            target,
            Some(OriginTexts {
                accepted: &accepted,
                working: None
            })
        )
        .expect_err("missing selected text")
        .code,
        "source_map_origin_text_missing"
    );
    assert_eq!(
        file.locate(native.as_bytes(), native.len()..native.len() + 1, texts)
            .expect_err("out of bounds")
            .code,
        "source_map_span_invalid"
    );
    let prolog = file
        .locate(native.as_bytes(), 0..0, texts)
        .expect("file-level prolog location");
    assert_eq!(prolog.authority, InputAuthority::Accepted);
    assert_eq!(prolog.range, None);
}

#[test]
/// BOM, CRLF and byte-preserved resource offsets remain identical for either selected authority.
fn payload_locations_keep_exact_module_bytes_and_do_not_require_yaml() {
    let bytes = b"\xef\xbb\xbfProcedure Sample()\r\nEndProcedure\r\n";
    for authority in [InputAuthority::Working, InputAuthority::Accepted] {
        let path = SourcePath::parse("Ext/ManagedApplicationModule.bsl").expect("module path");
        let input = InputFile {
            path: path.clone(),
            bytes: u64::try_from(bytes.len()).expect("small module"),
            sha256: files::hex(&Sha256::digest(bytes)),
        };
        let file = MappedFile {
            path,
            bytes: input.bytes,
            sha256: input.sha256.clone(),
            origin: FileOrigin::Payload {
                authority,
                input: input.clone(),
                module: true,
                owner: None,
            },
        };
        let location = file.locate(bytes, 3..12, None).expect("module coordinate");
        assert_eq!(location.authority, authority);
        assert_eq!(location.input, input);
        assert_eq!(location.range, Some(3..12));
        assert_eq!(
            file.locate(b"changed module", 0..0, None)
                .expect_err("changed module")
                .code,
            "source_map_input_changed"
        );
    }
}

#[test]
/// A detached DTO cannot infer metadata coordinates from a foreign root kind or unbounded XML.
fn native_root_must_match_accepted_descriptor_and_respect_existing_xml_limits() {
    let owner = owner();
    let accepted = owner.to_yaml().expect("accepted YAML");
    let native = owner.document.descriptor.to_xml().expect("native XML");
    let foreign = native
        .replace("<Catalog uuid", "<Document uuid")
        .replace("</Catalog>", "</Document>");
    let file = mapped(&owner.document, &foreign, &accepted, None);
    let texts = Some(OriginTexts {
        accepted: &accepted,
        working: None,
    });
    assert_eq!(
        file.locate(
            foreign.as_bytes(),
            span(&foreign, "<Name>Sample</Name>"),
            texts
        )
        .expect_err("foreign metadata root")
        .code,
        "source_map_origin_mismatch"
    );
    let deep = format!("{}{}", "<node>".repeat(65), "</node>".repeat(65));
    let file = mapped(&owner.document, &deep, &accepted, None);
    assert_eq!(
        file.locate(deep.as_bytes(), 0..0, texts)
            .expect_err("depth limit precedes XML parsing")
            .code,
        "source_map_native_text_invalid"
    );
}
