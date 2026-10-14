//! Test-owned native sources exercise the real all-four-type import and source identity contract.

use std::{fs, path::PathBuf};

use eska::project_core::{
    ProjectName, ProjectType,
    creation::import_query_project,
    source_format::{SourcePath, SourceReader},
};

use super::TestDirectory;

pub const TYPES: [ProjectType; 4] = [
    ProjectType::Configuration,
    ProjectType::Extension,
    ProjectType::Processing,
    ProjectType::Report,
];
pub const GOOD: &str =
    "\u{feff}// Я😀 current source\r\nПроцедура Проверка() Экспорт\r\nКонецПроцедуры\r\n";
pub const BAD: &str = "\u{feff}// Я😀 current source\r\nПроцедура Черновик(\r\nКонецПроцедуры\r\n";

/// Each fixture owns its native input and complete Queries project, with one declared module.
pub fn project(
    kind: ProjectType,
    body: &str,
    binary: bool,
) -> (TestDirectory, PathBuf, SourcePath) {
    let directory = TestDirectory::new();
    let native = directory.0.join("native");
    fs::create_dir(&native).unwrap();
    let external = matches!(kind, ProjectType::Processing | ProjectType::Report);
    let module = if external {
        "ScopeFixture/Ext/ObjectModule"
    } else {
        "CommonModules/Tools/Ext/Module"
    };
    let extension = if binary { "bin" } else { "bsl" };
    let module = native.join(format!("{module}.{extension}"));
    fs::create_dir_all(module.parent().unwrap()).unwrap();
    fs::write(module, body).unwrap();
    let (tag, file) = match kind {
        ProjectType::Configuration | ProjectType::Extension => {
            ("Configuration", "Configuration.xml")
        }
        ProjectType::Processing => ("ExternalDataProcessor", "ScopeFixture.xml"),
        ProjectType::Report => ("ExternalReport", "ScopeFixture.xml"),
    };
    let properties = if kind == ProjectType::Extension {
        "<ObjectBelonging>Adopted</ObjectBelonging><Name>ScopeFixture</Name><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>"
    } else {
        "<Name>ScopeFixture</Name>"
    };
    let children = if external {
        ""
    } else {
        "<CommonModule>Tools</CommonModule>"
    };
    fs::write(native.join(file), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><{tag} uuid=\"10000000-0000-4000-8000-000000000001\"><Properties>{properties}</Properties><ChildObjects>{children}</ChildObjects></{tag}></MetaDataObject>")).unwrap();
    if !external {
        fs::write(native.join("CommonModules/Tools.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><CommonModule uuid=\"20000000-0000-4000-8000-000000000001\"><Properties><Name>Tools</Name><Server>true</Server></Properties><ChildObjects/></CommonModule></MetaDataObject>").unwrap();
    }
    let root = directory.0.join("project");
    import_query_project(
        &native,
        &root,
        ProjectName::parse("local-syntax".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let reader = SourceReader::open(&root.join("src")).unwrap();
    assert_eq!(reader.project_type().unwrap(), kind);
    let module = reader.module_paths().next().unwrap().clone();
    assert_eq!(reader.module_paths().count(), 1);
    (directory, root, module)
}
