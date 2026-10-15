use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Native qualified Use is parsed by the shared metadata parser before adapter projection.
fn converted(properties: &str) -> Result<FunctionalOptionsParameter, AdapterError> {
    let text = format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:r="http://v8.1c.ru/8.3/xcf/readable" xmlns:i="http://www.w3.org/2001/XMLSchema-instance"><FunctionalOptionsParameter uuid="a0000000-0000-4000-8000-000000000001"><Properties><Name>OrganizationЯ</Name>{properties}</Properties></FunctionalOptionsParameter></MetaDataObject>"#
    );
    let parsed = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    functional_option_parameter(&parsed.objects[0])
}

/// Catalog and dimension targets preserve their full native identities and order.
#[test]
fn parameter_qualified_use_preserves_targets() {
    let value = converted("<Use><r:Item i:type='r:MDObjectRef'>Catalog.Items</r:Item><r:Item>InformationRegister.Settings.Dimension.Organization</r:Item></Use>").unwrap();
    let uses = value.uses().unwrap();
    assert_eq!(uses.len(), 2);
    assert_eq!(uses[0].owner(), (bsl_metadata::MdoType::Catalog, "Items"));
    assert!(
        matches!(&uses[1], FunctionalOptionParameterUse::InformationRegisterDimension { register, dimension } if register == "Settings" && dimension == "Organization")
    );
    assert_eq!(converted("<Use/>").unwrap().uses(), Some([].as_slice()));
}

/// Own absence, wrong target categories/namespaces and case-insensitive duplicates never become ready models.
#[test]
fn parameter_use_refuses_unknown_and_ambiguous_targets() {
    for invalid in [
        "",
        "<Use><r:Item>Document.Order</r:Item></Use>",
        "<Use><r:Item>InformationRegister.Settings.Resource.Value</r:Item></Use>",
        "<Use xmlns:f='urn:foreign'><f:Item>Catalog.Items</f:Item></Use>",
        "<Use><r:Item>Catalog.Items</r:Item><r:Item>Catalog.items</r:Item></Use>",
        "<Use/><Use/>",
    ] {
        assert!(converted(invalid).is_err(), "{invalid}");
    }
}

/// Exact original identity permits inherited/equivalent targets, while explicit empty or changed Use refuses.
#[test]
fn parameter_borrowed_use_requires_proven_original_targets() {
    let mut base = converted("<Use><r:Item>Catalog.Items</r:Item></Use>").unwrap();
    let identity = "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>a0000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>";
    let inherited = converted(identity).unwrap();
    assert!(inherited.adopts(&base));
    assert!(
        converted(&format!(
            "{identity}<Use><r:Item>Catalog.items</r:Item></Use>"
        ))
        .unwrap()
        .adopts(&base)
    );
    for uses in ["<Use/>", "<Use><r:Item>Catalog.Other</r:Item></Use>"] {
        assert!(
            !converted(&format!("{identity}{uses}"))
                .unwrap()
                .adopts(&base)
        );
    }
    base.apply_extension_overlay(&inherited);
    assert_eq!(base.uses().unwrap().len(), 1);
}
