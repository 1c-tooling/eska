//! Owned project fixtures compare against an independent old-layout export and preserve refused outputs.

use std::fs;

use crate::project_core::source_format::queries::test_support::*;
use crate::{
    project_core::source_format::{
        AcceptedBase, NativeQueryDocument, QUERY_LAYOUT_VERSION, QuerySlot, SourceReader,
        accept_source, export_designer, files::Inventory, import_designer_forms,
        import_designer_queries, prepare_form_layout, prepare_protected, prepare_readable_layout,
    },
    test_support::TestDir,
};

/// Query extraction changes authoring structure, while every generated native byte matches the old layout.
#[test]
fn query_project_import_export_and_repeat_preserve_control_bytes() {
    let (directory, native, root) = source();
    let input = Inventory::read(&native).unwrap();
    let before = Inventory::read(&root).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    assert_eq!(reader.manifest().schema_version, QUERY_LAYOUT_VERSION);
    assert_eq!(reader.manifest().query_resources.len(), 2);
    assert_eq!(reader.query_paths().count(), 4);
    assert!(!root.join(DIALOG).exists());
    assert!(!root.join("Catalogs/Sample/Forms/Main/Form.xml").exists());
    assert!(
        !fs::read_to_string(root.join(FORM))
            .unwrap()
            .contains("// list")
    );
    assert!(
        !fs::read_to_string(root.join(RESOURCE))
            .unwrap()
            .contains("// union")
    );
    assert_eq!(
        fs::read(root.join("Catalogs/Sample/Templates/Opaque/Template.xml")).unwrap(),
        [0, 255, 10, 13]
    );
    assert_eq!(
        reader.read_query_text(&path(QUERY), 1_000_000).unwrap(),
        "SELECT 2;\r\n// end"
    );
    assert_eq!(
        reader.read_query_text(&path(LIST), 1_000_000).unwrap(),
        "// list\nSELECT \"x\""
    );
    assert_eq!(
        reader
            .read_form_dialog_text(&path(FORM), 1_000_000)
            .unwrap(),
        form()
    );
    let old = directory.0.join("old");
    import_designer_forms(&native, &old).unwrap();
    let control = directory.0.join("control");
    export_designer(&old, &control).unwrap();
    let output = directory.0.join("output");
    export_designer(&root, &output).unwrap();
    assert_eq!(
        Inventory::read(&output).unwrap(),
        Inventory::read(&control).unwrap()
    );
    export_designer(&root, &output).unwrap();
    assert_eq!(
        fs::read(output.join(TEMPLATE)).unwrap(),
        schema().as_bytes()
    );
    assert_eq!(fs::read(output.join(DIALOG)).unwrap(), form().as_bytes());
    assert_eq!(Inventory::read(&native).unwrap(), input);
    assert_eq!(Inventory::read(&root).unwrap(), before);
}

/// A saved plain edit changes only its target native resource; source XML and provenance remain untouched.
#[test]
fn one_sdbl_edit_changes_one_native_query_property() {
    let (directory, _, root) = source();
    let original = directory.0.join("original");
    export_designer(&root, &original).unwrap();
    let before = Inventory::read(&root).unwrap();
    let text = "// new\nSELECT \"<&>\";\r\nSELECT 3";
    fs::write(root.join(QUERY), text).unwrap();
    let after = Inventory::read(&root).unwrap();
    let changed: Vec<_> = after
        .files
        .iter()
        .filter(|(path, entry)| before.files.get(*path) != Some(*entry))
        .map(|(path, _)| path)
        .collect();
    assert_eq!(changed, [&path(QUERY)]);
    let output = directory.0.join("changed");
    export_designer(&root, &output).unwrap();
    let control = Inventory::read(&original).unwrap();
    let generated = Inventory::read(&output).unwrap();
    let changed: Vec<_> = generated
        .files
        .iter()
        .filter(|(path, entry)| control.files.get(*path) != Some(*entry))
        .map(|(path, _)| path)
        .collect();
    assert_eq!(changed, [&path(TEMPLATE)]);
    let xml = fs::read_to_string(output.join(TEMPLATE)).unwrap();
    assert!(!xml.contains("urn:eska:query:1"));
    let document = NativeQueryDocument::from_xml(&xml).unwrap();
    assert_eq!(document.queries()[1].text, text);
    assert_eq!(document.queries()[0].text, "// union\nSELECT 1");
}

