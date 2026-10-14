//! Current read views remain independent of disk, approved history and plain query values.

use std::{fs, ops::Range};

use super::*;
use crate::project_core::source_format::{
    QuerySlot,
    queries::test_support::{DCS, DIALOG, FORM, RESOURCE, TEMPLATE, path, source},
    yaml::{self, Step},
};

/// Capture precisely the physical inputs declared for this resource, without reconstructing query values.
fn inputs(reader: &SourceReader, native: &str) -> (String, String) {
    let resource = reader
        .manifest()
        .query_resources
        .iter()
        .find(|resource| resource.path == path(native))
        .unwrap();
    let owner = reader.owner_for_native_main(&resource.descriptor).unwrap();
    let file = reader
        .manifest()
        .files
        .iter()
        .find(|file| file.path == resource.path)
        .unwrap();
    (
        fs::read_to_string(reader.root().join(owner.as_path())).unwrap(),
        fs::read_to_string(reader.root().join(file.source_path().as_path())).unwrap(),
    )
}

/// Native coordinates come from a parsed actual element, not from equal text in another scope.
fn element_range(xml: &str, namespace: &str, name: &str) -> Range<usize> {
    roxmltree::Document::parse(xml)
        .unwrap()
        .descendants()
        .find(|node| node.has_tag_name((namespace, name)))
        .unwrap()
        .range()
}

/// Both supplied current resources remain readable after their entire owned fixture has disappeared.
#[test]
fn pure_context_survives_removed_source_tree_and_keeps_exact_identity() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let dcs = inputs(&reader, TEMPLATE);
    let form = inputs(&reader, DIALOG);
    fs::remove_dir_all(&root).unwrap();
    for (native, (owner, resource), profile, physical) in [
        (TEMPLATE, dcs, QueryProfile::Composition, RESOURCE),
        (DIALOG, form, QueryProfile::ManagedForm, FORM),
    ] {
        let view = reader
            .parse_query_context(&path(native), &owner, &resource, 1_000_000)
            .unwrap();
        assert_eq!(view.native_resource(), &path(native));
        assert_eq!(view.source(), &path(physical));
        assert_eq!(view.profile(), profile);
        assert_eq!(view.references().len(), 2);
        let declared = reader.declared_query(&view.references()[0].source).unwrap();
        assert_eq!(view.owner(), declared.owner);
        assert_eq!(view.native_descriptor(), declared.native_descriptor);
        assert_eq!(view.uuid(), declared.uuid);
        assert!(!view.reference_xml().contains("SELECT"));
    }
}

/// Dirty native scopes and consumer ranges come solely from supplied XML, including BOM and CRLF.
#[test]
fn current_composition_scopes_and_consumer_bytes_are_exact() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, resource) = inputs(&reader, TEMPLATE);
    let resource = resource
        .replace("<name>Union</name>", "<name>Изменённый</name>")
        .replace(
            "<settingsVariant/>",
            "<settingsVariant><field>Сумма &amp; Код</field></settingsVariant>",
        );
    let view = reader
        .parse_query_context(&path(TEMPLATE), &owner, &resource, 1_000_000)
        .unwrap();
    assert_eq!(view.reference_xml(), resource);
    assert_eq!(
        view.references()[0].slot,
        QuerySlot::Composition {
            datasets: vec!["Изменённый".into(), "Same".into()]
        }
    );
    let range = element_range(&resource, DCS, "field");
    assert_eq!(
        view.locate_reference_range(range.clone()).unwrap(),
        QueryAuthoringRange::Exact(range.clone())
    );
    assert_eq!(&resource[range], "<field>Сумма &amp; Код</field>");
    assert_eq!(
        view.locate_reference_range(resource.len()..resource.len())
            .unwrap(),
        QueryAuthoringRange::Exact(resource.len()..resource.len())
    );
}

