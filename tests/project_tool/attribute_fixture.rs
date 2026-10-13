//! Actual accepted fields and table columns shared by CLI structural-profile scenarios.

use super::*;

pub const OWNER: &str = "20000000-0000-4000-8000-000000000001";
pub const FIELD: &str = "30000000-0000-4000-8000-000000000001";
pub const SHIFTED: &str = "40000000-0000-4000-8000-000000000001";
pub const TABLE: &str = "50000000-0000-4000-8000-000000000001";
pub const COLUMN: &str = "60000000-0000-4000-8000-000000000001";
pub const STATIONARY: &str = "70000000-0000-4000-8000-000000000001";

/// The native fixture separates root fields from a real direct table with inert accepted service bytes.
pub fn fixture(root: &Path, dependency: Option<&str>) {
    fs::create_dir_all(root.join("native/Catalogs")).unwrap();
    fs::write(root.join("native/Configuration.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name></Properties><ChildObjects><Catalog>Records</Catalog></ChildObjects></Configuration></MetaDataObject>").unwrap();
    let field = |uuid: &str, name: &str| {
        format!(
            "<Attribute uuid=\"{uuid}\"><Properties><Name>{name}</Name><Synonym/><Comment/><Type><v8:Type>xs:boolean</v8:Type></Type><PasswordMode>false</PasswordMode><Format/></Properties></Attribute>"
        )
    };
    fs::write(root.join("native/Catalogs/Records.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"><Catalog uuid=\"{OWNER}\"><Properties><Name>Records</Name></Properties><ChildObjects>{}{}<TabularSection uuid=\"{TABLE}\"><InternalInfo><Future>Retained table service bytes</Future></InternalInfo><Properties><Name>Rows</Name></Properties><ChildObjects>{}{}</ChildObjects></TabularSection></ChildObjects></Catalog></MetaDataObject>", field(FIELD,"УдаляемоеПоле"), field(SHIFTED,"RemainingField"), field(STATIONARY,"RemainingColumn"),field(COLUMN,"УдаляемаяКолонка"))).unwrap();
    if let Some(kind) = dependency {
        fs::create_dir_all(root.join("native/Catalogs/Records/Ext")).unwrap();
        let (path, text) = if kind == "xml" {
            (
                "Extra.xml",
                "<Form><DataPath><![CDATA[Объект.УдаляемоеПоле]]></DataPath></Form>",
            )
        } else if kind == "xml-split" {
            (
                "Extra.xml",
                "<Form><DataPath>Объект.Удаля<!-- keep -->емое<?note keep?>Поле</DataPath></Form>",
            )
        } else {
            (
                "ObjectModule.bsl",
                "\u{feff}Object.УдаляемоеПоле = Истина;\r\n",
            )
        };
        fs::write(root.join("native/Catalogs/Records/Ext").join(path), text).unwrap();
    }
}