/// Opening and addressed reads do not require neighboring values; full materialization still requires all files.
#[test]
fn query_reader_exposes_native_identity_and_reads_only_addressed_inputs() {
    let (directory, _, root) = source();
    fs::remove_file(root.join(QUERY)).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let query = reader.declared_query(&path(QUERY)).unwrap();
    assert_eq!(
        query.owner,
        &path("Catalogs/Sample/Templates/QuerySchema/template.yaml")
    );
    assert_eq!(
        query.native_descriptor,
        &path("Catalogs/Sample/Templates/QuerySchema.xml")
    );
    assert_eq!(query.uuid.as_str(), "80000000-0000-4000-8000-000000000001");
    assert!(
        reader
            .read_query_resource(query.native_resource, 1_000_000)
            .is_ok()
    );
    assert_eq!(
        reader.read_query_text(&path(LIST), 1_000_000).unwrap(),
        "// list\nSELECT \"x\""
    );
    assert!(reader.read_query_text(&path(LIST), 1).is_err());
    assert!(reader.read_query_text(&path(QUERY), 1_000_000).is_err());
    assert_eq!(
        reader
            .read_query_text(
                &path("Catalogs/Sample/Forms/Main/Queries/Attribute_7.sdbl"),
                1_000_000
            )
            .unwrap_err()
            .code,
        "source_query_undeclared"
    );
    let references = reader
        .read_query_resource(&path(DIALOG), 1_000_000)
        .unwrap();
    assert_eq!(
        references.references()[0].slot,
        QuerySlot::DynamicList {
            attribute: crate::project_core::source_format::FormId::parse("5").unwrap()
        }
    );
    let output = directory.0.join("refused");
    assert!(export_designer(&root, &output).is_err());
    assert!(!output.exists());
}

/// Missing/extra values and inline fallback refuse before publication and preserve a previous successful result.
#[test]
fn refused_query_exports_leave_previous_generation_and_source_bytes_unchanged() {
    let (directory, _, root) = source();
    let output = directory.0.join("output");
    export_designer(&root, &output).unwrap();
    let accepted = Inventory::read(&output).unwrap();
    let resource = fs::read_to_string(root.join(RESOURCE)).unwrap();
    let malformed = resource.replace("Query_Same.sdbl", "Missing.sdbl");
    fs::write(root.join(RESOURCE), &malformed).unwrap();
    assert!(export_designer(&root, &output).is_err());
    assert_eq!(Inventory::read(&output).unwrap(), accepted);
    assert_eq!(fs::read_to_string(root.join(RESOURCE)).unwrap(), malformed);
    fs::write(root.join(RESOURCE), resource).unwrap();
    fs::write(
        root.join("Catalogs/Sample/Templates/QuerySchema/Queries/Extra.sdbl"),
        "SELECT 0",
    )
    .unwrap();
    assert_eq!(
        export_designer(&root, &output).unwrap_err().code,
        "source_file_map_mismatch"
    );
    assert_eq!(Inventory::read(&output).unwrap(), accepted);
    fs::remove_file(root.join("Catalogs/Sample/Templates/QuerySchema/Queries/Extra.sdbl")).unwrap();
    fs::write(root.join(RESOURCE), schema()).unwrap();
    assert!(export_designer(&root, &output).is_err());
    assert_eq!(Inventory::read(&output).unwrap(), accepted);
}

/// New provenance cannot be injected into old schemas or point at another native owner or value.
#[test]
fn query_reverse_map_rejects_foreign_resources_and_physical_aliases() {
    let (_directory, _, root) = source();
    let accepted = SourceReader::open(&root).unwrap().manifest().clone();
    let mut invalid_maps = Vec::new();
    let mut old = accepted.clone();
    old.schema_version = 4;
    invalid_maps.push(old);
    let mut owner = accepted.clone();
    owner.query_resources[0].descriptor = path("Catalogs/Sample.xml");
    invalid_maps.push(owner);
    let mut location = accepted.clone();
    location.query_resources[0].path = path("CommonModules/Tools/Ext/Module.bsl");
    invalid_maps.push(location);
    let mut alias = accepted.clone();
    let source = alias.query_resources[0].sources[0].clone();
    alias.query_resources[0].sources.push(source);
    invalid_maps.push(alias);
    let mut foreign = accepted.clone();
    foreign.query_resources[0].sources[0] =
        path("Catalogs/Sample/Templates/Foreign/Queries/Test.sdbl");
    invalid_maps.push(foreign);
    let mut duplicate = accepted.clone();
    duplicate
        .query_resources
        .push(duplicate.query_resources[0].clone());
    invalid_maps.push(duplicate);
    for manifest in invalid_maps {
        assert!(manifest.validate().is_err());
    }
    let mut unsupported = accepted;
    unsupported.schema_version = 6;
    assert_eq!(
        unsupported.validate().unwrap_err().code,
        "source_schema_unsupported"
    );
}

