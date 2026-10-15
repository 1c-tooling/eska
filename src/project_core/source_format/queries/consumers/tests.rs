//! Storage consumer tests use supplied current inputs after owned files disappear, never a query resolver.

use std::fs;

use super::*;
use crate::project_core::source_format::{
    FormElementKind, FormId, QueryAuthoringRange, QuerySlot, SourceReader, import_designer_queries,
    queries::test_support::{self, DIALOG, TEMPLATE, path, source},
    yaml,
};
use crate::{query_creation_fixture as fixture, test_support::TestDir};

/// Read only the two declared inputs before a pure-view test removes its owned source tree.
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

/// A native binding is explicit; its alias is deliberately unrelated to metadata inference.
fn field(data_path: &str, name: &str) -> String {
    format!(
        "<field xsi:type=\"DataSetFieldField\"><dataPath>{data_path}</dataPath><field>{name}</field></field>"
    )
}

/// Actual supported list properties retain UI and attribute IDs in separate native collections.
fn consumer_form(text: &str) -> String {
    text.replace("<Table name=\"Grid\" id=\"5\"/>", "<Table name=\"Grid\" id=\"5\"><DataPath>Rows</DataPath><ChildItems><InputField name=\"Code\" id=\"6\"><DataPath>Rows.Code</DataPath></InputField></ChildItems></Table>")
        .replace("<ManualQuery>true</ManualQuery>", "<ManualQuery>true</ManualQuery><MainTable>Catalog.Sample</MainTable><ListSettings/>")
}

/// Every public Located value is checked against the exact native and current authoring bytes.
fn exact<T>(context: &QueryContextView, value: &QueryLocated<T>, text: &str) {
    assert_eq!(&context.reference_xml()[value.native_range.clone()], text);
    assert_eq!(
        value.authoring,
        QueryAuthoringRange::Exact(value.native_range.clone())
    );
}

/// CF/CFE/EPF/ERF preserve declared resource identities and pure consumers after all source files disappear.
#[test]
fn all_four_profiles_have_pure_identity_fields_lists_and_explicit_incomplete_coverage() {
    for kind in fixture::TYPES {
        let directory = TestDir::new();
        let native = fixture::native(&directory.0, kind);
        let schema = fs::read_to_string(native.input.join(&native.schema))
            .unwrap()
            .replace(
                "<name>Data</name>",
                &format!("<name>Data</name>{}", field("Code", "Code &amp; Value")),
            );
        fs::write(native.input.join(&native.schema), schema).unwrap();
        let form = consumer_form(&fs::read_to_string(native.input.join(&native.dialog)).unwrap());
        fs::write(native.input.join(&native.dialog), form).unwrap();
        let root = directory.0.join("queries");
        import_designer_queries(&native.input, &root).unwrap();
        let reader = SourceReader::open(&root).unwrap();
        let dcs = inputs(&reader, native.schema.to_str().unwrap());
        let form = inputs(&reader, native.dialog.to_str().unwrap());
        fs::remove_dir_all(&root).unwrap();
        for (resource, (owner, text), profile) in [
            (&native.schema, dcs, QueryProfile::Composition),
            (&native.dialog, form, QueryProfile::ManagedForm),
        ] {
            let context = reader
                .parse_query_context(&path(resource.to_str().unwrap()), &owner, &text, 1_000_000)
                .unwrap();
            let view = context.consumer_view().unwrap();
            assert_eq!(view.owner(), context.owner());
            assert_eq!(view.source(), context.source());
            assert_eq!(view.native_descriptor(), context.native_descriptor());
            assert_eq!(view.native_resource(), context.native_resource());
            assert_eq!(view.uuid(), context.uuid());
            assert_eq!(view.profile(), profile);
            assert!(!view.consumer_complete());
            if profile == QueryProfile::Composition {
                assert_eq!(view.datasets().len(), 1);
                let dataset = &view.datasets()[0];
                assert_eq!(dataset.kind, QueryDatasetKind::Query);
                assert_eq!(dataset.path[0].value, "Data");
                assert_eq!(dataset.query.as_ref().unwrap(), &context.references()[0]);
                assert_eq!(dataset.fields[0].field.value, "Code & Value");
                exact(
                    &context,
                    &dataset.fields[0].field,
                    "<field>Code &amp; Value</field>",
                );
                assert!(view.dynamic_lists().is_empty());
            } else {
                let list = &view.dynamic_lists()[0];
                assert_eq!(list.attribute.value, FormId::parse("5").unwrap());
                assert_eq!(list.name.value, "Rows");
                assert!(list.manual_query.value);
                assert_eq!(list.main_table.as_ref().unwrap().value, "Catalog.Sample");
                assert_eq!(list.query.as_ref().unwrap(), &context.references()[0]);
                assert_eq!(view.form_data_paths().len(), 2);
                assert_eq!(
                    view.form_data_paths()[1].element.value,
                    FormId::parse("6").unwrap()
                );
                assert_eq!(view.form_data_paths()[1].data_path.value, "Rows.Code");
                assert!(matches!(
                    list.main_table.as_ref().unwrap().authoring,
                    QueryAuthoringRange::EnclosingNode(_)
                ));
                assert!(view.datasets().is_empty());
            }
        }
    }
}

