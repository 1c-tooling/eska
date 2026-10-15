//! Native command descriptors and bodies are imported through the ordinary shared source reader.

use std::{fs, path::Path};

/// Extend this test's own native project with commands and one cold reference target.
pub fn add_commands(
    root: &Path,
    native: &Path,
    body: &str,
    extension: bool,
    uuid: &str,
    extra: &str,
) {
    let path = native.join("Configuration.xml");
    let mut configuration = fs::read_to_string(&path).unwrap().replace(
        "</ChildObjects>",
        "<Catalog>Items</Catalog><CommonCommand>Run</CommonCommand><CommonCommand>Other</CommonCommand></ChildObjects>",
    );
    if extension {
        configuration = configuration.replace("<Name>SyntaxFixture</Name>", "<Name>Extension</Name><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>").replace("10000000-0000-4000-8000-000000000001", &uuid.replacen("70000000","10000000",1));
        let module = native.join("CommonModules/Tools.xml");
        let text = fs::read_to_string(&module).unwrap().replace(
            "20000000-0000-4000-8000-000000000001",
            &uuid.replacen("70000000", "20000000", 1),
        );
        fs::write(module, text).unwrap();
    }
    fs::write(path, configuration).unwrap();
    fs::create_dir_all(native.join("Catalogs")).unwrap();
    let catalog_uuid = uuid.replacen("70000000", "90000000", 1);
    let attribute_uuid = uuid.replacen("70000000", "a0000000", 1);
    fs::write(native.join("Catalogs/Items.xml"),format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:t="http://v8.1c.ru/8.1/data/core" xmlns:x="http://www.w3.org/2001/XMLSchema" version="2.20"><Catalog uuid="{catalog_uuid}"><Properties><Name>Items</Name></Properties><ChildObjects><Attribute uuid="{attribute_uuid}"><Properties><Name>Choice</Name><Type><t:Type>x:boolean</t:Type></Type></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>"#)).unwrap();
    fs::create_dir_all(native.join("CommonCommands")).unwrap();
    for name in ["Run", "Other"] {
        let other_uuid = uuid.replacen("70000000", "80000000", 1);
        let command_uuid = if name == "Run" { uuid } else { &other_uuid };
        let properties = if name == "Run" { extra } else { "" };
        let parameters = if properties.contains("<ObjectBelonging>Adopted</ObjectBelonging>") {
            ""
        } else {
            "<CommandParameterType><t:Type>x:string</t:Type></CommandParameterType><ParameterUseMode>Single</ParameterUseMode>"
        };
        fs::write(native.join(format!("CommonCommands/{name}.xml")), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:t="http://v8.1c.ru/8.1/data/core" xmlns:x="http://www.w3.org/2001/XMLSchema" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" version="2.20"><CommonCommand uuid="{command_uuid}"><Properties><Name>{name}</Name>{parameters}<ModifiesData>false</ModifiesData>{properties}</Properties></CommonCommand></MetaDataObject>"#)).unwrap();
        let folder = native.join(format!("CommonCommands/{name}/Ext"));
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join("CommandModule.bsl"),
            if name == "Run" {
                body
            } else {
                "Procedure CommandProcessing(Value, Context)\nEndProcedure\n"
            },
        )
        .unwrap();
    }
    eska::project_core::source_format::import_designer_queries(
        native,
        &root.join("src-with-common-commands"),
    )
    .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap().replace(
        "source_format='eska'",
        "source_format='eska'\nsource='src-with-common-commands'",
    );
    fs::write(
        root.join("eska.toml"),
        if extension {
            config.replace("type='configuration'", "type='extension'")
        } else {
            config
        },
    )
    .unwrap();
}
