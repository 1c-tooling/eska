//! Each external-context test owns a native EPF/ERF fixture and one imported Queries project.

use std::{
    fs,
    path::{Path, PathBuf},
};

use eska::project_core::{
    ProjectType,
    source_format::{SourcePath, SourceReader, import_designer_queries},
};

pub const NAME: &str = "ВнешнийExternalOwner";
pub const OBJECT_BODY: &str = "\u{feff}// Я😀 own object context\r\nПроцедура Проба() Экспорт\r\n    ЭтотОбъект.\r\nКонецПроцедуры\r\n";
pub const FORM_BODY: &str = "\u{feff}// Я😀 own form context\r\n&НаКлиенте\r\nПроцедура Проба()\r\n    Объект.\r\nКонецПроцедуры\r\n";

pub struct Paths {
    pub owner: SourcePath,
    pub object_module: SourcePath,
    pub form_module: SourcePath,
    pub form_owner: SourcePath,
    pub dialog: SourcePath,
}

/// Use real external descriptor tags and main-attribute qualified names; no Configuration XML is created.
pub fn project(directory: &Path, kind: ProjectType) -> PathBuf {
    let tag = match kind {
        ProjectType::Processing => "ExternalDataProcessor",
        ProjectType::Report => "ExternalReport",
        _ => panic!("external fixture kind"),
    };
    let native = directory.join("native");
    fs::create_dir(&native).unwrap();
    fs::write(native.join(format!("{NAME}.xml")), format!("\u{feff}<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"><{tag} uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>{NAME}</Name></Properties><ChildObjects><Attribute uuid=\"20000000-0000-4000-8000-000000000001\"><Properties><Name>Сохраненный</Name><Type><v8:Type>xs:boolean</v8:Type></Type></Properties></Attribute><TabularSection uuid=\"30000000-0000-4000-8000-000000000001\"><Properties><Name>Строки</Name></Properties><ChildObjects><Attribute uuid=\"40000000-0000-4000-8000-000000000001\"><Properties><Name>Значение</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute></ChildObjects></TabularSection><Form>Dialog</Form></ChildObjects></{tag}></MetaDataObject>\r\n")).unwrap();
    let object = native.join(format!("{NAME}/Ext/ObjectModule.bsl"));
    fs::create_dir_all(object.parent().unwrap()).unwrap();
    fs::write(object, OBJECT_BODY).unwrap();
    fs::create_dir_all(native.join(format!("{NAME}/Forms"))).unwrap();
    fs::write(native.join(format!("{NAME}/Forms/Dialog.xml")), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Form uuid=\"50000000-0000-4000-8000-000000000001\"><Properties><Name>Dialog</Name><FormType>Managed</FormType></Properties></Form></MetaDataObject>").unwrap();
    let dialog = native.join(format!("{NAME}/Forms/Dialog/Ext/Form.xml"));
    fs::create_dir_all(dialog.parent().unwrap().join("Form")).unwrap();
    fs::write(&dialog, format!("\u{feff}<Form xmlns=\"http://v8.1c.ru/8.3/xcf/logform\" xmlns:cfg=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" version=\"2.20\"><AutoCommandBar name=\"\" id=\"-1\"/><Attributes><Attribute name=\"Объект\" id=\"1\"><Type><Type>cfg:{tag}Object.{NAME}</Type></Type><MainAttribute>true</MainAttribute></Attribute></Attributes></Form>\r\n")).unwrap();
    fs::write(dialog.parent().unwrap().join("Form/Module.bsl"), FORM_BODY).unwrap();
    let root = directory.join("project");
    fs::create_dir(&root).unwrap();
    import_designer_queries(&native, &root.join("src")).unwrap();
    fs::write(root.join("eska.toml"), format!("[project]\nname='external-fixture'\ntype='{}'\nsource='src'\nsource_format='eska'\n[eska]\naccepted_base='.eska/accepted'\n", kind.as_str())).unwrap();
    root
}

/// Bind paths through the reader's real declared identities, independent of physical authoring layout.
pub fn paths(root: &Path) -> Paths {
    let reader = SourceReader::open(&root.join("src")).unwrap();
    let module = |native: &str| {
        reader
            .module_paths()
            .find(|path| reader.declared_module(path).unwrap().native_path.as_str() == native)
            .unwrap()
            .clone()
    };
    let form = SourcePath::parse(&format!("{NAME}/Forms/Dialog.xml")).unwrap();
    let form = reader.declared_form(&form).unwrap();
    Paths {
        owner: reader.configuration().path.clone(),
        object_module: module(&format!("{NAME}/Ext/ObjectModule.bsl")),
        form_module: module(&format!("{NAME}/Forms/Dialog/Ext/Form/Module.bsl")),
        form_owner: form.owner.clone(),
        dialog: form.dialog.unwrap().clone(),
    }
}
