use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// The shared parser supplies property identity and coordinates without a command-specific XML reader.
fn converted(properties: &str) -> Result<CommonCommand, AdapterError> {
    let text = format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:t="http://v8.1c.ru/8.1/data/core" xmlns:xs="http://www.w3.org/2001/XMLSchema"><CommonCommand uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>RunЯ</Name>{properties}</Properties></CommonCommand></MetaDataObject>"#
    );
    let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    common_command(&descriptor.objects[0])
}

/// Single/multiple modes preserve the same native parameter type; explicit empty stays Unknown.
#[test]
fn current_parameter_type_modes_and_empty_description_are_distinct() {
    for (mode, usage) in [
        ("Single", CommandParameterUseMode::Single),
        ("Multiple", CommandParameterUseMode::Multiple),
    ] {
        let command = converted(&format!("<CommandParameterType><t:Type>xs:boolean</t:Type></CommandParameterType><ParameterUseMode>{mode}</ParameterUseMode><ModifiesData>false</ModifiesData>")).unwrap();
        assert_eq!(
            command.parameter_type(),
            Some(&bsl_metadata::AttributeType::Boolean)
        );
        assert_eq!(command.parameter_use(), Some(usage));
    }
    assert_eq!(
        converted("<CommandParameterType/><ParameterUseMode>Single</ParameterUseMode>")
            .unwrap()
            .parameter_type(),
        Some(&bsl_metadata::AttributeType::Unknown)
    );
}

/// Borrowed properties inherit only after the native caller proves the exact original UUID.
#[test]
fn borrowed_command_retains_absence_and_supplied_properties() {
    let mut base = converted("<CommandParameterType><t:Type>xs:boolean</t:Type></CommandParameterType><ParameterUseMode>Single</ParameterUseMode>").unwrap();
    let overlay = converted("<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>10000000-0000-4000-8000-000000000001</ExtendedConfigurationObject><ParameterUseMode>Multiple</ParameterUseMode>").unwrap();
    assert!(overlay.adopts(&base));
    assert_eq!(overlay.parameter_type(), None);
    base.apply_extension_overlay(&overlay);
    assert_eq!(
        base.parameter_type(),
        Some(&bsl_metadata::AttributeType::Boolean)
    );
    assert_eq!(
        base.parameter_use(),
        Some(CommandParameterUseMode::Multiple)
    );
}

/// A readable document cannot turn unknown enums, missing declarations or malformed booleans into Ready.
#[test]
fn invalid_command_values_do_not_become_defaults() {
    let valid = "<CommandParameterType/><ParameterUseMode>Single</ParameterUseMode>";
    for text in [
        String::new(), valid.replace("Single", "Future"), format!("{valid}<ParameterUseMode>Multiple</ParameterUseMode>"),
        format!("{valid}<ModifiesData>1</ModifiesData>"), format!("{valid}<ObjectBelonging>Adopted</ObjectBelonging>"),
        "<CommandParameterType><t:Type>xs:future</t:Type></CommandParameterType><ParameterUseMode>Single</ParameterUseMode>".into(),
    ] { assert!(converted(&text).is_err(), "{text}"); }
}
