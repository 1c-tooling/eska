//! Two independent integration services bind declared channels to a real module.

use std::{fs, path::Path};

/// Extend this test's native fixture, then import its own source without touching the original import.
pub fn add_integration_services(root: &Path, native: &Path) {
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(native.join("Configuration.xml"), configuration.replace("</ChildObjects>", "<IntegrationService>Bus</IntegrationService><IntegrationService>Neighbor</IntegrationService></ChildObjects>")).unwrap();
    fs::create_dir(native.join("IntegrationServices")).unwrap();
    for (name, number, handler) in [("Bus", 1, "SavedReceive"), ("Neighbor", 2, "OtherReceive")] {
        fs::write(native.join(format!("IntegrationServices/{name}.xml")), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><IntegrationService uuid="70000000-0000-4000-8000-{number:012}"><Properties><Name>{name}</Name></Properties><ChildObjects><IntegrationServiceChannel uuid="80000000-0000-4000-8000-{number:012}"><Properties><Name>Incoming</Name><ReceiveMessageProcessing>{handler}</ReceiveMessageProcessing></Properties></IntegrationServiceChannel><IntegrationServiceChannel uuid="90000000-0000-4000-8000-{number:012}"><Properties><Name>Outgoing</Name><ReceiveMessageProcessing/></Properties></IntegrationServiceChannel></ChildObjects></IntegrationService></MetaDataObject>"#)).unwrap();
    }
    let module = native.join("IntegrationServices/Bus/Ext");
    fs::create_dir_all(&module).unwrap();
    fs::write(module.join("Module.bsl"), "Procedure SavedReceive()\n Value = Metadata.IntegrationServices.Bus;\n Value = Value;\nEndProcedure\nProcedure CurrentReceive()\nEndProcedure\n").unwrap();
    eska::project_core::source_format::import_designer(native, &root.join("src-with-integration"))
        .unwrap();
    let configuration = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        configuration.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-integration'",
        ),
    )
    .unwrap();
}
