//! Explicit parameter declarations keep current scopes, exact bytes and namespace-resolved native values.

use super::*;
use crate::project_core::metadata_model::MetadataValue;

const CORE: &str = "http://v8.1c.ru/8.1/data/core";
const SCHEMA: &str = "http://www.w3.org/2001/XMLSchema";

/// Declarations belong to the whole schema; dataset-local lookalikes and duplicate names remain distinct.
#[test]
fn explicit_parameters_keep_order_types_expression_and_exact_current_locations() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, resource) = inputs(&reader, TEMPLATE);
    let parameter = format!(
        "<parameter xmlns:t=\"{CORE}\" xmlns:q=\"{SCHEMA}\"><name>ДатаЯ</name><valueType><t:Type>q:dateTime</t:Type><t:DateQualifiers><t:DateFractions>Date</t:DateFractions></t:DateQualifiers></valueType><expression>EndOfDay(&amp;ДатаЯ)</expression><valueListAllowed>true</valueListAllowed><futureOption/></parameter>"
    );
    let resource = resource.replace("<settingsVariant/>", &format!("<settingsVariant/><!-- 😀 -->\r\n{parameter}<parameter><name>датаЯ</name></parameter>"))
        .replace("<name>Same</name>", "<name>Same</name><parameter><name>DatasetLocal</name></parameter>");
    fs::remove_dir_all(&root).unwrap();
    let context = reader
        .parse_query_context(&path(TEMPLATE), &owner, &resource, 1_000_000)
        .unwrap();
    let view = context.consumer_view().unwrap();
    assert_eq!(view.parameter_declarations().len(), 2);
    let parameter = &view.parameter_declarations()[0];
    assert_eq!(parameter.scope, QueryConsumerScope::Resource);
    assert_eq!(parameter.name.value, "ДатаЯ");
    exact(&context, &parameter.name, "<name>ДатаЯ</name>");
    let value_type = parameter.value_type.as_ref().unwrap();
    let MetadataValue::Record(fields) = &value_type.value.value else {
        panic!("native value type");
    };
    assert_eq!(fields[0].key.namespace.as_deref(), Some(CORE));
    let MetadataValue::QualifiedText { text, key } = &fields[0].value else {
        panic!("resolved QName");
    };
    assert_eq!(text, "q:dateTime");
    assert_eq!(key.namespace.as_deref(), Some(SCHEMA));
    assert_eq!(key.name, "dateTime");
    exact(
        &context,
        value_type,
        "<valueType><t:Type>q:dateTime</t:Type><t:DateQualifiers><t:DateFractions>Date</t:DateFractions></t:DateQualifiers></valueType>",
    );
    let expression = parameter.expression.as_ref().unwrap();
    assert_eq!(expression.value, "EndOfDay(&ДатаЯ)");
    exact(
        &context,
        expression,
        "<expression>EndOfDay(&amp;ДатаЯ)</expression>",
    );
    assert!(parameter.value_list_allowed.as_ref().unwrap().value);
    assert!(view.parameter_declarations()[1].value_type.is_none());
    assert_eq!(view.parameter_declarations()[1].name.value, "датаЯ");
    assert!(
        view.unhandled()
            .iter()
            .any(|part| part.kind == QueryUnhandledKind::Parameter
                && matches!(part.scope, QueryConsumerScope::Dataset { .. }))
    );
    assert!(!view.consumer_complete());
}

/// Type IDs and foreign namespaces remain native values rather than borrowing an unrelated type by name.
#[test]
fn parameter_type_ids_unknown_qualifiers_and_foreign_nodes_are_not_discarded() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, resource) = inputs(&reader, TEMPLATE);
    let resource = resource.replace("<settingsVariant/>", &format!("<settingsVariant/><parameter xmlns:t=\"{CORE}\"><name>Reference</name><valueType><t:TypeId>10000000-0000-4000-8000-000000000001</t:TypeId><t:FutureQualifier>unknown</t:FutureQualifier></valueType></parameter><foreign:parameter xmlns:foreign=\"urn:foreign\"><name>Impostor</name></foreign:parameter>"));
    let context = reader
        .parse_query_context(&path(TEMPLATE), &owner, &resource, 1_000_000)
        .unwrap();
    let view = context.consumer_view().unwrap();
    assert_eq!(view.parameter_declarations().len(), 1);
    let MetadataValue::Record(fields) = &view.parameter_declarations()[0]
        .value_type
        .as_ref()
        .unwrap()
        .value
        .value
    else {
        panic!("native type fields");
    };
    assert_eq!(fields[0].key.name, "TypeId");
    assert_eq!(fields[1].key.name, "FutureQualifier");
    assert!(
        view.unhandled()
            .iter()
            .any(
                |part| part.node.value.namespace.as_deref() == Some("urn:foreign")
                    && part.coverage == QueryConsumerCoverage::Unsupported
            )
    );
}

/// Malformed supported declarations refuse the supplied current resource instead of silently falling back.
#[test]
fn malformed_explicit_parameter_names_and_flags_refuse_publication() {
    let (_directory, _, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let (owner, resource) = inputs(&reader, TEMPLATE);
    for body in [
        "",
        "<name/>",
        "<name>P</name><name>Other</name>",
        "<name>P</name><valueListAllowed>maybe</valueListAllowed>",
    ] {
        let current = resource.replace(
            "<settingsVariant/>",
            &format!("<settingsVariant/><parameter>{body}</parameter>"),
        );
        let context = reader
            .parse_query_context(&path(TEMPLATE), &owner, &current, 1_000_000)
            .unwrap();
        assert!(context.consumer_view().is_err(), "{body}");
    }
}
