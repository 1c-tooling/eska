//! Settings-storage fixtures enroll real native metadata and manager bodies through the common importer.

use std::{fs, path::Path};

pub const SAVED: &str = "\u{feff}// Я😀\r\nProcedure SavedMethod(Value = 1) Export\r\nEndProcedure\r\nProcedure PrivateMethod()\r\nEndProcedure\r\n";
pub const CURRENT: &str =
    "\u{feff}// Я😀 current\r\nProcedure CurrentMethod(Value = 2) Export\r\nEndProcedure\r\n";

/// Optional borrowed ownership retains the original UUID; the module is a declared source, never an XML sidecar.
pub fn add_storage(
    root: &Path,
    native: &Path,
    body: &str,
    extension: bool,
    uuid: &str,
    extra: &str,
) {
    let config_path = native.join("Configuration.xml");
    let mut configuration = fs::read_to_string(&config_path).unwrap().replace(
        "</ChildObjects>",
        if extension {
            "<SettingsStorage>CurrentЯ</SettingsStorage></ChildObjects>"
        } else {
            "<SettingsStorage>CurrentЯ</SettingsStorage><Catalog>Unrelated</Catalog></ChildObjects>"
        },
    );
    if extension {
        configuration = configuration
            .replace("<Name>SyntaxFixture</Name>", "<Name>Extension</Name><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>")
            .replace("10000000-0000-4000-8000-000000000001", &uuid.replacen("70000000", "10000000", 1));
        let path = native.join("CommonModules/Tools.xml");
        let module = fs::read_to_string(&path).unwrap().replace(
            "20000000-0000-4000-8000-000000000001",
            &uuid.replacen("70000000", "20000000", 1),
        );
        fs::write(path, module).unwrap();
    }
    fs::write(config_path, configuration).unwrap();
    if !extension {
        fs::create_dir_all(native.join("Catalogs")).unwrap();
        fs::write(native.join("Catalogs/Unrelated.xml"), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'><Catalog uuid='{}'><Properties><Name>Unrelated</Name></Properties></Catalog></MetaDataObject>", uuid.replacen("70000000", "30000000", 1))).unwrap();
    }
    fs::create_dir_all(native.join("SettingsStorages/CurrentЯ/Ext")).unwrap();
    fs::write(native.join("SettingsStorages/CurrentЯ.xml"), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'><SettingsStorage uuid='{uuid}'><Properties><Name>CurrentЯ</Name>{extra}</Properties><ChildObjects/></SettingsStorage></MetaDataObject>")).unwrap();
    fs::write(
        native.join("SettingsStorages/CurrentЯ/Ext/ManagerModule.bsl"),
        body,
    )
    .unwrap();
    eska::project_core::source_format::import_designer_queries(
        native,
        &root.join("src-with-storages"),
    )
    .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap().replace(
        "source_format='eska'",
        "source_format='eska'\nsource='src-with-storages'",
    );
    fs::write(
        root.join("eska.toml"),
        if extension {
            configuration.replace("type='configuration'", "type='extension'")
        } else {
            configuration
        },
    )
    .unwrap();
}
