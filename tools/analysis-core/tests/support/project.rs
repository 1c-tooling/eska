//! Shared fixture bytes contain no directory ownership or environment policy.

use std::{fs, path::PathBuf};

/// Create fully supported metadata with one UTF-8 module; no accepted base or platform is needed.
pub fn project(directory: &std::path::Path, body: &str, binary: bool) -> PathBuf {
    let native = directory.join("native");
    fs::create_dir_all(native.join("CommonModules/Tools/Ext")).unwrap();
    fs::write(native.join("Configuration.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>SyntaxFixture</Name></Properties><ChildObjects><CommonModule>Tools</CommonModule></ChildObjects></Configuration></MetaDataObject>").unwrap();
    fs::write(native.join("CommonModules/Tools.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><CommonModule uuid=\"20000000-0000-4000-8000-000000000001\"><Properties><Name>Tools</Name><Server>true</Server></Properties></CommonModule></MetaDataObject>").unwrap();
    let extension = if binary { "bin" } else { "bsl" };
    fs::write(
        native.join(format!("CommonModules/Tools/Ext/Module.{extension}")),
        body,
    )
    .unwrap();
    let root = directory.join("project");
    fs::create_dir(&root).unwrap();
    eska::project_core::source_format::import_designer(&native, &root.join("src")).unwrap();
    fs::write(root.join("eska.toml"), "[project]\nname='syntax-fixture'\ntype='configuration'\nsource_format='eska'\n[eska]\naccepted_base='.eska/accepted'\n").unwrap();
    root
}

/// Inline object commands own a separate body but have no separate Designer metadata descriptor.
pub fn inline_command_project(directory: &std::path::Path, body: &str) -> PathBuf {
    let root = project(directory, "// A complete common module.\n", false);
    let native = directory.join("native");
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace(
            "<CommonModule>Tools</CommonModule>",
            "<CommonModule>Tools</CommonModule><Catalog>Sample</Catalog>",
        ),
    )
    .unwrap();
    fs::create_dir_all(native.join("Catalogs/Sample/Commands/Run/Ext")).unwrap();
    fs::write(native.join("Catalogs/Sample.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Catalog uuid=\"30000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name></Properties><ChildObjects><Command uuid=\"40000000-0000-4000-8000-000000000001\"><Properties><Name>Run</Name></Properties></Command></ChildObjects></Catalog></MetaDataObject>").unwrap();
    fs::write(
        native.join("Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl"),
        body,
    )
    .unwrap();
    eska::project_core::source_format::import_designer(&native, &root.join("src-with-command"))
        .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        config.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-command'",
        ),
    )
    .unwrap();
    root
}
