use crate::error::{MetadataError, Result};
use crate::metadata_object::PredefinedItem;

/// Preserve the native loader's tolerant legacy behavior for Designer inputs.
pub fn parse_predefined_xml(xml: &str) -> Vec<PredefinedItem> {
    let doc = match roxmltree::Document::parse(xml) {
        Ok(doc) => doc,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to parse Predefined.xml");
            return Vec::new();
        }
    };

    let mut items = Vec::new();

    let root = doc.root_element();
    collect_items(&root, &mut items);

    tracing::debug!(count = items.len(), "Parsed predefined items");
    items
}

/// Parse an explicit current resource without treating malformed XML as a valid empty list.
///
/// # Errors
/// Returns XML syntax errors or a root other than `PredefinedData`.
/// Item extraction retains the native loader's name, UUID and hierarchy semantics.
pub fn try_parse_predefined_xml(xml: &str) -> Result<Vec<PredefinedItem>> {
    let doc = super::helpers::parse_xml(xml)?;
    let root = doc.root_element();
    if root.tag_name().name() != "PredefinedData" {
        return Err(MetadataError::InvalidFormat(format!(
            "expected a PredefinedData element, found {}",
            root.tag_name().name()
        )));
    }
    let mut items = Vec::new();
    collect_items(&root, &mut items);
    Ok(items)
}

/// Keep the same ordered native item projection for both tolerant and checked entry points.
fn collect_items(node: &roxmltree::Node<'_, '_>, items: &mut Vec<PredefinedItem>) {
    for child in node.children() {
        if !child.is_element() || child.tag_name().name() != "Item" {
            continue;
        }

        let uuid = child.attribute("id").unwrap_or("").to_string();

        let name = child
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "Name")
            .and_then(|n| n.text())
            .unwrap_or("")
            .to_string();

        if !name.is_empty() {
            items.push(PredefinedItem { name, name_en: None, uuid });
        }

        if let Some(child_items) =
            child.children().find(|n| n.is_element() && n.tag_name().name() == "ChildItems")
        {
            collect_items(&child_items, items);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A checked current resource must distinguish wrong-root and malformed input from no items.
    #[test]
    fn checked_predefined_xml_rejects_invalid_resources() {
        for text in ["<PredefinedData>", "<Other/>", "not XML", ""] {
            assert!(try_parse_predefined_xml(text).is_err());
        }
        assert!(try_parse_predefined_xml("<PredefinedData/>").unwrap().is_empty());
    }

    /// BOM, Unicode, UUID strings and hierarchy order use the same extraction as the native loader.
    #[test]
    fn checked_predefined_xml_retains_native_item_semantics() {
        let xml = "\u{feff}<PredefinedData><!-- Я😀 --><Item id=\"parent\"><Name>Parent</Name><ChildItems><Item id=\"child\"><Name>Child</Name></Item><Item id=\"empty\"><Name/></Item></ChildItems></Item></PredefinedData>\r\n";
        let current = try_parse_predefined_xml(xml).unwrap();
        assert_eq!(current, parse_predefined_xml(xml));
        assert_eq!(current.len(), 2);
        assert_eq!(current[1].name, "Child");
        assert_eq!(current[1].uuid, "child");
        assert!(current.iter().all(|item| item.name_en.is_none()));
    }

    #[test]
    fn test_parse_predefined_xml_flat() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<PredefinedData xmlns="http://v8.1c.ru/8.3/xcf/predef"
                xsi:type="CatalogPredefinedItems"
                version="2.20"
                xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
    <Item id="11111111-1111-1111-1111-111111111111">
        <Name>EmailПартнера</Name>
        <Code/>
        <Description>Электронная почта</Description>
        <IsFolder>false</IsFolder>
    </Item>
    <Item id="22222222-2222-2222-2222-222222222222">
        <Name>ТелефонПартнера</Name>
        <Code/>
        <Description>Телефон</Description>
        <IsFolder>false</IsFolder>
    </Item>
</PredefinedData>"#;

        let items = parse_predefined_xml(xml);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "EmailПартнера");
        assert_eq!(items[0].uuid, "11111111-1111-1111-1111-111111111111");
        assert!(items[0].name_en.is_none());
        assert_eq!(items[1].name, "ТелефонПартнера");
        assert_eq!(items[1].uuid, "22222222-2222-2222-2222-222222222222");
    }

    #[test]
    fn test_parse_predefined_xml_hierarchical() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<PredefinedData xmlns="http://v8.1c.ru/8.3/xcf/predef"
                xsi:type="CatalogPredefinedItems"
                version="2.20"
                xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
    <Item id="11111111-1111-1111-1111-111111111111">
        <Name>РодительскийЭлемент</Name>
        <Code/>
        <Description>Папка</Description>
        <IsFolder>true</IsFolder>
        <ChildItems>
            <Item id="22222222-2222-2222-2222-222222222222">
                <Name>ДочернийЭлемент1</Name>
                <Code/>
                <Description>Дочерний 1</Description>
                <IsFolder>false</IsFolder>
            </Item>
            <Item id="33333333-3333-3333-3333-333333333333">
                <Name>ДочернийЭлемент2</Name>
                <Code/>
                <Description>Дочерний 2</Description>
                <IsFolder>false</IsFolder>
            </Item>
        </ChildItems>
    </Item>
    <Item id="44444444-4444-4444-4444-444444444444">
        <Name>КорневойЭлемент</Name>
        <Code/>
        <Description>Корневой</Description>
        <IsFolder>false</IsFolder>
    </Item>
</PredefinedData>"#;

        let items = parse_predefined_xml(xml);

        assert_eq!(items.len(), 4);
        assert_eq!(items[0].name, "РодительскийЭлемент");
        assert_eq!(items[0].uuid, "11111111-1111-1111-1111-111111111111");
        assert_eq!(items[1].name, "ДочернийЭлемент1");
        assert_eq!(items[1].uuid, "22222222-2222-2222-2222-222222222222");
        assert_eq!(items[2].name, "ДочернийЭлемент2");
        assert_eq!(items[2].uuid, "33333333-3333-3333-3333-333333333333");
        assert_eq!(items[3].name, "КорневойЭлемент");
        assert_eq!(items[3].uuid, "44444444-4444-4444-4444-444444444444");
    }

    #[test]
    fn test_parse_predefined_xml_empty() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<PredefinedData xmlns="http://v8.1c.ru/8.3/xcf/predef"
                xsi:type="CatalogPredefinedItems"
                version="2.20"
                xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
</PredefinedData>"#;

        let items = parse_predefined_xml(xml);
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_predefined_xml_invalid() {
        let items = parse_predefined_xml("not valid xml <><");
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_predefined_xml_skips_empty_name() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<PredefinedData xmlns="http://v8.1c.ru/8.3/xcf/predef"
                xsi:type="CatalogPredefinedItems"
                version="2.20"
                xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
    <Item id="11111111-1111-1111-1111-111111111111">
        <Name></Name>
        <IsFolder>false</IsFolder>
    </Item>
    <Item id="22222222-2222-2222-2222-222222222222">
        <Name>НормальныйЭлемент</Name>
        <IsFolder>false</IsFolder>
    </Item>
</PredefinedData>"#;

        let items = parse_predefined_xml(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "НормальныйЭлемент");
    }
}