/// Preserved list settings map to their exact quoted YAML scalar, while generated UI names stay unmapped.
#[test]
fn current_form_ranges_have_explicit_scalar_precision_and_real_attribute_scope() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, _) = inputs(&reader, DIALOG);
    let mut value = yaml::decode(&owner).unwrap();
    value["form"]["elements"]["5"]["name"] = "ИзменённаяТаблица".into();
    let text = yaml::encode(&value).unwrap();
    let view = reader
        .parse_query_context(&path(DIALOG), &text, &text, 1_000_000)
        .unwrap();
    assert_eq!(
        view.references()[0].slot,
        QuerySlot::DynamicList {
            attribute: crate::project_core::source_format::FormId::parse("5").unwrap()
        }
    );
    assert!(view.reference_xml().contains("ИзменённаяТаблица"));
    let range = view.references()[0].source_range();
    let QueryAuthoringRange::EnclosingNode(authoring) = view.locate_reference_range(range).unwrap()
    else {
        panic!("preserved list query reference must have a current YAML scalar");
    };
    let scalar = &text[authoring];
    assert!(scalar.starts_with('"'));
    let decoded: String = serde_json::from_str(scalar).unwrap();
    assert!(decoded.contains("<Attribute name=\"Rows\" id=\"5\">"));
    assert!(!decoded.contains("ИзменённаяТаблица"));
    let document = roxmltree::Document::parse(view.reference_xml()).unwrap();
    let name = document
        .descendants()
        .find(|node| node.tag_name().name() == "Table")
        .unwrap()
        .attributes()
        .find(|attribute| attribute.name() == "name")
        .unwrap()
        .range_value();
    assert_eq!(
        view.locate_reference_range(name).unwrap(),
        QueryAuthoringRange::Unmapped
    );
    assert_eq!(
        view.locate_reference_range(0..view.reference_xml().len())
            .unwrap(),
        QueryAuthoringRange::Unmapped
    );
}

/// Block scalars use their complete current CST span; decoded offsets are deliberately not guessed.
#[test]
fn literal_yaml_scalar_is_enclosing_and_folded_yaml_is_rejected() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, _) = inputs(&reader, DIALOG);
    let (value, tree) = yaml::decode_with_tree(&owner).unwrap();
    let parts = value["form"]["native"]["root"].as_array().unwrap();
    let (index, native) = parts
        .iter()
        .enumerate()
        .find_map(|(index, part)| {
            part["value"]
                .as_str()
                .filter(|value| value.contains("<Attributes>"))
                .map(|value| (index, value))
        })
        .unwrap();
    let scalar = yaml::range_at(
        &tree,
        &[
            Step::Key("form"),
            Step::Key("native"),
            Step::Key("root"),
            Step::Index(index),
            Step::Key("value"),
        ],
    )
    .unwrap();
    let mut block = String::from("|\n");
    for line in native.replace("\r\n", "\n").lines() {
        block.push_str("          ");
        block.push_str(line);
        block.push('\n');
    }
    let mut text = owner.clone();
    text.replace_range(scalar.clone(), &block);
    let view = reader
        .parse_query_context(&path(DIALOG), &text, &text, 1_000_000)
        .unwrap();
    let QueryAuthoringRange::EnclosingNode(range) = view
        .locate_reference_range(view.references()[0].source_range())
        .unwrap()
    else {
        panic!("literal block must retain enclosing scalar coordinates");
    };
    assert!(text[range].starts_with('|'));
    let mut folded = owner;
    folded.replace_range(scalar, &block.replacen('|', ">", 1));
    assert_eq!(
        reader
            .parse_query_context(&path(DIALOG), &folded, &folded, 1_000_000)
            .unwrap_err()
            .code,
        "source_yaml_folding_forbidden"
    );
}

