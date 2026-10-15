//! Tiny native-origin declarations exercise the shared input lifecycle without production content.

use std::{fs, path::Path};

/// Add the native flat family to this fixture only, then import a new authoritative source tree.
pub fn add_parameters(root: &Path, native: &Path) {
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(native.join("Configuration.xml"), configuration.replace("</ChildObjects>", "<Catalog>Items</Catalog><SessionParameter>CurrentUser</SessionParameter><SessionParameter>Neighbor</SessionParameter><SessionParameter>SelectedUser</SessionParameter></ChildObjects>")).unwrap();
    fs::create_dir(native.join("Catalogs")).unwrap();
    fs::write(native.join("Catalogs/Items.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Catalog uuid="80000000-0000-4000-8000-000000000001"><Properties><Name>Items</Name></Properties></Catalog></MetaDataObject>"#).unwrap();
    fs::create_dir(native.join("SessionParameters")).unwrap();
    for (name, id, ty) in [
        (
            "CurrentUser",
            1,
            "<t:Type>x:string</t:Type><t:StringQualifiers><t:Length>20</t:Length></t:StringQualifiers>",
        ),
        ("Neighbor", 2, "<t:Type>x:boolean</t:Type>"),
        ("SelectedUser", 3, "<t:Type>c:CatalogRef.Items</t:Type>"),
    ] {
        let xml = format!(
            r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:t="http://v8.1c.ru/8.1/data/core" xmlns:x="http://www.w3.org/2001/XMLSchema" xmlns:c="http://v8.1c.ru/8.1/data/enterprise/current-config" version="2.20"><SessionParameter uuid="70000000-0000-4000-8000-{id:012}"><Properties><Name>{name}</Name><Type>{ty}</Type></Properties></SessionParameter></MetaDataObject>"#
        );
        fs::write(native.join(format!("SessionParameters/{name}.xml")), xml).unwrap();
    }
    eska::project_core::source_format::import_designer(native, &root.join("src-with-parameters"))
        .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        config.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-parameters'",
        ),
    )
    .unwrap();
}
