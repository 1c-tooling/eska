//! A test-owned composition binds an exported dependency without changing existing public fixtures.

use std::{fs, path::Path};

/// Add a second module to the tiny fixture and select a separate fresh imported source tree.
pub fn add_dependency(root: &Path, native: &Path, dependency: &str) {
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace(
            "<CommonModule>Tools</CommonModule>",
            "<CommonModule>Tools</CommonModule><CommonModule>ZDependency</CommonModule>",
        ),
    )
    .unwrap();
    fs::create_dir_all(native.join("CommonModules/ZDependency/Ext")).unwrap();
    fs::write(native.join("CommonModules/ZDependency.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><CommonModule uuid=\"20000000-0000-4000-8000-000000000002\"><Properties><Name>ZDependency</Name><Server>true</Server></Properties></CommonModule></MetaDataObject>").unwrap();
    fs::write(
        native.join("CommonModules/ZDependency/Ext/Module.bsl"),
        dependency,
    )
    .unwrap();
    eska::project_core::source_format::import_designer(native, &root.join("src-dependent"))
        .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        config.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-dependent'",
        ),
    )
    .unwrap();
}