/// Repeated dataset/field names remain distinct through two union ancestors and qualified type aliases.
#[test]
fn nested_union_fields_keep_full_scopes_actual_ranges_and_namespace_aliases() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, resource) = inputs(&reader, TEMPLATE);
    let field = field("Код", "Code").replace(
        "xsi:type=\"DataSetFieldField\"",
        &format!("xmlns:s=\"{DCS}\" xsi:type=\"s:DataSetFieldField\""),
    );
    let resource = resource
        .replace("<name>Same</name>", &format!("<name>Same</name>{field}"))
        .replacen(
            "<dataSet xsi:type=\"DataSetUnion\">",
            "<dataSet xsi:type=\"DataSetUnion\"><name>Outer</name><item xsi:type=\"DataSetUnion\">",
            1,
        )
        .replacen("</item></dataSet>", "</item></item></dataSet>", 1);
    fs::remove_dir_all(&root).unwrap();
    let context = reader
        .parse_query_context(&path(TEMPLATE), &owner, &resource, 1_000_000)
        .unwrap();
    let view = context.consumer_view().unwrap();
    let paths: Vec<Vec<&str>> = view
        .datasets()
        .iter()
        .map(|dataset| {
            dataset
                .path
                .iter()
                .map(|part| part.value.as_str())
                .collect()
        })
        .collect();
    assert_eq!(
        paths,
        [
            vec!["Outer"],
            vec!["Outer", "Union"],
            vec!["Outer", "Union", "Same"],
            vec!["Same"]
        ]
    );
    let nested = &view.datasets()[2];
    let direct = &view.datasets()[3];
    assert_eq!(
        nested.query.as_ref().unwrap().slot,
        QuerySlot::Composition {
            datasets: vec!["Outer".into(), "Union".into(), "Same".into()]
        }
    );
    for dataset in [nested, direct] {
        assert_eq!(dataset.fields.len(), 1);
        exact(
            &context,
            &dataset.fields[0].data_path,
            "<dataPath>Код</dataPath>",
        );
    }
    assert_ne!(
        nested.fields[0].data_path.native_range,
        direct.fields[0].data_path.native_range
    );
}

/// Parameters, links, resources, expressions, settings and foreign shapes stay explicit inventory.
#[test]
fn unknown_native_parts_are_locatable_and_cannot_gain_semantic_completeness() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, resource) = inputs(&reader, TEMPLATE);
    let opaque = "<parameter><name>P</name></parameter><dataSetLink/><calculatedField/><totalField/><fake:parameter xmlns:fake=\"urn:foreign\"/><futurePart/>";
    let resource = resource.replace("<settingsVariant/>", &format!("<settingsVariant/>{opaque}"))
        .replace("<name>Same</name>", "<name>Same</name><field xmlns:foreign=\"urn:foreign\" xsi:type=\"foreign:DataSetFieldField\"><dataPath>Code</dataPath><field>Code</field></field><foreign:field xmlns:foreign=\"urn:foreign\"/>");
    let context = reader
        .parse_query_context(&path(TEMPLATE), &owner, &resource, 1_000_000)
        .unwrap();
    let view = context.consumer_view().unwrap();
    assert!(!view.consumer_complete());
    assert!(
        view.datasets()
            .iter()
            .all(|dataset| dataset.fields.is_empty())
    );
    for kind in [
        QueryUnhandledKind::Parameter,
        QueryUnhandledKind::DatasetLink,
        QueryUnhandledKind::CalculatedField,
        QueryUnhandledKind::TotalField,
        QueryUnhandledKind::Settings,
    ] {
        assert!(view.unhandled().iter().any(|part| part.kind == kind && part.coverage == QueryConsumerCoverage::Incomplete));
    }
    let foreign = view
        .unhandled()
        .iter()
        .find(|part| {
            part.node.value.namespace.as_deref() == Some("urn:foreign")
                && part.node.value.local == "parameter"
        })
        .unwrap();
    assert_eq!(foreign.kind, QueryUnhandledKind::Other);
    assert_eq!(foreign.coverage, QueryConsumerCoverage::Unsupported);
    exact(
        &context,
        &foreign.node,
        "<fake:parameter xmlns:fake=\"urn:foreign\"/>",
    );
    assert!(
        view.unhandled()
            .iter()
            .any(|part| part.node.value.local == "field"
                && part.node.value.namespace.as_deref() == Some(DCS)
                && part.coverage == QueryConsumerCoverage::Unsupported
                && matches!(part.scope, QueryConsumerScope::Dataset { .. }))
    );
    for part in view.unhandled() {
        assert_eq!(
            part.node.authoring,
            QueryAuthoringRange::Exact(part.node.native_range.clone())
        );
        assert!(
            context
                .reference_xml()
                .get(part.node.native_range.clone())
                .is_some()
        );
    }
}

