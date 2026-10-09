//! Compare supported type semantics with the selected native parser, then challenge `QName` identity.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// A complete descriptor resolves every type token at its own namespace scope.
fn xml(fields: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:v8ui="http://v8.1c.ru/8.1/data/ui" xmlns:mxl="http://v8.1c.ru/8.2/data/spreadsheet" xmlns:other="urn:other" version="2.20"><DefinedType uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>ValueType</Name>{fields}</Properties></DefinedType></MetaDataObject>"#
    )
}

/// Convert through the shared semantic parser rather than a private DOM in the adapter.
fn converted(text: &str) -> Result<bsl_metadata::DefinedType, AdapterError> {
    let parsed = metadata_parser::parse(text, None, PropertiesMode::All).unwrap();
    crate::defined_type(&parsed.objects[0])
}

/// Defaults and mixed Type/TypeSet ordering follow the pinned kernel's documented input model.
#[test]
fn supported_defined_types_match_native_kernel_models() {
    for fields in [
        "<Type/>",
        "<Type><v8:Type>xs:boolean</v8:Type></Type>",
        "<Type><v8:Type>xs:string</v8:Type><v8:StringQualifiers><v8:Length>75</v8:Length><v8:AllowedLength>Variable</v8:AllowedLength></v8:StringQualifiers></Type>",
        "<Type><v8:Type>xs:decimal</v8:Type></Type>",
        "<Type><v8:Type>xs:decimal</v8:Type><v8:NumberQualifiers><v8:Digits>15</v8:Digits><v8:FractionDigits>2</v8:FractionDigits><v8:AllowedSign>Any</v8:AllowedSign></v8:NumberQualifiers></Type>",
        "<Type><v8:Type>xs:dateTime</v8:Type><v8:DateQualifiers><v8:DateFractions>DateTime</v8:DateFractions></v8:DateQualifiers></Type>",
        "<Type><v8:Type>xs:dateTime</v8:Type><v8:DateQualifiers><v8:DateFractions>Time</v8:DateFractions></v8:DateQualifiers></Type>",
        "<Type><v8:Type>cfg:CatalogRef.Items</v8:Type><v8:Type>cfg:DocumentObject.Receipt</v8:Type></Type>",
        "<Type><v8:Type>cfg:ConstantValueManager.Value</v8:Type><v8:Type>cfg:InformationRegisterRecordSet.Rates</v8:Type><v8:Type>cfg:BusinessProcessRoutePointRef.Flow</v8:Type></Type>",
        "<Type><v8:Type>cfg:ExternalDataProcessorObject.Tools</v8:Type><v8:Type>cfg:ReportObject</v8:Type><v8:Type>cfg:DynamicList</v8:Type></Type>",
        "<Type><v8:TypeSet>cfg:CatalogRef</v8:TypeSet><v8:Type>xs:string</v8:Type><v8:TypeSet>cfg:AnyIBRef</v8:TypeSet><v8:TypeSet>cfg:BusinessProcessRoutePointRef</v8:TypeSet></Type>",
        "<Type><v8:TypeSet>cfg:DefinedType.Amount</v8:TypeSet><v8:TypeSet>cfg:Characteristic.Properties</v8:TypeSet></Type>",
        "<Type><v8:Type>v8:UUID</v8:Type><v8:Type>v8:ValueStorage</v8:Type><v8:Type>v8:ValueTable</v8:Type><v8:Type>v8ui:Picture</v8:Type><v8:Type>mxl:SpreadsheetDocument</v8:Type><v8:Type>v8:Array</v8:Type></Type>",
    ] {
        let text = xml(fields);
        assert_eq!(
            converted(&text).unwrap(),
            bsl_metadata::xml_parser::parse_defined_type_xml(&text).unwrap(),
            "{fields}"
        );
    }
}

/// A prefix alias or default `QName` namespace cannot change the represented type.
#[test]
fn type_identity_uses_resolved_namespace_and_local_name() {
    let canonical = xml(
        "<Type><v8:Type>xs:decimal</v8:Type><v8:TypeSet>cfg:DefinedType.Amount</v8:TypeSet></Type>",
    );
    let alias = canonical
        .replace("xmlns:v8=", "xmlns:core=")
        .replace("xmlns:xs=", "xmlns:schema=")
        .replace("xmlns:cfg=", "xmlns:config=")
        .replace("v8:", "core:")
        .replace("xs:", "schema:")
        .replace("cfg:", "config:");
    assert_eq!(converted(&canonical).unwrap(), converted(&alias).unwrap());
    let default = xml(
        "<Type><v8:Type xmlns='http://www.w3.org/2001/XMLSchema'>decimal</v8:Type><v8:TypeSet>cfg:DefinedType.Amount</v8:TypeSet></Type>",
    );
    assert_eq!(converted(&canonical).unwrap(), converted(&default).unwrap());
}

