//! Two real Web descriptors publish independent operations and a declared BSL body.

use std::{fs, path::Path};

/// Extend the owned native fixture, then import to a separate source directory.
pub fn add_web_services(root: &Path, native: &Path) {
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace(
            "</ChildObjects>",
            "<WebService>API</WebService><WebService>Neighbor</WebService></ChildObjects>",
        ),
    )
    .unwrap();
    fs::create_dir(native.join("WebServices")).unwrap();
    for (name, number, handler) in [("API", 1, "SavedCall"), ("Neighbor", 2, "OtherCall")] {
        fs::write(native.join(format!("WebServices/{name}.xml")),format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><WebService uuid="40000000-0000-4000-8000-{number:012}"><Properties><Name>{name}</Name><Namespace>urn:saved</Namespace></Properties><ChildObjects><Operation uuid="50000000-0000-4000-8000-{number:012}"><Properties><Name>Call</Name><ProcedureName>{handler}</ProcedureName></Properties><ChildObjects><Parameter uuid="60000000-0000-4000-8000-{number:012}"><Properties><Name>Request</Name></Properties></Parameter></ChildObjects></Operation></ChildObjects></WebService></MetaDataObject>"#)).unwrap();
    }
    let module = native.join("WebServices/API/Ext");
    fs::create_dir_all(&module).unwrap();
    fs::write(
        module.join("Module.bsl"),
        "Function SavedCall(Request)\n Value = Metadata.WebServices.API;\n Value = Value;\n Return Undefined;\nEndFunction\n",
    )
    .unwrap();
    eska::project_core::source_format::import_designer(native, &root.join("src-with-web")).unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-web'",
        ),
    )
    .unwrap();
}