/// Duplicate native bindings and malformed supported scalar shapes refuse instead of publishing partial facts.
#[test]
fn duplicate_empty_missing_and_nested_supported_fields_refuse_publication() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, resource) = inputs(&reader, TEMPLATE);
    let valid = field("Code", "Code");
    let invalid_fields = [
        valid.clone() + &field("Code", "Other"),
        field("", "Code"),
        field("Code", ""),
        valid.replace("<dataPath>Code</dataPath>", ""),
        valid.replace(
            "<dataPath>Code</dataPath>",
            "<dataPath>Code</dataPath><dataPath>Again</dataPath>",
        ),
        valid.replace("<field>Code</field>", "<field><nested/></field>"),
        valid.replace(
            "<dataPath>Code</dataPath>",
            "<foreign:dataPath xmlns:foreign=\"urn:foreign\">Code</foreign:dataPath>",
        ),
    ];
    for fields in invalid_fields {
        let current = resource.replace("<name>Same</name>", &format!("<name>Same</name>{fields}"));
        let context = reader
            .parse_query_context(&path(TEMPLATE), &owner, &current, 1_000_000)
            .unwrap();
        assert!(context.consumer_view().is_err(), "{fields}");
        assert_eq!(context.reference_xml(), current);
    }
    let valid_resource =
        resource.replace("<name>Same</name>", &format!("<name>Same</name>{valid}"));
    assert!(
        reader
            .parse_query_context(&path(TEMPLATE), &owner, &valid_resource, 1_000_000)
            .unwrap()
            .consumer_view()
            .is_ok()
    );
}

