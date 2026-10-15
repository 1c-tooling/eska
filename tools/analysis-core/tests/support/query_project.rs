//! Complete imported all-type query projects preserve native aliases and use only owned playground paths.

use std::{fs, path::PathBuf};

use eska::project_core::{
    ProjectName,
    creation::import_query_project,
    source_format::{SourcePath, SourceReader},
};

use super::TestDirectory;

#[path = "../../../../tests/fixtures/eska-format/query_creation.rs"]
mod native_fixture;

pub const TYPES: [&str; 4] = native_fixture::TYPES;
pub const BAD: &str =
    "\u{feff}// Я😀 current query\r\nВЫБРАТЬ Таблица. ИЗ Справочник.Нет КАК Таблица\r\n";

pub struct QueryProject {
    pub _directory: TestDirectory,
    pub root: PathBuf,
    pub source: SourcePath,
    pub neighbor: SourcePath,
}

/// Both plain queries are chosen by declaration; physical artifact stems never determine metadata names.
pub fn project(kind: &str) -> QueryProject {
    let directory = TestDirectory::new();
    let native = native_fixture::native(&directory.0, kind);
    let root = directory.0.join("project");
    import_query_project(
        &native.input,
        &root,
        ProjectName::parse("query-tests".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let reader = SourceReader::open(&root.join("src")).unwrap();
    assert_eq!(reader.configuration().path.as_str(), native.root_source);
    assert_eq!(
        reader.configuration().declarations[0].uuid.as_str(),
        native_fixture::ROOT_UUID
    );
    assert_eq!(
        reader
            .declared_owner(&SourcePath::parse(native.form_source.to_str().unwrap()).unwrap())
            .unwrap()
            .uuid
            .as_str(),
        native_fixture::FORM_UUID
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
    let resource = SourcePath::parse(native.schema.to_str().unwrap()).unwrap();
    let source = reader
        .query_paths()
        .find(|source| reader.declared_query(source).unwrap().native_resource == &resource)
        .unwrap()
        .clone();
    let neighbor = reader
        .query_paths()
        .find(|path| *path != &source)
        .unwrap()
        .clone();
    for path in reader.query_paths() {
        fs::write(root.join("src").join(path.as_path()), "SELECT 1 AS Value").unwrap();
    }
    QueryProject {
        _directory: directory,
        root,
        source,
        neighbor,
    }
}

/// Whole-tree byte controls include settings, protected candidates and source artifacts.
pub fn snapshot(project: &QueryProject) -> native_fixture::Snapshot {
    native_fixture::snapshot(&project.root)
}
