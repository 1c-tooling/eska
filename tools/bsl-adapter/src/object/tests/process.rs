//! Independent native parsers verify process/exchange standard fields and real task-addressing attributes.

use super::{model, xml};
use crate::AdapterIssue;

type NativeParser = fn(&str) -> bsl_metadata::Result<MetadataObject>;
use bsl_metadata::{MetadataObject, xml_parser};

/// Different catalogues are selected by actual object kind; lengths and code modes keep native semantics.
#[test]
fn process_task_and_exchange_models_match_native_fields_and_ownership() {
    let children = r#"<Attribute uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>ExtraField</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute><TabularSection uuid="30000000-0000-4000-8000-000000000001"><Properties><Name>Rows</Name></Properties><ChildObjects><Attribute uuid="40000000-0000-4000-8000-000000000001"><Properties><Name>Quantity</Name><Type><v8:Type>xs:decimal</v8:Type></Type></Properties></Attribute></ChildObjects></TabularSection>"#;
    let cases: [(&str, NativeParser); 3] = [
        ("BusinessProcess", xml_parser::parse_business_process_xml),
        ("Task", xml_parser::parse_task_xml),
        ("ExchangePlan", xml_parser::parse_exchange_plan_xml),
    ];
    for (kind, reference) in cases {
        for properties in [
            "",
            "<CodeLength>9</CodeLength><CodeType>Number</CodeType><DescriptionLength>150</DescriptionLength><NumberLength>12</NumberLength><NumberType>Number</NumberType><CheckUnique>true</CheckUnique>",
            "<CodeLength>0</CodeLength><DescriptionLength>0</DescriptionLength><NumberLength>0</NumberLength>",
            "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>50000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>",
        ] {
            let text = xml(kind, properties, children);
            assert_eq!(model(&text).unwrap(), reference(&text).unwrap(), "{kind}");
        }
    }
}

/// Task-addressing declarations are real object fields, preserving typed values without invented IDs.
#[test]
fn task_addressing_attributes_match_the_native_model_and_reject_invalid_source_types() {
    let child = r#"<AddressingAttribute uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Assignee</Name><Type><v8:Type>cfg:CatalogRef.People</v8:Type></Type></Properties></AddressingAttribute>"#;
    let text = xml("Task", "", child);
    assert_eq!(
        model(&text).unwrap(),
        xml_parser::parse_task_xml(&text).unwrap()
    );
    let invalid = text.replace("cfg:CatalogRef.People", "cfg:FutureRef.People");
    let error = model(&invalid).unwrap_err();
    assert!(matches!(error.issue, AdapterIssue::UnsupportedType { .. }));
    assert!(invalid[error.range].contains("FutureRef.People"));
    let missing = text.replace("<Type><v8:Type>cfg:CatalogRef.People</v8:Type></Type>", "");
    assert!(matches!(
        model(&missing).unwrap_err().issue,
        AdapterIssue::MissingProperty { .. }
    ));
}
