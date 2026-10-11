//! Independent common-module declarations exercise native visibility without reading neighboring bodies.

use std::{fs, path::Path};

pub const SAVED: &str = "\u{feff}// Я😀\r\nProcedure SavedUtility(Value = 1) Export\r\nEndProcedure\r\nProcedure PrivateUtility()\r\nEndProcedure\r\n";
pub const CURRENT: &str = "Procedure CurrentUtility(Value = \"😀\") Export\r\nEndProcedure\r\n";

/// Import declared text/binary/absent bodies and a mixed-environment caller into one fresh owned source.
pub fn add_common_modules(root: &Path, native: &Path) {
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    let names = ["Utility", "Neighbor", "Binary", "Bodyless"];
    let children = names
        .map(|name| format!("<CommonModule>{name}</CommonModule>"))
        .join("");
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace("</ChildObjects>", &(children + "</ChildObjects>")),
    )
    .unwrap();
    let caller = fs::read_to_string(native.join("CommonModules/Tools.xml")).unwrap();
    fs::write(
        native.join("CommonModules/Tools.xml"),
        caller.replace(
            "</Properties>",
            "<ClientManagedApplication>true</ClientManagedApplication></Properties>",
        ),
    )
    .unwrap();
    for (index, name) in names.iter().enumerate() {
        fs::write(native.join(format!("CommonModules/{name}.xml")), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><CommonModule uuid=\"90000000-0000-4000-8000-00000000000{}\"><Properties><Name>{name}</Name><Global>false</Global><Server>true</Server><ClientManagedApplication>false</ClientManagedApplication><ServerCall>true</ServerCall></Properties></CommonModule></MetaDataObject>",index+1)).unwrap();
        let directory = native.join(format!("CommonModules/{name}/Ext"));
        fs::create_dir_all(&directory).unwrap();
        match *name {
            "Utility" => fs::write(directory.join("Module.bsl"), SAVED).unwrap(),
            "Neighbor" => fs::write(
                directory.join("Module.bsl"),
                "Procedure NeighborUtility() Export\nEndProcedure\n",
            )
            .unwrap(),
            "Binary" => fs::write(directory.join("Module.bin"), [0xff, 0xfe]).unwrap(),
            _ => {}
        }
    }
    eska::project_core::source_format::import_designer(
        native,
        &root.join("src-with-common-modules"),
    )
    .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-common-modules'",
        ),
    )
    .unwrap();
}