/// Composition settings have their own namespace and must not depend on the legacy XML prefix.
#[test]
fn composition_settings_types_use_exact_namespace_identity() {
    let text = xml(
        "<Type xmlns:dcsset='http://v8.1c.ru/8.1/data-composition-system/settings'><v8:Type>dcsset:SettingsComposer</v8:Type><v8:Type>dcsset:Filter</v8:Type></Type>",
    );
    let canonical = converted(&text).unwrap();
    assert_eq!(
        canonical,
        bsl_metadata::xml_parser::parse_defined_type_xml(&text).unwrap()
    );
    assert_eq!(
        canonical,
        converted(&text.replace("dcsset", "alias")).unwrap()
    );
    let foreign = text.replace(
        "http://v8.1c.ru/8.1/data-composition-system/settings",
        "urn:foreign",
    );
    assert!(matches!(
        converted(&foreign).unwrap_err().issue,
        AdapterIssue::UnsupportedType { .. }
    ));
    let unknown = text.replace("SettingsComposer", "FutureSettings");
    assert!(matches!(
        converted(&unknown).unwrap_err().issue,
        AdapterIssue::UnsupportedType { .. }
    ));
}

/// An unbound or rebound prefix cannot acquire a schema or platform catalog type by spelling.
#[test]
fn foreign_and_unresolved_type_names_are_explicit_failures() {
    for token in [
        "other:decimal",
        "other:ValueTable",
        "cfg:FutureThing.Value",
        "cfg:CatalogRef.Items.Other",
        "cfg:EnumObject.Items",
        "cfg:InformationRegisterObject.Rates",
    ] {
        let text = xml(&format!("<Type><v8:Type>{token}</v8:Type></Type>"));
        let error = converted(&text).unwrap_err();
        assert!(matches!(error.issue, AdapterIssue::UnsupportedType { .. }));
        assert!(text[error.range].starts_with("<Type>"));
    }
    let text = xml("<Type><v8:TypeSet>cfg:FutureThing</v8:TypeSet></Type>");
    assert!(matches!(
        converted(&text).unwrap_err().issue,
        AdapterIssue::UnsupportedType { .. }
    ));
    let text = xml("<Type><v8:Type>unbound:string</v8:Type></Type>");
    assert_eq!(
        converted(&text).unwrap_err().issue,
        AdapterIssue::UnresolvedTypeName {
            text: "unbound:string".into()
        }
    );
    let text = xml("<Type><v8:Type xmlns:xs='urn:other'>xs:string</v8:Type></Type>");
    assert!(matches!(
        converted(&text).unwrap_err().issue,
        AdapterIssue::UnsupportedType { .. }
    ));
}

/// New fields and foreign lookalikes remain unsupported rather than producing Unknown or defaults.
#[test]
fn unknown_type_fields_and_qualifiers_are_not_discarded() {
    for fields in [
        "<other:Type>xs:string</other:Type>",
        "<v8:FutureType>xs:string</v8:FutureType>",
        "<v8:Type>xs:decimal</v8:Type><v8:NumberQualifiers><v8:FuturePrecision>10</v8:FuturePrecision></v8:NumberQualifiers>",
        "<v8:Type>xs:string</v8:Type><v8:StringQualifiers><other:Length>50</other:Length></v8:StringQualifiers>",
    ] {
        let text = xml(&format!("<Type>{fields}</Type>"));
        assert!(matches!(
            converted(&text).unwrap_err().issue,
            AdapterIssue::UnsupportedTypeField { .. }
        ));
    }
}

/// Bad explicit values cannot inherit numeric defaults or narrow with overflow.
#[test]
fn malformed_and_duplicate_qualifiers_are_rejected_at_type_location() {
    for qualifiers in [
        "<v8:NumberQualifiers><v8:Digits>256</v8:Digits></v8:NumberQualifiers>",
        "<v8:NumberQualifiers><v8:FractionDigits>-1</v8:FractionDigits></v8:NumberQualifiers>",
        "<v8:StringQualifiers><v8:Length>4294967296</v8:Length></v8:StringQualifiers>",
        "<v8:StringQualifiers><v8:Length><v8:Value>50</v8:Value></v8:Length></v8:StringQualifiers>",
        "<v8:DateQualifiers><v8:DateFractions>Future</v8:DateFractions></v8:DateQualifiers>",
        "<v8:NumberQualifiers><v8:Digits>10</v8:Digits><v8:Digits>12</v8:Digits></v8:NumberQualifiers>",
        "<v8:NumberQualifiers/><v8:NumberQualifiers/>",
    ] {
        let text = xml(&format!(
            "<Type><v8:Type>xs:decimal</v8:Type>{qualifiers}</Type>"
        ));
        let error = converted(&text).unwrap_err();
        assert!(text[error.range].starts_with("<Type>"));
    }
}

/// Missing Type and missing properties remain failures rather than an empty successful model.
#[test]
fn defined_type_requires_complete_properties_and_valid_identity() {
    let text = xml("");
    assert_eq!(
        converted(&text).unwrap_err().issue,
        AdapterIssue::MissingProperty {
            name: "Type".into()
        }
    );
    let parsed = metadata_parser::parse(&xml("<Type/>"), None, PropertiesMode::Summary).unwrap();
    assert_eq!(
        crate::defined_type(&parsed.objects[0]).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    let text = xml("<Type/>").replace("10000000-0000-4000-8000-000000000001", "invalid");
    assert_eq!(
        converted(&text).unwrap_err().issue,
        AdapterIssue::InvalidUuid
    );
}
