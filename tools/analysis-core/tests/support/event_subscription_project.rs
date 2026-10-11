//! A real source import supplies the same two subscriptions to SDK and process tests.

use std::{fs, path::Path};

/// Extend only an owned fixture, then publish one new source directory through the normal importer.
pub fn add_subscriptions(root: &Path, native: &Path) {
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace("</ChildObjects>", "<EventSubscription>BeforeSave</EventSubscription><EventSubscription>Neighbor</EventSubscription></ChildObjects>"),
    ).unwrap();
    fs::create_dir(native.join("EventSubscriptions")).unwrap();
    for (name, uuid, method) in [
        (
            "BeforeSave",
            "30000000-0000-4000-8000-000000000001",
            "Saved",
        ),
        ("Neighbor", "40000000-0000-4000-8000-000000000001", "Other"),
    ] {
        fs::write(native.join(format!("EventSubscriptions/{name}.xml")),format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" version="2.20"><EventSubscription uuid="{uuid}"><Properties><Name>{name}</Name><Comment/><Source><v8:Type>cfg:CatalogObject.Items</v8:Type><v8:TypeSet>cfg:DocumentObject</v8:TypeSet></Source><Event>BeforeWrite</Event><Handler>CommonModule.Tools.{method}</Handler></Properties></EventSubscription></MetaDataObject>"#)).unwrap();
    }
    eska::project_core::source_format::import_designer(
        native,
        &root.join("src-with-subscriptions"),
    )
    .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-subscriptions'",
        ),
    )
    .unwrap();
}
