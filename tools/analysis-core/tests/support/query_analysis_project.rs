//! Query diagnostics use reversible source projects with real canonical metadata declarations.

use eska::project_core::{
    ProjectName,
    creation::import_query_project,
    source_format::{SourcePath, SourceReader},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(any(feature = "check-cli", feature = "analyzer-cli"))]
use super::composition_project as composition_fixture;
use super::query_creation_fixture as native_fixture;

pub const TYPES: [&str; 4] = native_fixture::TYPES;

pub struct Project {
    pub root: PathBuf,
    pub source: SourcePath,
    pub catalog: SourcePath,
}

/// Standalone CF has a real typed catalog; external profiles keep their honest absent-base boundary.
pub fn project(parent: &Path, kind: &str) -> Project {
    create(
        parent,
        kind,
        "project",
        "NativeName",
        native_fixture::ROOT_UUID,
    )
}

/// Source-only comparisons prove that native validation never writes the imported project.
pub fn snapshot(project: &Project) -> native_fixture::Snapshot {
    native_fixture::snapshot(&project.root)
}

/// Source-owned B declares base + A while the sibling remains a registered invisible contributor.
#[cfg(any(feature = "check-cli", feature = "analyzer-cli"))]
pub fn composition(parent: &Path) -> (Project, PathBuf, SourcePath) {
    let topology = composition_fixture::project(parent);
    let project = create(
        parent,
        "extension",
        "selected",
        "B",
        "10000000-0000-4000-8000-00000000000f",
    );
    fs::write(
        project.root.join("bsl-analyzer.toml"),
        composition_fixture::CONFIG,
    )
    .unwrap();
    (project, topology.roots["a"].clone(), topology.catalog)
}

/// Every mutation belongs to the caller's new playground fixture and occurs before source import.
fn create(parent: &Path, kind: &str, directory: &str, name: &str, uuid: &str) -> Project {
    let native = native_fixture::native(parent, kind);
    let root_header = native.input.join(&native.root);
    let mut header = fs::read_to_string(&root_header)
        .unwrap()
        .replace(native_fixture::ROOT_UUID, uuid)
        .replace("<Name>NativeName</Name>", &format!("<Name>{name}</Name>"));
    if kind == "configuration" {
        header = header.replace("<ChildObjects>", "<ChildObjects><Catalog>Items</Catalog>");
        fs::create_dir_all(native.input.join("Catalogs")).unwrap();
        fs::write(native.input.join("Catalogs/Items.xml"),"<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"><Catalog uuid=\"90000000-0000-4000-8000-000000000001\"><Properties><Name>Items</Name><CodeLength>9</CodeLength><DescriptionLength>100</DescriptionLength></Properties><ChildObjects><Attribute uuid=\"91000000-0000-4000-8000-000000000001\"><Properties><Name>CurrentField</Name><Type><v8:Type>xs:string</v8:Type><v8:StringQualifiers><v8:Length>40</v8:Length><v8:AllowedLength>Variable</v8:AllowedLength></v8:StringQualifiers></Type></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>").unwrap();
    }
    fs::write(root_header, header).unwrap();
    let root = parent.join(directory);
    import_query_project(
        &native.input,
        &root,
        ProjectName::parse("query-analysis".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let reader = SourceReader::open(&root.join("src")).unwrap();
    assert_eq!(reader.configuration().path.as_str(), native.root_source);
    assert!(
        reader
            .declared_owner(&SourcePath::parse(native.form_source.to_str().unwrap()).unwrap())
            .is_ok()
    );
    assert!(root.join("src").join(native.module_source).is_file());
    for path in [
        &native.root,
        &native.form,
        &native.dialog,
        &native.schema,
        &native.module,
    ] {
        assert!(native.input.join(path).is_file());
    }
    let native_resource = SourcePath::parse(native.schema.to_str().unwrap()).unwrap();
    let source = reader
        .query_paths()
        .find(|path| reader.declared_query(path).unwrap().native_resource == &native_resource)
        .unwrap()
        .clone();
    for path in reader.query_paths() {
        fs::write(root.join("src").join(path.as_path()), "SELECT 1 AS Value").unwrap();
    }
    let catalog = reader
        .owner_for_native_main(&SourcePath::parse("Catalogs/Items.xml").unwrap())
        .cloned()
        .unwrap_or_else(|| SourcePath::parse("Catalogs/Items/object.yaml").unwrap());
    Project {
        root,
        source,
        catalog,
    }
}
