//! Small native profiles share exact form/query/payload input across core and process acceptance.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub const TYPES: [&str; 4] = ["configuration", "extension", "processing", "report"];
pub const ROOT_UUID: &str = "10000000-0000-4000-8000-000000000001";
pub const FORM_UUID: &str = "70000000-0000-4000-8000-000000000001";
const TEMPLATE_UUID: &str = "80000000-0000-4000-8000-000000000001";
const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

pub struct Native {
    pub input: PathBuf,
    pub root: PathBuf,
    pub root_source: &'static str,
    pub form: PathBuf,
    pub form_source: PathBuf,
    pub dialog: PathBuf,
    pub schema: PathBuf,
    pub module: PathBuf,
    pub module_source: &'static str,
}

pub type Snapshot = (BTreeMap<PathBuf, Vec<u8>>, BTreeSet<PathBuf>);

/// Compare the small fixture's exact files and directories without borrowing private core inventory.
pub fn snapshot(root: &Path) -> Snapshot {
    let mut files = BTreeMap::new();
    let mut directories = BTreeSet::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        for entry in fs::read_dir(root.join(&relative)).unwrap() {
            let entry = entry.unwrap();
            let path = relative.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                directories.insert(path.clone());
                pending.push(path);
            } else {
                files.insert(path, fs::read(entry.path()).unwrap());
            }
        }
    }
    (files, directories)
}

/// Write only inside the caller's unique test directory, with native aliases distinct from names.
pub fn native(parent: &Path, kind: &str) -> Native {
    let input = parent.join("native");
    fs::create_dir(&input).unwrap();
    let (tag, root, root_source, form_tag, template_tag, prefix, module, module_source) = match kind
    {
        "configuration" | "extension" => (
            "Configuration",
            "Configuration.xml",
            "configuration.yaml",
            "CommonForm",
            "CommonTemplate",
            "",
            "Ext/ManagedApplicationModule.bsl",
            "ManagedApplicationModule.bsl",
        ),
        "processing" => (
            "ExternalDataProcessor",
            "Передача.v1.xml",
            "processing.yaml",
            "Form",
            "Template",
            "Передача.v1/",
            "Передача.v1/Ext/ObjectModule.bsl",
            "ObjectModule.bsl",
        ),
        "report" => (
            "ExternalReport",
            "Передача.v1.xml",
            "report.yaml",
            "Form",
            "Template",
            "Передача.v1/",
            "Передача.v1/Ext/ObjectModule.bsl",
            "ObjectModule.bsl",
        ),
        _ => panic!("unknown test project type"),
    };
    let purpose = if kind == "extension" {
        "<ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>"
    } else {
        ""
    };
    write(&input.join(root), format!(
        "\u{feff}<MetaDataObject xmlns=\"{MD}\" version=\"2.20\"><{tag} uuid=\"{ROOT_UUID}\"><Properties><Name>NativeName</Name><Comment>Accepted</Comment><Version>1.2.3</Version>{purpose}</Properties><ChildObjects><{form_tag}>Main</{form_tag}><{template_tag}>Schema</{template_tag}></ChildObjects></{tag}></MetaDataObject>\r\n"
    ).as_bytes());
    let forms = if prefix.is_empty() {
        "CommonForms"
    } else {
        "Forms"
    };
    let templates = if prefix.is_empty() {
        "CommonTemplates"
    } else {
        "Templates"
    };
    let form = PathBuf::from(format!("{prefix}{forms}/Main.xml"));
    write(&input.join(&form), format!(
        "\u{feff}<MetaDataObject xmlns=\"{MD}\" version=\"2.20\"><{form_tag} uuid=\"{FORM_UUID}\"><Properties><Name>Main</Name><Comment>Accepted form</Comment><FormType>Managed</FormType></Properties></{form_tag}></MetaDataObject>\r\n"
    ).as_bytes());
    let dialog = PathBuf::from(format!("{prefix}{forms}/Main/Ext/Form.xml"));
    write(&input.join(&dialog), "\u{feff}<Form xmlns=\"http://v8.1c.ru/8.3/xcf/logform\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" version=\"2.20\">\r\n<AutoCommandBar name=\"\" id=\"-1\"/><ChildItems><Table name=\"Grid\" id=\"5\"/></ChildItems><Attributes><Attribute name=\"Rows\" id=\"5\"><Settings xsi:type=\"DynamicList\"><ManualQuery>true</ManualQuery><QueryText>SELECT &quot;x&quot;</QueryText></Settings></Attribute></Attributes><ReadOnly>false</ReadOnly>\r\n</Form>\r\n".as_bytes());
    let template = input.join(format!("{prefix}{templates}/Schema.xml"));
    write(&template, format!(
        "\u{feff}<MetaDataObject xmlns=\"{MD}\" version=\"2.20\"><{template_tag} uuid=\"{TEMPLATE_UUID}\"><Properties><Name>Schema</Name><TemplateType>DataCompositionSchema</TemplateType></Properties></{template_tag}></MetaDataObject>\r\n"
    ).as_bytes());
    let schema = PathBuf::from(format!("{prefix}{templates}/Schema/Ext/Template.xml"));
    write(&input.join(&schema), "\u{feff}<DataCompositionSchema xmlns=\"http://v8.1c.ru/8.1/data-composition-system/schema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><!-- keep --><dataSet xsi:type=\"DataSetQuery\"><name>Data</name><query>SELECT 2;&#xD;\n// native query</query></dataSet><settingsVariant/></DataCompositionSchema>\r\n".as_bytes());
    write(&input.join(module), "\u{feff}// native module\r\nФункция Версия() Экспорт\r\n\tВозврат \"1.2.3\";\r\nКонецФункции\r\n".as_bytes());
    write(&input.join("opaque.bin"), &[0, 255, 10, 13]);
    fs::create_dir(input.join("Empty")).unwrap();
    Native {
        input,
        root: root.into(),
        root_source,
        form,
        form_source: format!("{forms}/Main/form.yaml").into(),
        dialog,
        schema,
        module: module.into(),
        module_source,
    }
}

/// Native resources own their directory creation; no project or settings are fabricated by fixtures.
fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
