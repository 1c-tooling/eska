//! Empty reference values do not require editing inactive native details or inventing new bindings.

use super::*;
use eska::project_core::{
    metadata_parser::PropertiesMode,
    source_format::{
        InputAuthority, ObjectSource, OriginTexts, SourceReader, prepare_property_edit,
        prepare_protected,
    },
};
use yaml_edit::{AsYaml, YamlFile, YamlNode};

/// Change the authoritative value only; readable service prefixes remain untouched for the manual author.
fn clear(source: &Path, readable: bool) -> (SourcePath, String, String) {
    let relative = SourcePath::parse("Catalogs/Sample/object.yaml").unwrap();
    let path = source.join(relative.as_path());
    let accepted = fs::read_to_string(&path).unwrap();
    let wire = serde_json::to_value(ObjectSource::from_yaml(&accepted).unwrap()).unwrap();
    let parsed = YamlFile::parse(&accepted);
    assert!(parsed.ok());
    let mut node = YamlNode::Mapping(parsed.tree().document().unwrap().as_mapping().unwrap());
    for key in ["document", "descriptor", "object"] {
        node = node.as_mapping().unwrap().get(key).unwrap();
    }
    if readable {
        node = node.as_mapping().unwrap().get("properties").unwrap();
        node = node.as_mapping().unwrap().get("Owners").unwrap();
    } else {
        let sections = wire["document"]["descriptor"]["object"]["sections"]
            .as_array()
            .unwrap();
        let section = sections
            .iter()
            .position(|section| section["kind"] == "properties")
            .unwrap();
        let property = sections[section]["items"]
            .as_array()
            .unwrap()
            .iter()
            .position(|property| property["name"] == "Owners")
            .unwrap();
        node = node.as_mapping().unwrap().get("sections").unwrap();
        node = node.as_sequence().unwrap().get(section).unwrap();
        node = node.as_mapping().unwrap().get("items").unwrap();
        node = node.as_sequence().unwrap().get(property).unwrap();
        node = node.as_mapping().unwrap().get("value").unwrap();
    }
    let range = node.as_node().unwrap().text_range();
    let range = usize::from(range.start())..usize::from(range.end());
    let newline = if accepted[range.clone()].ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let mut working = accepted.clone();
    working.replace_range(range, &format!("{{\"state\": \"empty\"}}{newline}"));
    let working = format!("# Keep annotation\r\n{}", working.replace('\n', "\r\n"));
    fs::write(path, &working).unwrap();
    (relative, accepted, working)
}

/// Both source envelopes clear a previously bound list while preserving inputs, maps and semantic projections.
#[test]
fn clearing_an_accepted_list_keeps_its_authoring_bytes_and_working_location() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = super::references::prepare(&test.0, readable, false);
        let pinned = super::baseline::tree(&test.0.join("accepted"));
        let manifest = fs::read(source.join("source.json")).unwrap();
        let (path, accepted, working) = clear(&source, readable);
        let before = super::baseline::tree(&source);
        let reader = SourceReader::open(&source).unwrap();
        for mode in [PropertiesMode::All, PropertiesMode::Summary] {
            assert_eq!(
                reader.read_metadata(&path, mode).unwrap(),
                reader.parse_metadata(&path, &working, mode).unwrap()
            );
        }
        let output = test.0.join("generations");
        let prepared = prepare_protected(&source, &base, &output, true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        assert_eq!(
            prepared.generation(),
            prepare_protected(&source, &base, &output, true)
                .unwrap()
                .generation()
        );
        let native = fs::read(prepared.xml().join("Catalogs/Sample.xml")).unwrap();
        let xml = std::str::from_utf8(&native).unwrap();
        let start = xml.find("<Owners/>").unwrap();
        assert_eq!(
            Descriptor::from_xml(xml)
                .unwrap()
                .object
                .property(PropertyName::Owners),
            Some(&PropertyValue::Empty)
        );
        let mapped = prepared
            .source_map()
            .file(&SourcePath::parse("Catalogs/Sample.xml").unwrap())
            .unwrap();
        let location = mapped
            .locate(
                &native,
                start..start + 1,
                Some(OriginTexts {
                    accepted: &accepted,
                    working: Some(&working),
                }),
            )
            .unwrap();
        assert_eq!(location.authority, InputAuthority::Working);
        assert!(working[location.range.unwrap()].contains("empty"));
        assert_eq!(super::baseline::tree(&source), before);
        assert_eq!(super::baseline::tree(&test.0.join("accepted")), pinned);
        assert_eq!(fs::read(source.join("source.json")).unwrap(), manifest);
    }
}

