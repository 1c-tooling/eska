use std::{
    fs,
    path::{Path, PathBuf},
};

use eska::project::{
    designer_source::{SourceError, SourceRole, open_projects},
    metadata_model::{MetadataKind, MetadataObject, ModuleRole},
    metadata_parser::{self, PropertiesMode},
    object_model,
};
use serde::Deserialize;

use crate::support::TestDir;

#[derive(Deserialize)]
struct ExpectedCase {
    name: String,
    project_type: String,
    root_descriptor: String,
    object_ids: Vec<String>,
}

/// Locate immutable input fragments; runnable projects are created only in the playground.
fn fragments() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/designer")
}

/// Copy only test-owned fragments, preserving their BOM and newline bytes.
fn copy_fragments(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_fragments(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Reusable Designer fragments have reviewed identities and agree in both source directions.
#[test]
fn designer_fixture_snapshots_cover_four_project_types_and_source_ownership() {
    let expected: Vec<ExpectedCase> =
        serde_json::from_str(include_str!("../fixtures/designer/expected.json")).unwrap();
    for case in expected {
        let directory = TestDir::new();
        copy_fragments(&fragments().join(&case.name), &directory.0.join("src"));
        fs::write(
            directory.0.join("eska.toml"),
            format!("[project]\ntype='{}'\n", case.project_type),
        )
        .unwrap();
        let source = open_projects(&directory.0, &[], false).unwrap().remove(0);
        assert_eq!(source.descriptor(), Path::new(&case.root_descriptor));
        let model = object_model::discover(source.project()).unwrap();
        assert_eq!(
            model
                .objects()
                .map(|object| object.id().to_string())
                .collect::<Vec<_>>(),
            case.object_ids,
            "{}",
            case.name
        );
        for object in model.objects() {
            for mode in [PropertiesMode::Summary, PropertiesMode::All] {
                let parsed = metadata_parser::load(&source, object.id(), mode).unwrap();
                assert!(
                    parsed.diagnostics.is_empty(),
                    "{}: {:?}",
                    object.id(),
                    parsed.diagnostics
                );
                let actual = parsed
                    .objects
                    .iter()
                    .find(|parsed| parsed.metadata.id() == object.id())
                    .unwrap();
                assert_eq!(&actual.metadata, object.metadata());
                assert_eq!(actual.properties.is_some(), mode == PropertiesMode::All);
            }
            let locations = source.sources(object.id()).unwrap();
            assert!(!locations.is_empty(), "{}", object.id());
            for location in locations
                .iter()
                .filter(|location| matches!(location.role, SourceRole::Module(_)))
            {
                let affected = source
                    .changed_owners(std::slice::from_ref(&location.path))
                    .unwrap();
                assert!(affected.issues().is_empty());
                assert_eq!(
                    affected
                        .model()
                        .objects_for_changed_path(&location.path)
                        .iter()
                        .map(|owner| owner.id())
                        .collect::<Vec<_>>(),
                    vec![object.id()],
                    "{}: {:?}",
                    case.name,
                    location.path
                );
            }
        }
        if case.name == "configuration" {
            let missing = MetadataObject::new(
                MetadataKind::Catalog,
                "БезФайла".into(),
                "shared".into(),
                None,
            )
            .unwrap();
            assert!(source.sources(missing.id()).unwrap().is_empty());
            let binary = MetadataObject::new(
                MetadataKind::CommonModule,
                "ЗащищенныйМодуль".into(),
                "shared".into(),
                None,
            )
            .unwrap();
            assert!(
                source
                    .module(binary.id(), ModuleRole::Module)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(source.sources(binary.id()).unwrap().len(), 1);
        }
    }
}

/// Prefixes and encoding variants are valid; malformed, foreign and unknown roots are explicit.
#[test]
fn designer_edge_fragments_preserve_encoding_and_report_invalid_roots() {
    let directory = TestDir::new();
    fs::create_dir(directory.0.join("src")).unwrap();
    fs::write(
        directory.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    for case in [
        "prefixed.xml",
        "malformed.xml",
        "foreign-namespace.xml",
        "unknown.xml",
    ] {
        let bytes = fs::read(fragments().join("edge-cases").join(case)).unwrap();
        fs::write(directory.0.join("src/Configuration.xml"), &bytes).unwrap();
        let result = open_projects(&directory.0, &[], false);
        if case == "prefixed.xml" {
            assert!(bytes.starts_with(&[0xef, 0xbb, 0xbf]));
            assert!(bytes.windows(2).any(|bytes| bytes == b"\r\n"));
            assert_eq!(
                result.unwrap()[0].root().id().as_str(),
                "configuration:Префикс"
            );
        } else {
            assert!(matches!(
                result,
                Err(SourceError::InvalidRoot { .. } | SourceError::InvalidMetadata { .. })
            ));
        }
        assert_eq!(
            fs::read(directory.0.join("src/Configuration.xml")).unwrap(),
            bytes
        );
    }
    assert!(MetadataKind::from_xml_tag("FutureObject").is_err());
}

/// Manually exercise a real export without copying, changing or building that configuration.
#[test]
#[ignore = "manual read-only smoke; requires ESKA_DESIGNER_PROJECT"]
fn manual_large_designer_open_and_source_lookup() {
    let path = std::env::var_os("ESKA_DESIGNER_PROJECT").expect("explicit read-only project path");
    let started = std::time::Instant::now();
    let source = open_projects(Path::new(&path), &[], false)
        .unwrap()
        .remove(0);
    let opened = started.elapsed();
    let root_sources = source.sources(source.root().id()).unwrap();
    assert!(
        root_sources
            .iter()
            .any(|location| location.role == SourceRole::Descriptor)
    );
    let input = fs::read_to_string(source.project().source().join(source.descriptor())).unwrap();
    let document = roxmltree::Document::parse(&input).unwrap();
    let object = document
        .root_element()
        .children()
        .find(roxmltree::Node::is_element)
        .unwrap();
    let children = object
        .children()
        .find(|node| node.has_tag_name(("http://v8.1c.ru/8.3/MDClasses", "ChildObjects")))
        .unwrap();
    let mut checked = 0;
    for child in children
        .children()
        .filter(roxmltree::Node::is_element)
        .take(12)
    {
        let kind = MetadataKind::from_xml_tag(child.tag_name().name()).unwrap();
        let metadata =
            MetadataObject::new(kind, child.text().unwrap().into(), String::new(), None).unwrap();
        let locations = source.sources(metadata.id()).unwrap();
        assert!(
            locations
                .iter()
                .any(|location| location.role == SourceRole::Descriptor)
        );
        let parsed = metadata_parser::load(&source, metadata.id(), PropertiesMode::All).unwrap();
        assert!(
            parsed.diagnostics.is_empty(),
            "{}: {:?}",
            metadata.id(),
            parsed.diagnostics
        );
        checked += 1;
    }
    assert!(checked > 0);
    println!(
        "root_open_ms={} sampled_objects={checked} total_ms={}",
        opened.as_millis(),
        started.elapsed().as_millis()
    );
}

/// Reading a parent does not read child descriptors, modules or form/template payloads.
#[test]
fn designer_parser_loads_only_requested_descriptors_and_checks_inline_identity() {
    let directory = TestDir::new();
    copy_fragments(&fragments().join("configuration"), &directory.0.join("src"));
    fs::write(
        directory.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    let source = open_projects(&directory.0, &[], false).unwrap().remove(0);
    let owner = MetadataObject::new(
        MetadataKind::Catalog,
        "Контрагенты".into(),
        "id".into(),
        None,
    )
    .unwrap();
    let form = MetadataObject::new(
        MetadataKind::Form,
        "ФормаЭлемента".into(),
        "id".into(),
        Some(owner.id().clone()),
    )
    .unwrap();
    fs::write(
        directory
            .0
            .join("src/Catalogs/Контрагенты/Forms/ФормаЭлемента.xml"),
        "<broken>",
    )
    .unwrap();
    let parsed = metadata_parser::load(&source, owner.id(), PropertiesMode::Summary).unwrap();
    assert!(parsed.diagnostics.is_empty());
    assert!(
        parsed
            .references
            .iter()
            .any(|reference| &reference.id == form.id())
    );
    assert!(matches!(
        metadata_parser::load(&source, form.id(), PropertiesMode::All),
        Err(metadata_parser::LoadError::Parse { .. })
    ));
    let missing = MetadataObject::new(
        MetadataKind::Attribute,
        "НетТакого".into(),
        "id".into(),
        Some(owner.id().clone()),
    )
    .unwrap();
    assert!(matches!(
        metadata_parser::load(&source, missing.id(), PropertiesMode::Summary),
        Err(metadata_parser::LoadError::ObjectNotFound(_))
    ));
    let root = metadata_parser::load(&source, source.root().id(), PropertiesMode::Summary).unwrap();
    assert!(
        root.references
            .iter()
            .any(|reference| reference.name == "БезФайла")
    );
    assert!(root.diagnostics.is_empty());
}
