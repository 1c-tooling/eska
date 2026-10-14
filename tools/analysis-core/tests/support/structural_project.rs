//! All structural descriptor and process tests share one independently owned Queries source.

use eska::project_core::source_format::import_designer_queries;
use std::{
    fs,
    path::{Path, PathBuf},
};

const DCS: &str = "http://v8.1c.ru/8.1/data-composition-system/schema";
const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

/// One owned native fixture has a language, DCS template, common template and BSL body.
pub fn project(directory: &Path) -> PathBuf {
    let native = directory.join("native");
    fs::create_dir(&native).unwrap();
    for (path, kind, number, fields, children) in [
        (
            "Configuration.xml",
            "Configuration",
            1,
            "<Name>Demo</Name><Version>0.1.0</Version>",
            "<Language>Locale</Language><Report>Demo</Report><CommonTemplate>Global</CommonTemplate><CommonModule>Shared</CommonModule>",
        ),
        (
            "Languages/Locale.xml",
            "Language",
            2,
            "<Name>Locale</Name><LanguageCode>ru</LanguageCode>",
            "",
        ),
        (
            "Reports/Demo.xml",
            "Report",
            3,
            "<Name>Demo</Name>",
            "<Template>Schema</Template>",
        ),
        (
            "Reports/Demo/Templates/Schema.xml",
            "Template",
            4,
            "<Name>Schema</Name><TemplateType>DataCompositionSchema</TemplateType>",
            "",
        ),
        (
            "CommonTemplates/Global.xml",
            "CommonTemplate",
            5,
            "<Name>Global</Name><TemplateType>BinaryData</TemplateType>",
            "",
        ),
        (
            "CommonModules/Shared.xml",
            "CommonModule",
            6,
            "<Name>Shared</Name><Server>true</Server>",
            "",
        ),
    ] {
        let path = native.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("\u{feff}<MetaDataObject xmlns=\"{MD}\" version=\"2.20\"><{kind} uuid=\"10000000-0000-4000-8000-{number:012}\"><Properties>{fields}</Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>\r\n")).unwrap();
    }
    let resource = native.join("Reports/Demo/Templates/Schema/Ext/Template.xml");
    fs::create_dir_all(resource.parent().unwrap()).unwrap();
    fs::write(resource, format!("\u{feff}<DataCompositionSchema xmlns=\"{DCS}\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><dataSet xsi:type=\"DataSetQuery\"><name>Main</name><query>SELECT 1 AS Value</query></dataSet><settingsVariant/></DataCompositionSchema>\r\n")).unwrap();
    let opaque = native.join("CommonTemplates/Global/Ext/Template.bin");
    fs::create_dir_all(opaque.parent().unwrap()).unwrap();
    fs::write(opaque, [0, 255, 4]).unwrap();
    let module = native.join("CommonModules/Shared/Ext/Module.bsl");
    fs::create_dir_all(module.parent().unwrap()).unwrap();
    fs::write(
        module,
        "\u{feff}Процедура Проверка() Экспорт\r\nКонецПроцедуры\r\n",
    )
    .unwrap();
    let root = directory.join("project");
    fs::create_dir(&root).unwrap();
    let source = root.join("src");
    import_designer_queries(&native, &source).unwrap();
    fs::write(root.join("eska.toml"), "[project]\ntype = \"configuration\"\nsource = \"src\"\nsource_format = \"eska\"\n[eska]\naccepted_base = \".eska/accepted\"\n").unwrap();
    root
}

/// Process proofs select only native BSL parsing so unrelated rule defaults cannot affect coverage.
pub fn settings(root: &Path) {
    use std::fmt::Write as _;
    let content = ide::all_diagnostic_codes().fold(
        "[diagnostics.parameters]\n".into(),
        |mut content: String, code| {
            writeln!(
                content,
                "{}={}",
                code.as_str(),
                code.as_str() == "ParseError"
            )
            .unwrap();
            content
        },
    );
    fs::write(root.join("bsl-analyzer.toml"), content).unwrap();
}