/// A frozen list cannot be cleared, and a changed inactive prefix still requires an explicit exclusion.
#[test]
fn clearing_cannot_bypass_freeze_or_hide_native_binding_edits() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = super::references::prepare(&test.0, readable, true);
        clear(&source, readable);
        let output = test.0.join("generations");
        let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(failure.error.code, "source_changes_excluded");
        assert!(!output.exists());
        let restored = prepare_protected(&source, &base, &output, false).unwrap();
        let xml = fs::read_to_string(restored.xml().join("Catalogs/Sample.xml")).unwrap();
        assert!(xml.contains("Catalog.Other"));
        assert!(!xml.contains("<Owners/>"));
    }
    let test = TestDir::new();
    let (source, base) = super::references::prepare(&test.0, true, false);
    let (path, _, working) = clear(&source, true);
    let changed = working.replace("\"item_prefix\": \"xr\"", "\"item_prefix\": \"untrusted\"");
    assert_ne!(working, changed);
    fs::write(source.join(path.as_path()), &changed).unwrap();
    let output = test.0.join("generations");
    assert_eq!(
        prepare_protected(&source, &base, &output, true)
            .unwrap_err()
            .error
            .code,
        "source_changes_excluded"
    );
    assert!(!output.exists());
    let restored = prepare_protected(&source, &base, &output, false).unwrap();
    assert!(!restored.diagnostics().is_empty());
    let xml = fs::read_to_string(restored.xml().join("Catalogs/Sample.xml")).unwrap();
    assert!(xml.contains("<Owners/>") && !xml.contains("untrusted:"));
    assert_eq!(
        fs::read_to_string(source.join(path.as_path())).unwrap(),
        changed
    );
}

/// Canonical Empty transport is also valid; inactive prefixes are optional, never a required editable value.
#[test]
fn canonical_empty_source_without_inactive_bindings_remains_supported() {
    let test = TestDir::new();
    let (source, base) = super::references::prepare(&test.0, true, false);
    let (path, _, working) = clear(&source, true);
    let owner = ObjectSource::from_yaml(&working).unwrap();
    let canonical = owner.to_yaml().unwrap();
    fs::write(source.join(path.as_path()), &canonical).unwrap();
    let prepared = prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
    assert!(prepared.diagnostics().is_empty());
    assert!(
        fs::read_to_string(prepared.xml().join("Catalogs/Sample.xml"))
            .unwrap()
            .contains("<Owners/>")
    );
}

/// The public edit plan changes the value subtree only and refuses to discard annotations on removed items.
#[test]
fn point_edit_clears_an_existing_list_without_touching_its_native_slot() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = super::references::prepare(&test.0, readable, false);
        let (path, accepted, expected) = clear(&source, readable);
        let original = format!("# Keep annotation\r\n{}", accepted.replace('\n', "\r\n"));
        fs::write(source.join(path.as_path()), &original).unwrap();
        let id = super::baseline::uuid('2');
        let edit = prepare_property_edit(
            &source,
            &base,
            &id,
            PropertyName::Owners,
            PropertyValue::Empty,
        )
        .unwrap();
        assert_eq!(edit.updated(), expected);
        edit.publish(&base).unwrap();
        assert!(
            prepare_protected(&source, &base, &test.0.join("generations"), true)
                .unwrap()
                .diagnostics()
                .is_empty()
        );
        let annotated = original.replace(
            "\"Catalog.Sample\"",
            "\"Catalog.Sample\" # keep member annotation",
        );
        fs::write(source.join(path.as_path()), &annotated).unwrap();
        assert_eq!(
            prepare_property_edit(
                &source,
                &base,
                &id,
                PropertyName::Owners,
                PropertyValue::Empty
            )
            .unwrap_err()
            .code,
            "source_edit_value_unsupported"
        );
        assert_eq!(
            fs::read_to_string(source.join(path.as_path())).unwrap(),
            annotated
        );
    }
}
