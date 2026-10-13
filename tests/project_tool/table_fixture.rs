//! Native table declarations shared by CLI removal and rename scenarios.

use super::attribute_fixture::fixture as field_fixture;
use super::*;

pub const SERVICES: [&str; 4] = [
    "a0000000-0000-4000-8000-000000000001",
    "b0000000-0000-4000-8000-000000000001",
    "c0000000-0000-4000-8000-000000000001",
    "d0000000-0000-4000-8000-000000000001",
];

/// Explicit native declaration slots and a complete property profile replace only the private fixture's inert service bytes.
pub fn fixture(root: &Path, dependency: Option<&str>) {
    field_fixture(root, None);
    let path = root.join("native/Catalogs/Records.xml");
    let mut native = fs::read_to_string(&path).unwrap();
    native = native.replace(
        "version=\"2.20\"",
        "xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" version=\"2.20\"",
    );
    let declarations=[("TabularSection",0),("TabularSectionRow",2)].map(|(role,index)| {
        format!("<xr:GeneratedType name=\"Catalog{role}.Records.Rows\" category=\"{role}\"><xr:TypeId>{}</xr:TypeId><xr:ValueId>{}</xr:ValueId></xr:GeneratedType>",SERVICES[index],SERVICES[index+1])
    }).concat();
    native=native.replace("<Future>Retained table service bytes</Future>",&declarations).replace("<Name>Rows</Name></Properties>","<Name>Rows</Name><Synonym/><Comment/><ToolTip/><FillChecking>DontCheck</FillChecking><Use>ForItem</Use><LineNumberLength>5</LineNumberLength></Properties>");
    fs::write(path, native).unwrap();
    if let Some(kind) = dependency {
        let directory = root.join("native/Catalogs/Records/Ext");
        fs::create_dir_all(&directory).unwrap();
        let (path, body) = if kind == "xml" {
            (
                "Extra.xml",
                "<Form><DataPath><![CDATA[Объект.Rows]]></DataPath></Form>".into(),
            )
        } else if kind == "service" {
            ("Extra.xml", format!("<Root ref=\"{}\"/>", SERVICES[3]))
        } else {
            (
                "ObjectModule.bsl",
                "\u{feff}Object.Rows.Clear();\r\n".into(),
            )
        };
        fs::write(directory.join(path), body).unwrap();
    }
}
