use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Use the ordinary metadata parser so namespace and duplicate checks exercise real input.
fn converted(properties: &str) -> Result<FilterCriterion, AdapterError> {
    let text = format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:t="http://v8.1c.ru/8.1/data/core" xmlns:x="http://www.w3.org/2001/XMLSchema" xmlns:r="http://v8.1c.ru/8.3/xcf/readable" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><FilterCriterion uuid="70000000-0000-4000-8000-000000000001"><Properties><Name>Linked</Name>{properties}</Properties></FilterCriterion></MetaDataObject>"#
    );
    let parsed = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    filter_criterion(&parsed.objects[0])
}

/// Distinct fields keep their full paths while Find returns their enclosing object references.
#[test]
fn search_type_and_qname_aware_content_remain_separate() {
    let value = converted("<Type><t:Type>x:string</t:Type></Type><Content><r:Item xsi:type='r:MDObjectRef'>Catalog.Items.Attribute.Key</r:Item><r:Item xsi:type='r:MDObjectRef'>Document.Orders.TabularSection.Rows.Attribute.Key</r:Item></Content>").unwrap();
    assert!(matches!(
        value.search_type(),
        Some(bsl_metadata::AttributeType::String { .. })
    ));
    let content = value.content().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].kind, MdoType::Catalog);
    assert_eq!(content[1].name, "Orders");
    assert_eq!(
        content[1].path,
        "Document.Orders.TabularSection.Rows.Attribute.Key"
    );
}

/// A borrowed declaration needs the original UUID and cannot replace its configured search type.
#[test]
fn borrowed_content_adds_without_removing_base_fields() {
    let mut base = converted("<Type><t:Type>x:boolean</t:Type></Type><Content><r:Item xsi:type='r:MDObjectRef'>Catalog.Items.Attribute.Key</r:Item></Content>").unwrap();
    let extra = "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>70000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>";
    let overlay = converted(&format!("{extra}<Content><r:Item xsi:type='r:MDObjectRef'>Document.Orders.Attribute.Key</r:Item></Content>")).unwrap();
    assert!(overlay.adopts(&base));
    base.apply_extension_overlay(&overlay);
    assert_eq!(base.content().unwrap().len(), 2);
    assert_eq!(
        base.search_type(),
        Some(&bsl_metadata::AttributeType::Boolean)
    );
    let changed = converted(&format!("{extra}<Type><t:Type>x:string</t:Type></Type>")).unwrap();
    assert!(!changed.adopts(&base));
}

/// Unknown families and malformed or duplicate membership must not yield a usable criterion.
#[test]
fn malformed_current_criteria_are_rejected() {
    for properties in [
        String::new(),
        "<Type/>".into(),
        "<Type/><Content/><UseStandardCommands>1</UseStandardCommands>".into(),
        "<Type/><Content/><ObjectBelonging>Adopted</ObjectBelonging>".into(),
        "<Type/><Content/><Type/>".into(),
        "<Type><t:Type>x:future</t:Type></Type><Content/>".into(),
    ] {
        assert!(converted(&properties).is_err(), "{properties}");
    }
    for path in [
        "Catalog.Items",
        "Catalog..Attribute.Key",
        "Catalog.Items.Attribute.",
        "InformationRegister.Rows.Attribute.Key",
    ] {
        assert!(
            converted(&format!(
                "<Type/><Content><r:Item xsi:type='r:MDObjectRef'>{path}</r:Item></Content>"
            ))
            .is_err(),
            "{path}"
        );
    }
    assert!(converted("<Type/><Content><r:Item xsi:type='r:MDObjectRef'>Catalog.Items.Attribute.Key</r:Item><r:Item xsi:type='r:MDObjectRef'>catalog.items.attribute.key</r:Item></Content>").is_err());
    assert!(converted("<Type/><Content><t:ItemMDObjectRef>Catalog.Items.Attribute.Key</t:ItemMDObjectRef></Content>").is_err());
    assert!(
        converted("<Type/><Content/>")
            .unwrap()
            .content()
            .unwrap()
            .is_empty()
    );
}
