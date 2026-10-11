//! Fixtures edit native property identity instead of matching a serialized XML representation.

use eska::project_core::source_format::{
    ObjectSource, Property, PropertyName, PropertyValue, Section, XmlFragment,
};

/// Produce a current boolean or deliberately unsupported native Server value in one YAML copy.
pub fn server_value(input: &str, value: &str) -> String {
    value.parse::<bool>().map_or_else(
        |_| {
            assert_eq!(value, "Future");
            replace(
                input,
                PropertyName::Server,
                Property::Preserved {
                    fragment: XmlFragment {
                        xml: "<Server>Future</Server>".into(),
                        namespaces: vec![],
                    },
                },
            )
        },
        |value| boolean_value(input, PropertyName::Server, value),
    )
}

/// Change a current boolean flag independently of the YAML serialization style.
pub fn boolean_value(input: &str, name: PropertyName, value: bool) -> String {
    replace(
        input,
        name,
        Property::Typed {
            name,
            value: PropertyValue::Boolean(value),
        },
    )
}

/// Select a declared property by identity and require the fixture to actually change.
fn replace(input: &str, name: PropertyName, replacement: Property) -> String {
    let mut owner = ObjectSource::from_yaml(input).unwrap();
    let properties = owner
        .document
        .descriptor
        .object
        .sections
        .iter_mut()
        .find_map(|section| {
            if let Section::Properties(properties) = section {
                Some(properties)
            } else {
                None
            }
        })
        .unwrap();
    let target = properties
        .iter_mut()
        .find(|property| {
            matches!(
                property,
                Property::Typed {
                    name: current,
                    ..
                } if *current == name
            )
        })
        .unwrap();
    *target = replacement;
    assert_ne!(ObjectSource::from_yaml(input).unwrap(), owner);
    serde_json::to_string(&owner).unwrap()
}
