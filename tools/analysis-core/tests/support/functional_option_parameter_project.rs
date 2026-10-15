//! Native parameter fixtures share the ordinary project importer and existing current storage owners.

use std::{fs, path::Path};

/// Add parameter declarations to a test-owned native source and import an independent current layout.
pub fn add_parameters(root: &Path, native: &Path) {
    let config = native.join("Configuration.xml");
    let text = fs::read_to_string(&config).unwrap().replace("</ChildObjects>", "<FunctionalOptionsParameter>OrganizationЯ</FunctionalOptionsParameter><FunctionalOptionsParameter>SettingЯ</FunctionalOptionsParameter><FunctionalOptionsParameter>UnusedЯ</FunctionalOptionsParameter></ChildObjects>");
    fs::write(config, text).unwrap();
    let module = native.join("CommonModules/Tools.xml");
    let text = fs::read_to_string(&module).unwrap().replace(
        "<Server>true</Server>",
        "<Server>false</Server><ClientManagedApplication>true</ClientManagedApplication>",
    );
    fs::write(module, text).unwrap();
    let register = native.join("InformationRegisters/Settings.xml");
    let text = fs::read_to_string(&register).unwrap().replace("</ChildObjects>", "<Dimension uuid='b0000000-0000-4000-8000-000000000001'><Properties><Name>Key</Name><Type><t:Type>x:string</t:Type></Type></Properties></Dimension></ChildObjects>");
    fs::write(register, text).unwrap();
    fs::create_dir_all(native.join("FunctionalOptionsParameters")).unwrap();
    for (name, uuid, target) in [
        (
            "OrganizationЯ",
            "a0000000-0000-4000-8000-000000000001",
            "Catalog.Items",
        ),
        (
            "SettingЯ",
            "a0000000-0000-4000-8000-000000000002",
            "InformationRegister.Settings.Dimension.Key",
        ),
        ("UnusedЯ", "a0000000-0000-4000-8000-000000000003", ""),
    ] {
        let use_ = if target.is_empty() {
            "<Use/>".into()
        } else {
            format!("<Use><r:Item i:type='r:MDObjectRef'>{target}</r:Item></Use>")
        };
        let xml = format!(
            "<MetaDataObject version='2.20' xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:r='http://v8.1c.ru/8.3/xcf/readable' xmlns:i='http://www.w3.org/2001/XMLSchema-instance'><FunctionalOptionsParameter uuid='{uuid}'><Properties><Name>{name}</Name>{use_}</Properties></FunctionalOptionsParameter></MetaDataObject>"
        );
        fs::write(
            native.join(format!("FunctionalOptionsParameters/{name}.xml")),
            xml,
        )
        .unwrap();
    }
    eska::project_core::source_format::import_designer_queries(
        native,
        &root.join("src-with-option-parameters"),
    )
    .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap().replace(
        "source='src-with-options'",
        "source='src-with-option-parameters'",
    );
    fs::write(root.join("eska.toml"), config).unwrap();
}