/// Current YAML changes supply list/table facts without accepted maps, saved IO or decoded subrange claims.
#[test]
fn current_form_bytes_keep_attribute_ui_namespaces_and_enclosing_yaml_locations() {
    let directory = TestDir::new();
    let native = test_support::fixture(&directory);
    fs::write(native.join(DIALOG), consumer_form(&test_support::form())).unwrap();
    let root = directory.0.join("source");
    import_designer_queries(&native, &root).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, _) = inputs(&reader, DIALOG);
    let mut value = yaml::decode(&owner).unwrap();
    value["form"]["elements"]["6"]["name"] = "CurrentCode".into();
    for part in value["form"]["native"]["root"].as_array_mut().unwrap() {
        if let Some(xml) = part.get_mut("value") {
            *xml = xml
                .as_str()
                .unwrap()
                .replace("Catalog.Sample", "Catalog.Current")
                .into();
        }
    }
    for part in value["form"]["native"]["elements"]["6"]["parts"]
        .as_array_mut()
        .unwrap()
    {
        if let Some(xml) = part.get_mut("value") {
            *xml = xml
                .as_str()
                .unwrap()
                .replace("Rows.Code", "Rows.ТекущийКод")
                .into();
        }
    }
    let text = yaml::encode(&value).unwrap();
    fs::remove_dir_all(&root).unwrap();
    let context = reader
        .parse_query_context(&path(DIALOG), &text, &text, 1_000_000)
        .unwrap();
    let view = context.consumer_view().unwrap();
    assert_eq!(view.dynamic_lists().len(), 3);
    let list = &view.dynamic_lists()[0];
    assert_eq!(list.attribute.value.as_str(), "5");
    assert_eq!(list.name.value, "Rows");
    assert_eq!(list.main_table.as_ref().unwrap().value, "Catalog.Current");
    assert_eq!(view.dynamic_lists()[1].attribute.value.as_str(), "6");
    assert_eq!(view.dynamic_lists()[1].name.value, "Empty");
    let automatic = &view.dynamic_lists()[2];
    assert!(!automatic.manual_query.value);
    assert!(automatic.query.is_none());
    let ui = &view.form_data_paths()[1];
    assert_eq!(ui.element.value.as_str(), "6");
    assert_eq!(ui.element_name.value, "CurrentCode");
    assert_eq!(ui.element_kind, FormElementKind::InputField);
    assert_eq!(ui.data_path.value, "Rows.ТекущийКод");
    assert_eq!(ui.element_name.authoring, QueryAuthoringRange::Unmapped);
    for located in [&ui.data_path, list.main_table.as_ref().unwrap()] {
        let QueryAuthoringRange::EnclosingNode(range) = &located.authoring else {
            panic!("preserved native value must retain its current YAML scalar");
        };
        let preserved: String = serde_json::from_str(&text[range.clone()]).unwrap();
        assert!(preserved.contains(&located.value));
        assert_eq!(
            context
                .locate_reference_range(located.native_range.clone())
                .unwrap(),
            located.authoring
        );
    }
    assert!(
        view.unhandled()
            .iter()
            .any(|part| part.kind == QueryUnhandledKind::Settings
                && part.scope
                    == QueryConsumerScope::FormAttribute {
                        attribute: FormId::parse("5").unwrap()
                    })
    );
    assert!(!view.consumer_complete());
}

/// Foreign `DataPath` nodes stay inventory; ambiguous direct `MainTable`/`DataPath` nodes refuse publication.
#[test]
fn form_foreign_properties_are_not_promoted_and_duplicate_scalar_nodes_refuse() {
    let directory = TestDir::new();
    let native = test_support::fixture(&directory);
    let form = consumer_form(&test_support::form()).replace(
        "<DataPath>Rows.Code</DataPath>",
        "<fake:DataPath xmlns:fake=\"urn:foreign\">Rows.Code</fake:DataPath>",
    );
    fs::write(native.join(DIALOG), form).unwrap();
    let root = directory.0.join("source");
    import_designer_queries(&native, &root).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, _) = inputs(&reader, DIALOG);
    let context = reader
        .parse_query_context(&path(DIALOG), &owner, &owner, 1_000_000)
        .unwrap();
    let view = context.consumer_view().unwrap();
    assert_eq!(view.form_data_paths().len(), 1);
    assert!(
        view.unhandled()
            .iter()
            .any(|part| part.node.value.local == "DataPath"
                && part.coverage == QueryConsumerCoverage::Unsupported)
    );
    for (before, after) in [
        (
            "<MainTable>Catalog.Sample</MainTable>",
            "<MainTable>Catalog.Sample</MainTable><MainTable>Again</MainTable>",
        ),
        (
            "<DataPath>Rows</DataPath>",
            "<DataPath>Rows</DataPath><DataPath>Again</DataPath>",
        ),
        (
            "<DataPath>Rows</DataPath>",
            "<DataPath><nested/></DataPath>",
        ),
    ] {
        let mut value = yaml::decode(&owner).unwrap();
        let native = &mut value["form"]["native"];
        replace_preserved(native, before, after);
        let current = yaml::encode(&value).unwrap();
        let context = reader
            .parse_query_context(&path(DIALOG), &current, &current, 1_000_000)
            .unwrap();
        assert!(context.consumer_view().is_err(), "{after}");
    }
}

/// Test mutations address preserved stencils structurally; production never searches XML strings for coordinates.
fn replace_preserved(value: &mut serde_json::Value, before: &str, after: &str) {
    match value {
        serde_json::Value::Object(fields) => {
            for (name, value) in fields {
                if name == "value" {
                    if let Some(xml) = value.as_str() {
                        *value = xml.replace(before, after).into();
                    }
                } else {
                    replace_preserved(value, before, after);
                }
            }
        }
        serde_json::Value::Array(parts) => {
            for value in parts {
                replace_preserved(value, before, after);
            }
        }
        _ => {}
    }
}
