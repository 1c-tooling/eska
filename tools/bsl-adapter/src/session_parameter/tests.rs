//! Canonical and native readers agree on actual identity, ownership and configured value types.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Every fixture declares namespace identity explicitly; the spelling of prefixes is immaterial.
fn xml(fields: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:t="http://v8.1c.ru/8.1/data/core" xmlns:x="http://www.w3.org/2001/XMLSchema" xmlns:c="http://v8.1c.ru/8.1/data/enterprise/current-config" version="2.20"><SessionParameter uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>CurrentUser</Name>{fields}</Properties></SessionParameter></MetaDataObject>"#
    )
}

/// The adapter consumes the shared parser's current typed property tree, not a private XML DOM.
fn converted(text: &str) -> Result<SessionParameter, AdapterError> {
    let descriptor = metadata_parser::parse(text, None, PropertiesMode::All).unwrap();
    session_parameter(&descriptor.objects[0])
}

/// Aliases, unions, qualifiers and broad reference `TypeSet` values use the shared type semantics.
#[test]
fn session_parameter_types_match_native_without_fake_managers() {
    for field in [
        "<Type/>",
        "<Type><t:Type>x:string</t:Type><t:StringQualifiers><t:Length>71</t:Length><t:AllowedLength>Variable</t:AllowedLength></t:StringQualifiers></Type>",
        "<Type><t:Type>x:boolean</t:Type><t:Type>c:CatalogRef.Users</t:Type><t:TypeSet>c:AnyRef</t:TypeSet></Type>",
        "<Type><t:Type>t:FixedStructure</t:Type><t:Type>t:ValueStorage</t:Type></Type>",
        "<Type><t:TypeSet>c:DefinedType.UserType</t:TypeSet></Type>",
        "<Type><t:Type xmlns:p='http://v8.1c.ru/8.1/data/ui'>p:Picture</t:Type></Type>",
        "<Type><t:Type>x:decimal</t:Type><t:NumberQualifiers><t:Digits>18</t:Digits><t:FractionDigits>3</t:FractionDigits></t:NumberQualifiers></Type>",
    ] {
        let text = xml(field);
        assert_eq!(
            converted(&text).unwrap(),
            bsl_metadata::xml_parser::parse_session_parameter_xml(&text).unwrap()
        );
    }
    let text = xml(
        "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>20000000-0000-4000-8000-000000000001</ExtendedConfigurationObject><Type><t:Type>x:boolean</t:Type></Type>",
    );
    let model = converted(&text).unwrap();
    assert_eq!(model.object_belonging(), ObjectBelonging::Adopted);
    assert_eq!(
        model,
        bsl_metadata::xml_parser::parse_session_parameter_xml(&text).unwrap()
    );
}

/// Current malformed types and adoption identities cannot inherit an earlier valid input.
#[test]
fn session_parameter_rejects_missing_foreign_future_and_ambiguous_current_types() {
    for fields in [
        "",
        "<Type/><Type/>",
        "<Type><t:Type>unbound:Value</t:Type></Type>",
        "<Type><t:Type xmlns:f='urn:foreign'>f:SessionParameters</t:Type></Type>",
        "<Type><t:Type>c:FutureType</t:Type></Type>",
        "<Type><t:Type>c:CatalogRef.</t:Type></Type>",
        "<Type><t:TypeSet>c:DefinedType.Name.WithDot</t:TypeSet></Type>",
        "<Type><t:TypeSet>c:FutureType</t:TypeSet></Type>",
        "<Type><t:Type>c:AnyRef</t:Type></Type>",
        "<Type><t:Type>x:string</t:Type><t:StringQualifiers><t:Length>bad</t:Length></t:StringQualifiers></Type>",
        "<Type/><ObjectBelonging>Adopted</ObjectBelonging>",
        "<Type/><ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>bad</ExtendedConfigurationObject>",
        "<Type/><ExtendedConfigurationObject>20000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>",
    ] {
        let text = xml(fields);
        assert!(converted(&text).is_err(), "{fields}");
        assert!(
            bsl_metadata::xml_parser::parse_session_parameter_xml(&text).is_err(),
            "{fields}"
        );
    }
}

/// Summary capture and invalid source identities do not produce a ready typed value.
#[test]
fn session_parameter_requires_full_capture_and_real_source_identity() {
    let text = xml("<Type/>");
    let summary = metadata_parser::parse(&text, None, PropertiesMode::Summary).unwrap();
    assert_eq!(
        session_parameter(&summary.objects[0]).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    let invalid = text.replace("10000000-0000-4000-8000-000000000001", "invalid");
    assert_eq!(
        converted(&invalid).unwrap_err().issue,
        AdapterIssue::InvalidUuid
    );
    assert!(bsl_metadata::xml_parser::parse_session_parameter_xml(&invalid).is_err());
    let wrong = text.replace("SessionParameter", "CommonAttribute");
    assert!(matches!(
        converted(&wrong).unwrap_err().issue,
        AdapterIssue::WrongKind { .. }
    ));
    assert!(bsl_metadata::xml_parser::parse_session_parameter_xml(&wrong).is_err());
}
