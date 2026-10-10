//! Independent native controls distinguish real chart declarations from their implicit rows.

use super::{model, xml};
use crate::AdapterIssue;
use bsl_metadata::{MetadataObject, xml_parser};

type NativeParser = fn(&str) -> bsl_metadata::Result<MetadataObject>;

/// All chart catalogues preserve standard conditions, real user fields, ownership and source table ordering.
#[test]
fn three_chart_families_match_native_standard_and_declared_members() {
    let children = r#"<Attribute uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>ExtraField</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute><TabularSection uuid="30000000-0000-4000-8000-000000000001"><Properties><Name>Rows</Name></Properties><ChildObjects><Attribute uuid="40000000-0000-4000-8000-000000000001"><Properties><Name>Quantity</Name><Type><v8:Type>xs:decimal</v8:Type></Type></Properties></Attribute></ChildObjects></TabularSection>"#;
    let cases: [(&str, NativeParser, &str); 3] = [
        (
            "ChartOfCharacteristicTypes",
            xml_parser::parse_chart_of_characteristic_types_xml,
            "WholeCharacteristicKind",
        ),
        (
            "ChartOfAccounts",
            xml_parser::parse_chart_of_accounts_xml,
            "WholeChartOfAccounts",
        ),
        (
            "ChartOfCalculationTypes",
            xml_parser::parse_chart_of_calculation_types_xml,
            "WholeCatalog",
        ),
    ];
    for (kind, reference, series) in cases {
        for properties in [String::new(), format!("<CodeLength>9</CodeLength><CodeType>Number</CodeType><DescriptionLength>100</DescriptionLength><Hierarchical>true</Hierarchical><CheckUnique>true</CheckUnique><CodeSeries>{series}</CodeSeries>"), "<CodeLength>0</CodeLength><DescriptionLength>0</DescriptionLength>".into(), "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>50000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>".into()] {
            let text = xml(kind, &properties, children);
            assert_eq!(model(&text).unwrap(), reference(&text).unwrap(), "{kind}");
        }
    }
}

/// Account flags remain on the account; ext-dimension flags enter only the implicit row and match native nil IDs.
#[test]
fn account_chart_flags_and_implicit_ext_dimension_rows_match_native() {
    let children = r#"<AccountingFlag uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Accounting</Name><Type><v8:Type>xs:boolean</v8:Type></Type></Properties></AccountingFlag><ExtDimensionAccountingFlag uuid="30000000-0000-4000-8000-000000000001"><Properties><Name>QuantityAccounting</Name><Type><v8:Type>xs:boolean</v8:Type></Type></Properties></ExtDimensionAccountingFlag>"#;
    for property in [
        "",
        "<ExtDimensionTypes/>",
        "<ExtDimensionTypes>ChartOfCharacteristicTypes.Dimensions</ExtDimensionTypes>",
    ] {
        let text = xml("ChartOfAccounts", property, children);
        let actual = model(&text).unwrap();
        assert_eq!(
            actual,
            xml_parser::parse_chart_of_accounts_xml(&text).unwrap()
        );
        assert!(
            actual
                .attributes
                .iter()
                .any(|field| field.name == "Accounting")
        );
        assert!(
            !actual
                .attributes
                .iter()
                .any(|field| field.name == "QuantityAccounting")
        );
        let table = actual.tabular_sections.last().unwrap();
        assert!(table.uuid().is_nil());
        assert_eq!(table.name_en(), Some("ExtDimensionTypes"));
        assert_eq!(
            table.attributes().last().unwrap().name(),
            "QuantityAccounting"
        );
        assert!(table.attributes().iter().all(|field| field.uuid().is_nil()));
    }
}

/// Every supported dependency mode controls both the standard field and the three self-reference sections.
#[test]
fn calculation_dependency_modes_match_native_presence_and_types() {
    for property in [
        "",
        "<DependenceOnCalculationTypes>DontUse</DependenceOnCalculationTypes>",
        "<DependenceOnCalculationTypes>OnActionPeriod</DependenceOnCalculationTypes>",
        "<DependenceOnCalculationTypes>OnRegistrationPeriod</DependenceOnCalculationTypes>",
    ] {
        let text = xml("ChartOfCalculationTypes", property, "");
        let actual = model(&text).unwrap();
        assert_eq!(
            actual,
            xml_parser::parse_chart_of_calculation_types_xml(&text).unwrap()
        );
        let dependent =
            property.contains("OnActionPeriod") || property.contains("OnRegistrationPeriod");
        assert_eq!(actual.tabular_sections.len(), if dependent { 3 } else { 0 });
        assert_eq!(
            actual
                .attributes
                .iter()
                .any(|field| field.name == "ПериодДействияБазовый"),
            dependent
        );
        assert!(actual.tabular_sections.iter().all(|table| {
            table.uuid().is_nil()
                && table
                    .attributes()
                    .iter()
                    .all(|column| column.uuid().is_nil())
        }));
    }
}

/// Bad dependency tokens/references and malformed real flag types fail at the current source location.
#[test]
fn invalid_chart_settings_and_flags_do_not_become_ready_models() {
    for (kind, property) in [
        (
            "ChartOfCalculationTypes",
            "<DependenceOnCalculationTypes>Future</DependenceOnCalculationTypes>",
        ),
        ("ChartOfCalculationTypes", "<DependenceOnCalculationTypes/>"),
        (
            "ChartOfAccounts",
            "<ExtDimensionTypes>Future.Items</ExtDimensionTypes>",
        ),
        (
            "ChartOfAccounts",
            "<ExtDimensionTypes>Catalog.Items</ExtDimensionTypes>",
        ),
        (
            "ChartOfAccounts",
            "<ExtDimensionTypes>ChartOfCharacteristicTypes.</ExtDimensionTypes>",
        ),
    ] {
        let text = xml(kind, property, "");
        let error = model(&text).unwrap_err();
        assert!(matches!(error.issue, AdapterIssue::InvalidProperty { .. }));
        assert!(text[error.range].contains(property));
    }
    let duplicate = xml(
        "ChartOfCalculationTypes",
        "<DependenceOnCalculationTypes>DontUse</DependenceOnCalculationTypes><DependenceOnCalculationTypes>OnActionPeriod</DependenceOnCalculationTypes>",
        "",
    );
    assert!(matches!(
        model(&duplicate).unwrap_err().issue,
        AdapterIssue::DuplicateProperty { .. }
    ));
    for kind in ["AccountingFlag", "ExtDimensionAccountingFlag"] {
        let child = format!(
            "<{kind} uuid=\"20000000-0000-4000-8000-000000000001\"><Properties><Name>Flag</Name><Type><v8:Type>cfg:FutureRef.Items</v8:Type></Type></Properties></{kind}>"
        );
        let text = xml("ChartOfAccounts", "", &child);
        let error = model(&text).unwrap_err();
        assert!(matches!(error.issue, AdapterIssue::UnsupportedType { .. }));
        assert!(text[error.range].contains("FutureRef.Items"));
        let missing = text.replace("<Type><v8:Type>cfg:FutureRef.Items</v8:Type></Type>", "");
        assert!(matches!(
            model(&missing).unwrap_err().issue,
            AdapterIssue::MissingProperty { .. }
        ));
    }
}