/// Invalid supplied input cannot inherit saved owner identity, an old resource or a different link set.
#[test]
fn current_identity_type_links_and_same_file_bytes_are_strict() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, resource) = inputs(&reader, TEMPLATE);
    let query = &reader
        .manifest()
        .query_resources
        .iter()
        .find(|resource| resource.path == path(TEMPLATE))
        .unwrap()
        .sources[0];
    let uuid = reader.declared_query(query).unwrap().uuid.as_str();
    let changed_uuid = owner.replace(uuid, "80000000-0000-4000-8000-000000000099");
    assert_eq!(
        reader
            .parse_query_context(&path(TEMPLATE), &changed_uuid, &resource, 1_000_000)
            .unwrap_err()
            .code,
        "source_identity_mismatch"
    );
    assert_eq!(
        reader
            .parse_query_context(
                &path(TEMPLATE),
                &owner.replace("DataCompositionSchema", "BinaryData"),
                &resource,
                1_000_000
            )
            .unwrap_err()
            .code,
        "source_query_map_invalid"
    );
    assert!(
        reader
            .parse_query_context(
                &path(TEMPLATE),
                &owner,
                &resource.replace(query.as_str(), "Queries/Foreign.sdbl"),
                1_000_000
            )
            .is_err()
    );
    let mut inline = resource.clone();
    inline.replace_range(
        element_range(&resource, DCS, "query"),
        "<query>SELECT 1</query>",
    );
    assert!(
        reader
            .parse_query_context(&path(TEMPLATE), &owner, &inline, 1_000_000)
            .is_err()
    );
    let (form, _) = inputs(&reader, DIALOG);
    assert_eq!(
        reader
            .parse_query_context(&path(DIALOG), &form, &(form.clone() + "\n"), 1_000_000)
            .unwrap_err()
            .code,
        "source_query_context_input_mismatch"
    );
    assert!(
        reader
            .parse_query_context(&path(TEMPLATE), &owner, "<Other/>", 1_000_000)
            .is_err()
    );
    assert_eq!(
        reader
            .parse_query_context(&path("Undeclared.xml"), &owner, &resource, 1_000_000)
            .unwrap_err()
            .code,
        "source_query_undeclared"
    );
}

/// All input and output spans are bounded before they can be used as authoring locations.
#[test]
fn query_context_rejects_input_limits_and_invalid_byte_ranges() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, resource) = inputs(&reader, TEMPLATE);
    assert_eq!(
        reader
            .parse_query_context(&path(TEMPLATE), &owner, &resource, 1)
            .unwrap_err()
            .code,
        "source_document_too_large"
    );
    let view = reader
        .parse_query_context(&path(TEMPLATE), &owner, &resource, 1_000_000)
        .unwrap();
    for range in [Range { start: 2, end: 1 }, 0..resource.len() + 1, 1..2] {
        assert_eq!(
            view.locate_reference_range(range).unwrap_err().code,
            "source_location_invalid"
        );
    }
}

/// Current CF/CFE and external layouts share storage views without granting an external semantic base.
#[test]
fn four_native_profiles_keep_declared_resource_ownership_without_disk_reads() {
    use crate::project_core::source_format::{EXTERNAL_LAYOUT_VERSION, import_designer_queries};
    use crate::{query_creation_fixture as fixture, test_support::TestDir};

    for kind in fixture::TYPES {
        let directory = TestDir::new();
        let native = fixture::native(&directory.0, kind);
        let root = directory.0.join("queries");
        import_designer_queries(&native.input, &root).unwrap();
        let reader = SourceReader::open(&root).unwrap();
        assert_eq!(
            reader.manifest().schema_version,
            if matches!(kind, "processing" | "report") {
                EXTERNAL_LAYOUT_VERSION
            } else {
                QUERY_LAYOUT_VERSION
            }
        );
        let native_schema = native.schema.to_str().unwrap();
        let native_dialog = native.dialog.to_str().unwrap();
        let dcs = inputs(&reader, native_schema);
        let form = inputs(&reader, native_dialog);
        fs::remove_dir_all(&root).unwrap();
        for (resource, (owner, text), profile) in [
            (native_schema, dcs, QueryProfile::Composition),
            (native_dialog, form, QueryProfile::ManagedForm),
        ] {
            let view = reader
                .parse_query_context(&path(resource), &owner, &text, 1_000_000)
                .unwrap();
            assert_eq!(view.profile(), profile);
            assert_eq!(view.references().len(), 1);
            let declared = reader.declared_query(&view.references()[0].source).unwrap();
            assert_eq!(view.native_descriptor(), declared.native_descriptor);
            assert_eq!(view.uuid(), declared.uuid);
            let location = view
                .locate_reference_range(view.references()[0].source_range())
                .unwrap();
            assert!(matches!(
                (profile, location),
                (QueryProfile::Composition, QueryAuthoringRange::Exact(_))
                    | (
                        QueryProfile::ManagedForm,
                        QueryAuthoringRange::EnclosingNode(_)
                    )
            ));
        }
    }
}