/// Reserved names and separators become injective portable filenames; stable links follow dataset renames.
#[test]
fn initial_query_paths_encode_scoped_names_without_filename_authority() {
    let directory = TestDir::new();
    let native = fixture(&directory);
    let unusual = schema()
        .replace("<name>Union</name>", "<name>CON</name>")
        .replace("<name>Same</name>", "<name>A/B %2F.</name>");
    fs::write(native.join(TEMPLATE), unusual).unwrap();
    let root = directory.0.join("source");
    import_designer_queries(&native, &root).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let query =
        path("Catalogs/Sample/Templates/QuerySchema/Queries/Query_CON/Query_A%2FB%20%252F%2E.sdbl");
    assert!(reader.declared_query(&query).is_some());
    let resource = fs::read_to_string(root.join(RESOURCE)).unwrap();
    fs::write(
        root.join(RESOURCE),
        resource.replace("<name>CON</name>", "<name>Renamed</name>"),
    )
    .unwrap();
    let references = reader
        .read_query_resource(&path(TEMPLATE), 1_000_000)
        .unwrap();
    assert_eq!(references.references()[0].source, query);
    assert_eq!(
        references.references()[0].slot,
        QuerySlot::Composition {
            datasets: vec!["Renamed".into(), "A/B %2F.".into()]
        }
    );
    let output = directory.0.join("output");
    export_designer(&root, &output).unwrap();
    assert!(
        fs::read_to_string(output.join(TEMPLATE))
            .unwrap()
            .contains("<name>Renamed</name>")
    );
}

/// A composition resource without query text remains exact but cannot later conceal an inline authoring value.
#[test]
fn queryless_composition_resources_have_an_explicit_empty_source_set() {
    let directory = TestDir::new();
    let native = fixture(&directory);
    let empty = format!(
        "<DataCompositionSchema xmlns=\"{DCS}\"><settingsVariant/></DataCompositionSchema>"
    );
    fs::write(native.join(TEMPLATE), &empty).unwrap();
    let root = directory.0.join("source");
    let manifest = import_designer_queries(&native, &root).unwrap();
    assert!(
        manifest
            .query_resources
            .iter()
            .find(|resource| resource.path == path(TEMPLATE))
            .unwrap()
            .sources
            .is_empty()
    );
    let output = directory.0.join("output");
    export_designer(&root, &output).unwrap();
    assert_eq!(fs::read_to_string(output.join(TEMPLATE)).unwrap(), empty);
    let map_path = root.join("source.json");
    let original_map = fs::read(&map_path).unwrap();
    let mut incomplete = manifest;
    incomplete
        .query_resources
        .retain(|resource| resource.path != path(TEMPLATE));
    fs::write(&map_path, serde_json::to_vec_pretty(&incomplete).unwrap()).unwrap();
    assert_eq!(
        export_designer(&root, &directory.0.join("incomplete-map"))
            .unwrap_err()
            .code,
        "source_query_map_invalid"
    );
    fs::write(map_path, original_map).unwrap();
    fs::write(
        root.join(RESOURCE),
        format!("<Form xmlns=\"{LOGFORM}\" version=\"2.20\"/>"),
    )
    .unwrap();
    assert_eq!(
        export_designer(&root, &directory.0.join("foreign-profile"))
            .unwrap_err()
            .code,
        "source_query_map_invalid"
    );
    fs::write(root.join(RESOURCE), &empty).unwrap();
    let descriptor = root.join("Catalogs/Sample/Templates/QuerySchema/template.yaml");
    let original = fs::read_to_string(&descriptor).unwrap();
    fs::write(
        &descriptor,
        original.replace("DataCompositionSchema", "BinaryData"),
    )
    .unwrap();
    assert_eq!(
        export_designer(&root, &directory.0.join("foreign-type"))
            .unwrap_err()
            .code,
        "source_query_map_invalid"
    );
    fs::write(descriptor, original).unwrap();
    fs::write(root.join(RESOURCE), schema()).unwrap();
    assert!(export_designer(&root, &directory.0.join("refused")).is_err());
}

/// Old migration targets preserve a newer project; the same accepted source exports without a fallback.
#[test]
fn earlier_layout_targets_preserve_queries_and_protected_no_op_matches_control() {
    let (directory, _, root) = source();
    let before = Inventory::read(&root).unwrap();
    for plan in [
        prepare_form_layout(&root).unwrap(),
        prepare_readable_layout(&root).unwrap(),
    ] {
        assert_eq!(plan.manifest().schema_version, QUERY_LAYOUT_VERSION);
        assert!(plan.changes().is_empty());
    }
    let accepted = directory.0.join("accepted");
    let reference = accept_source(&root, &accepted, &[]).unwrap();
    let base = AcceptedBase::open(&accepted, &reference).unwrap();
    let output = directory.0.join("protected");
    let prepared = prepare_protected(&root, &base, &output, true).unwrap();
    let control = directory.0.join("control");
    export_designer(&root, &control).unwrap();
    assert_eq!(
        Inventory::read(prepared.xml()).unwrap(),
        Inventory::read(&control).unwrap()
    );
    prepared.validate().unwrap();
    assert_eq!(Inventory::read(&root).unwrap(), before);
}
