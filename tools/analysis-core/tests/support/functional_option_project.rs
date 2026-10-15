//! Native storage owners make configured functional-option types observable to ordinary BSL inference.

use std::{fs, path::Path};

/// Import only test-owned native objects with distinct UUIDs and explicit current storage.
pub fn add_options(root: &Path, native: &Path) {
    let path = native.join("Configuration.xml");
    let text = fs::read_to_string(&path).unwrap().replace("</ChildObjects>", "<Constant>Storage</Constant><Catalog>Items</Catalog><InformationRegister>Settings</InformationRegister><FunctionalOption>FeatureЯ</FunctionalOption><FunctionalOption>Other</FunctionalOption></ChildObjects>");
    fs::write(path, text).unwrap();

    for (family, name, kind, uuid, properties, children) in [
        (
            "Constants",
            "Storage",
            "Constant",
            "30000000-0000-4000-8000-000000000001",
            "<Name>Storage</Name><Type><t:Type>x:string</t:Type></Type>",
            "",
        ),
        (
            "Catalogs",
            "Items",
            "Catalog",
            "40000000-0000-4000-8000-000000000001",
            "<Name>Items</Name>",
            "<Attribute uuid='50000000-0000-4000-8000-000000000001'><Properties><Name>Stored</Name><Type><t:Type>x:boolean</t:Type></Type></Properties></Attribute>",
        ),
        (
            "InformationRegisters",
            "Settings",
            "InformationRegister",
            "60000000-0000-4000-8000-000000000001",
            "<Name>Settings</Name><InformationRegisterPeriodicity>Nonperiodical</InformationRegisterPeriodicity>",
            "<Resource uuid='70000000-0000-4000-8000-000000000001'><Properties><Name>Stored</Name><Type><t:Type>x:dateTime</t:Type></Type></Properties></Resource>",
        ),
    ] {
        fs::create_dir_all(native.join(family)).unwrap();
        fs::write(
            native.join(format!("{family}/{name}.xml")),
            descriptor(kind, uuid, properties, children),
        )
        .unwrap();
    }
    fs::create_dir_all(native.join("FunctionalOptions")).unwrap();
    for (name, uuid) in [
        ("FeatureЯ", "80000000-0000-4000-8000-000000000001"),
        ("Other", "80000000-0000-4000-8000-000000000002"),
    ] {
        fs::write(native.join(format!("FunctionalOptions/{name}.xml")), descriptor("FunctionalOption", uuid, &format!("<Name>{name}</Name><Location>Constant.Storage</Location><PrivilegedGetMode>true</PrivilegedGetMode><Content xmlns:r='http://v8.1c.ru/8.3/xcf/readable'><r:Object>Catalog.Items</r:Object></Content>"), "")).unwrap();
    }
    eska::project_core::source_format::import_designer_queries(
        native,
        &root.join("src-with-options"),
    )
    .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap().replace(
        "source_format='eska'",
        "source_format='eska'\nsource='src-with-options'",
    );
    fs::write(root.join("eska.toml"), config).unwrap();
}

/// Each descriptor retains its native namespace while accepting independently owned property text.
fn descriptor(kind: &str, uuid: &str, properties: &str, children: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:t="http://v8.1c.ru/8.1/data/core" xmlns:x="http://www.w3.org/2001/XMLSchema" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" version="2.20"><{kind} uuid="{uuid}"><Properties>{properties}</Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>"#
    )
}
