//! Typed completion fixture exists only within its own analyzer process tests.

use super::support;
use std::{fs, path::PathBuf};

/// Create a supported catalog attribute and source module in a unique playground directory.
pub fn project(body: &str) -> (support::TestDirectory, PathBuf) {
    let (directory, root) = support::project("// Current common module.\n", false);
    let native = directory.0.join("native");
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace(
            "<CommonModule>Tools</CommonModule>",
            "<CommonModule>Tools</CommonModule><Catalog>Sample</Catalog>",
        ),
    )
    .unwrap();
    fs::create_dir_all(native.join("Catalogs/Sample/Ext")).unwrap();
    fs::write(native.join("Catalogs/Sample.xml"),"<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"><Catalog uuid=\"30000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name></Properties><ChildObjects><Attribute uuid=\"40000000-0000-4000-8000-000000000001\"><Properties><Name>Description</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>").unwrap();
    fs::write(native.join("Catalogs/Sample/Ext/ObjectModule.bsl"), body).unwrap();
    eska::project_core::source_format::import_designer(&native, &root.join("src-completion"))
        .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        config.replace(
            "source_format='eska'",
            "source='src-completion'\nsource_format='eska'",
        ),
    )
    .unwrap();
    (directory, root)
}
