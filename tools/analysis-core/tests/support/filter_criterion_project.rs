//! Real Designer input creates separate criterion descriptors and native reference targets.

use std::{fs, path::Path};

/// Add independent object fields and a criterion, optionally borrowed by exact original UUID.
pub fn add_criteria(root: &Path, native: &Path, extension: bool, uuid: &str, extra: &str) {
    let path = native.join("Configuration.xml");
    let children = if extension {
        "<FilterCriterion>Linked</FilterCriterion>"
    } else {
        "<Catalog>Items</Catalog><Document>Orders</Document><FilterCriterion>Linked</FilterCriterion>"
    };
    let mut configuration = fs::read_to_string(&path)
        .unwrap()
        .replace("</ChildObjects>", &format!("{children}</ChildObjects>"));
    if extension {
        configuration = configuration.replace("<Name>SyntaxFixture</Name>", "<Name>Extension</Name><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>").replace("10000000-0000-4000-8000-000000000001", &uuid.replacen("70000000", "10000000", 1));
    }
    fs::write(path, configuration).unwrap();
    for (family, kind, name, field, prefix) in [
        ("Catalogs", "Catalog", "Items", "Choice", "30000000"),
        ("Documents", "Document", "Orders", "Other", "40000000"),
    ]
    .into_iter()
    .filter(|_| !extension)
    {
        fs::create_dir_all(native.join(family)).unwrap();
        let object_uuid = uuid.replacen("70000000", prefix, 1);
        let attribute_uuid = uuid.replacen(
            "70000000",
            if kind == "Catalog" {
                "a0000000"
            } else {
                "b0000000"
            },
            1,
        );
        let key_uuid = uuid.replacen(
            "70000000",
            if kind == "Catalog" {
                "c0000000"
            } else {
                "d0000000"
            },
            1,
        );
        fs::write(native.join(format!("{family}/{name}.xml")), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:t="http://v8.1c.ru/8.1/data/core" xmlns:x="http://www.w3.org/2001/XMLSchema" version="2.20"><{kind} uuid="{object_uuid}"><Properties><Name>{name}</Name></Properties><ChildObjects><Attribute uuid="{attribute_uuid}"><Properties><Name>{field}</Name><Type><t:Type>x:boolean</t:Type></Type></Properties></Attribute><Attribute uuid="{key_uuid}"><Properties><Name>Key</Name><Type><t:Type>x:string</t:Type></Type></Properties></Attribute></ChildObjects></{kind}></MetaDataObject>"#)).unwrap();
    }
    fs::create_dir_all(native.join("FilterCriteria")).unwrap();
    let properties = if extra.contains("<ObjectBelonging>Adopted</ObjectBelonging>") {
        ""
    } else {
        "<Type><t:Type>x:string</t:Type></Type><Content><r:Item xsi:type='r:MDObjectRef'>Catalog.Items.Attribute.Key</r:Item></Content>"
    };
    fs::write(native.join("FilterCriteria/Linked.xml"), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:t="http://v8.1c.ru/8.1/data/core" xmlns:x="http://www.w3.org/2001/XMLSchema" xmlns:r="http://v8.1c.ru/8.3/xcf/readable" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" version="2.20"><FilterCriterion uuid="{uuid}"><Properties><Name>Linked</Name>{properties}{extra}<UseStandardCommands>false</UseStandardCommands></Properties></FilterCriterion></MetaDataObject>"#)).unwrap();
    eska::project_core::source_format::import_designer_queries(
        native,
        &root.join("src-with-criteria"),
    )
    .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap().replace(
        "source_format='eska'",
        "source_format='eska'\nsource='src-with-criteria'",
    );
    fs::write(
        root.join("eska.toml"),
        if extension {
            config.replace("type='configuration'", "type='extension'")
        } else {
            config
        },
    )
    .unwrap();
}
