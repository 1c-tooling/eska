//! Two real HTTP descriptors publish independent templates and a declared BSL body.

use std::{fs, path::Path};

/// Extend the owned native fixture, then import to a separate source directory.
pub fn add_http_services(root: &Path, native: &Path) {
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace(
            "</ChildObjects>",
            "<HTTPService>API</HTTPService><HTTPService>Neighbor</HTTPService></ChildObjects>",
        ),
    )
    .unwrap();
    fs::create_dir(native.join("HTTPServices")).unwrap();
    for (name, number, handler) in [("API", 1, "PingGet"), ("Neighbor", 2, "OtherGet")] {
        fs::write(native.join(format!("HTTPServices/{name}.xml")),format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><HTTPService uuid="40000000-0000-4000-8000-{number:012}"><Properties><Name>{name}</Name><RootURL>saved</RootURL></Properties><ChildObjects><URLTemplate uuid="50000000-0000-4000-8000-{number:012}"><Properties><Name>Ping</Name><Template>/saved</Template></Properties><ChildObjects><Method uuid="60000000-0000-4000-8000-{number:012}"><Properties><Name>Get</Name><HTTPMethod>GET</HTTPMethod><Handler>{handler}</Handler></Properties></Method></ChildObjects></URLTemplate></ChildObjects></HTTPService></MetaDataObject>"#)).unwrap();
    }
    let module = native.join("HTTPServices/API/Ext");
    fs::create_dir_all(&module).unwrap();
    fs::write(
        module.join("Module.bsl"),
        "Function PingGet(Request)\n Value = Metadata.HTTPServices.API;\n Value = Value;\n Return Undefined;\nEndFunction\n",
    )
    .unwrap();
    eska::project_core::source_format::import_designer(native, &root.join("src-with-http"))
        .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-http'",
        ),
    )
    .unwrap();
}
