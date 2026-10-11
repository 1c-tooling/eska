//! A real import provides two organisational containers without unrelated resource consumers.

use std::{fs, path::Path};

/// Extend only the owned native fixture and publish a separate source directory.
pub fn add_subsystems(root: &Path, native: &Path) {
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace(
            "</ChildObjects>",
            "<Subsystem>Services</Subsystem><Subsystem>Neighbor</Subsystem></ChildObjects>",
        ),
    )
    .unwrap();
    fs::create_dir(native.join("Subsystems")).unwrap();
    for (name, uuid, member) in [
        (
            "Services",
            "70000000-0000-4000-8000-000000000001",
            "Catalog.Items",
        ),
        (
            "Neighbor",
            "80000000-0000-4000-8000-000000000001",
            "Document.Receipt",
        ),
    ] {
        fs::write(native.join(format!("Subsystems/{name}.xml")),format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:xr="http://v8.1c.ru/8.3/xcf/readable" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" version="2.20"><Subsystem uuid="{uuid}"><Properties><Name>{name}</Name><Content><xr:Item xsi:type="xr:MDObjectRef">{member}</xr:Item></Content></Properties></Subsystem></MetaDataObject>"#)).unwrap();
    }
    eska::project_core::source_format::import_designer(native, &root.join("src-with-subsystems"))
        .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-subsystems'",
        ),
    )
    .unwrap();
}
