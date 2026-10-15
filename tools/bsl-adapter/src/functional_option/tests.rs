use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Qualified native input follows the ordinary metadata parser and the current shared property view.
fn converted(properties: &str) -> Result<FunctionalOption, AdapterError> {
    let text = format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:r="http://v8.1c.ru/8.3/xcf/readable" xmlns:i="http://www.w3.org/2001/XMLSchema-instance"><FunctionalOption uuid="80000000-0000-4000-8000-000000000001"><Properties><Name>FeatureЯ</Name>{properties}</Properties></FunctionalOption></MetaDataObject>"#
    );
    let parsed = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    functional_option(&parsed.objects[0])
}

/// Storage kind and interface content retain separate meaning; neither supplies a presumed runtime value.
#[test]
fn functional_option_locations_and_content_use_exact_native_properties() {
    for (location, expected) in [
        (
            "Constant.Storage",
            FunctionalOptionLocation::Constant {
                name: "Storage".into(),
            },
        ),
        (
            "Catalog.Items.Attribute.Stored",
            FunctionalOptionLocation::CatalogAttribute {
                catalog: "Items".into(),
                attribute: "Stored".into(),
            },
        ),
        (
            "InformationRegister.Settings.Resource.Stored",
            FunctionalOptionLocation::InformationRegisterResource {
                register: "Settings".into(),
                resource: "Stored".into(),
            },
        ),
        ("", FunctionalOptionLocation::Empty),
    ] {
        let value = converted(&format!("<Location>{location}</Location><PrivilegedGetMode>true</PrivilegedGetMode><Content><r:Object i:type='r:MDObjectRef'>Catalog.Items.Attribute.Stored</r:Object></Content>")).unwrap();
        assert_eq!(value.location(), Some(&expected));
        assert_eq!(value.privileged_get(), Some(true));
        assert_eq!(
            value.content().unwrap(),
            &["Catalog.Items.Attribute.Stored"]
        );
    }
}

/// Absent borrowed storage inherits only after identity proof; an explicit empty must not reveal the old storage.
#[test]
fn functional_option_adoption_preserves_absent_and_explicit_empty_distinction() {
    let mut base = converted("<Location>Constant.Storage</Location><PrivilegedGetMode>true</PrivilegedGetMode><Content/>").unwrap();
    let identity = "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>80000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>";
    let overlay = converted(&format!(
        "{identity}<Content><r:Object i:type='r:MDObjectRef'>Catalog.Items</r:Object></Content>"
    ))
    .unwrap();
    assert!(overlay.adopts(&base));
    assert_eq!(overlay.location(), None);
    let empty = converted(&format!("{identity}<Location/>")).unwrap();
    assert!(!empty.adopts(&base));
    base.apply_extension_overlay(&overlay);
    assert_eq!(
        base.location(),
        Some(&FunctionalOptionLocation::Constant {
            name: "Storage".into()
        })
    );
    assert_eq!(base.content().unwrap(), &["Catalog.Items"]);
}

/// Unknown storage shapes and malformed explicit flags cannot turn into an inferred Boolean or default policy.
#[test]
fn functional_option_invalid_current_properties_are_rejected() {
    for properties in [
        "",
        "<Location/>",
        "<Location/><Content/>",
        "<Location/><PrivilegedGetMode>1</PrivilegedGetMode><Content/>",
        "<Location/><PrivilegedGetMode>true</PrivilegedGetMode><Content/><Location/>",
        "<ObjectBelonging>Adopted</ObjectBelonging>",
    ] {
        assert!(converted(properties).is_err(), "{properties}");
    }
    for location in [
        "Constant.",
        "Catalog.Items",
        "Document.Order.Attribute.Stored",
        "InformationRegister.Settings.Dimension.Stored",
    ] {
        assert!(converted(&format!("<Location>{location}</Location><PrivilegedGetMode>true</PrivilegedGetMode><Content/>")).is_err(), "{location}");
    }
}

/// Functional-option Content uses qualified Object entries, unlike other metadata reference lists.
#[test]
fn functional_option_content_preserves_native_objects_and_rejects_other_shapes() {
    let head = "<Location>Constant.Storage</Location><PrivilegedGetMode>true</PrivilegedGetMode>";
    let content = "<Content><r:Object>CommonCommand.Open</r:Object><r:Object>Catalog.Items.Attribute.Stored</r:Object></Content>";
    let value = converted(&format!("{head}{content}")).unwrap();
    assert_eq!(
        value.content().unwrap(),
        &["CommonCommand.Open", "Catalog.Items.Attribute.Stored"]
    );
    for invalid in [
        "<Content><r:Item>Catalog.Items</r:Item></Content>",
        "<Content xmlns:f='urn:foreign'><f:Object>Catalog.Items</f:Object></Content>",
        "<Content><r:Object>Catalog.Items</r:Object><r:Object>Catalog.Items</r:Object></Content>",
    ] {
        assert!(converted(&format!("{head}{invalid}")).is_err(), "{invalid}");
    }
}
