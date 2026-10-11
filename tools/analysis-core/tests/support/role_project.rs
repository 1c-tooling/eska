//! Declared roles distinguish preserved rights resources from genuinely absent rights.

use std::{fs, path::Path};

/// Keep verbatim resource bytes, including BOM and CRLF, shared by resident and process tests.
pub fn rights(member: &str, restricted: bool) -> String {
    let restriction = if restricted {
        "<restrictionByCondition><condition>  WHERE Current = TRUE  </condition></restrictionByCondition>"
    } else {
        ""
    };
    format!(
        "\u{feff}<Rights xmlns=\"http://v8.1c.ru/8.2/roles\"><setForNewObjects>{restricted}</setForNewObjects><setForAttributesByDefault>true</setForAttributesByDefault><independentRightsOfChildObjects>{restricted}</independentRightsOfChildObjects><object><name>{member}</name><right><name>Read</name><value>true</value>{restriction}</right></object></Rights>\r\n"
    )
}

/// Extend only the owned native fixture and import into a new source directory.
pub fn add_roles(root: &Path, native: &Path) {
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace(
            "</ChildObjects>",
            "<Role>Reader</Role><Role>Neighbor</Role><Role>Absent</Role></ChildObjects>",
        ),
    )
    .unwrap();
    fs::create_dir(native.join("Roles")).unwrap();
    for (name, suffix, member) in [
        ("Reader", 1, Some("Catalog.Items.Attribute.Code")),
        ("Neighbor", 2, Some("Document.Receipt")),
        ("Absent", 3, None),
    ] {
        fs::write(native.join(format!("Roles/{name}.xml")), format!(
            "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Role uuid=\"70000000-0000-4000-8000-00000000000{suffix}\"><Properties><Name>{name}</Name></Properties></Role></MetaDataObject>"
        )).unwrap();
        if let Some(member) = member {
            let directory = native.join(format!("Roles/{name}/Ext"));
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("Rights.xml"), rights(member, false)).unwrap();
        }
    }
    eska::project_core::source_format::import_designer(native, &root.join("src-with-roles"))
        .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-roles'",
        ),
    )
    .unwrap();
}
