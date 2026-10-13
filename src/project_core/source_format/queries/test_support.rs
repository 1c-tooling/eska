//! Owned query fixtures serve project, protection and coordinate checks with one native input model.

use crate::{
    project_core::source_format::{SourcePath, files::Inventory, import_designer_queries},
    test_support::TestDir,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(in crate::project_core::source_format) const FORM: &str =
    "Catalogs/Sample/Forms/Main/form.yaml";
pub(in crate::project_core::source_format) const DIALOG: &str =
    "Catalogs/Sample/Forms/Main/Ext/Form.xml";
pub(in crate::project_core::source_format) const TEMPLATE: &str =
    "Catalogs/Sample/Templates/QuerySchema/Ext/Template.xml";
pub(in crate::project_core::source_format) const RESOURCE: &str =
    "Catalogs/Sample/Templates/QuerySchema/Template.xml";
pub(in crate::project_core::source_format) const QUERY: &str =
    "Catalogs/Sample/Templates/QuerySchema/Queries/Query_Same.sdbl";
pub(in crate::project_core::source_format) const LIST: &str =
    "Catalogs/Sample/Forms/Main/Queries/Attribute_5.sdbl";
pub(in crate::project_core::source_format) const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
pub(in crate::project_core::source_format) const DCS: &str =
    "http://v8.1c.ru/8.1/data-composition-system/schema";
pub(in crate::project_core::source_format) const LOGFORM: &str = "http://v8.1c.ru/8.3/xcf/logform";
pub(in crate::project_core::source_format) const XSI: &str =
    "http://www.w3.org/2001/XMLSchema-instance";

/// Checked paths retain their complete descriptor/resource spelling throughout the tests.
pub(in crate::project_core::source_format) fn path(value: &str) -> SourcePath {
    SourcePath::parse(value).unwrap()
}

/// Native form attribute IDs overlap UI IDs and include a saved empty and an automatic list.
pub(in crate::project_core::source_format) fn form() -> String {
    format!(
        "\u{feff}<Form xmlns=\"{LOGFORM}\" xmlns:xsi=\"{XSI}\" version=\"2.20\">\r\n<AutoCommandBar name=\"\" id=\"-1\"/><ChildItems><Table name=\"Grid\" id=\"5\"/></ChildItems><Attributes><Attribute name=\"Rows\" id=\"5\"><Settings xsi:type=\"DynamicList\"><ManualQuery>true</ManualQuery><QueryText>// list\r\nSELECT &quot;x&quot;</QueryText></Settings></Attribute><Attribute name=\"Empty\" id=\"6\"><Settings xsi:type=\"DynamicList\"><ManualQuery>true</ManualQuery><QueryText /></Settings></Attribute><Attribute name=\"Auto\" id=\"7\"><Settings xsi:type=\"DynamicList\"><ManualQuery>false</ManualQuery></Settings></Attribute></Attributes>\r\n</Form>\r\n"
    )
}

/// The same dataset name appears in two distinct scopes, without language interpretation.
pub(in crate::project_core::source_format) fn schema() -> String {
    format!(
        "\u{feff}<DataCompositionSchema xmlns=\"{DCS}\" xmlns:xsi=\"{XSI}\"><!--keep--><dataSet xsi:type=\"DataSetUnion\"><name>Union</name><item xsi:type=\"DataSetQuery\"><name>Same</name><query>// union\r\nSELECT 1</query></item></dataSet><dataSet xsi:type=\"DataSetQuery\"><name>Same</name><query>SELECT 2;&#xD;\n// end</query></dataSet><settingsVariant/></DataCompositionSchema>\r\n"
    )
}

/// Tests modify only a unique playground tree, including a declared opaque binary XML-named payload.
pub(in crate::project_core::source_format) fn fixture(directory: &TestDir) -> PathBuf {
    let native = directory.0.join("native");
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    fs::create_dir(&native).unwrap();
    let inventory = Inventory::read(&original).unwrap();
    for path in inventory.directories {
        fs::create_dir_all(native.join(path.as_path())).unwrap();
    }
    for path in inventory.files.keys() {
        fs::copy(original.join(path.as_path()), native.join(path.as_path())).unwrap();
    }
    fs::remove_file(native.join("Ext/ParentConfigurations.bin")).unwrap();
    fs::write(native.join(DIALOG), form()).unwrap();
    let catalog = native.join("Catalogs/Sample.xml");
    fs::write(
        &catalog,
        fs::read_to_string(&catalog).unwrap().replace(
            "<Form>Main</Form>",
            "<Form>Main</Form><Template>QuerySchema</Template><Template>Opaque</Template>",
        ),
    )
    .unwrap();
    for (name, kind, uuid) in [
        (
            "QuerySchema",
            "DataCompositionSchema",
            "80000000-0000-4000-8000-000000000001",
        ),
        (
            "Opaque",
            "BinaryData",
            "80000000-0000-4000-8000-000000000002",
        ),
    ] {
        let base = native.join(format!("Catalogs/Sample/Templates/{name}"));
        fs::create_dir_all(base.join("Ext")).unwrap();
        fs::write(base.with_extension("xml"), format!("\u{feff}<MetaDataObject xmlns=\"{MD}\" version=\"2.20\"><Template uuid=\"{uuid}\"><Properties><Name>{name}</Name><TemplateType>{kind}</TemplateType></Properties></Template></MetaDataObject>\r\n")).unwrap();
        let bytes = if name == "Opaque" {
            vec![0, 255, 10, 13]
        } else {
            schema().into_bytes()
        };
        fs::write(base.join("Ext/Template.xml"), bytes).unwrap();
    }
    native
}

/// A complete opt-in import retains its original input for byte-level controls.
pub(in crate::project_core::source_format) fn source() -> (TestDir, PathBuf, PathBuf) {
    let directory = TestDir::new();
    let native = fixture(&directory);
    let root = directory.0.join("source");
    import_designer_queries(&native, &root).unwrap();
    (directory, native, root)
}
