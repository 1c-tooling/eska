use crate::support::TestDir;
use eska::project::{
    configurator::{ChildrenState, TreeOptions},
    designer_source::SourceRole,
    metadata_model::{
        CollectionKind, MetadataKind, MetadataObject, ModuleRole, NodeId, ObjectId, ProjectScope,
    },
    metadata_parser::LoadError,
    metadata_workspace::{MetadataWorkspace, WorkspaceError},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// Copy static fixtures into a uniquely owned playground directory.
fn copy(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &path);
        } else {
            fs::copy(entry.path(), path).unwrap();
        }
    }
}

/// Create a project without invoking CLI, Git or the platform.
fn fixture(root: &Path, case: &str) {
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/designer")
            .join(case),
        &root.join("src"),
    );
    fs::write(
        root.join("eska.toml"),
        format!("[project]\ntype='{case}'\n"),
    )
    .unwrap();
}

/// Capture both paths and exact bytes to detect writes or newly created cache files.
fn bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            for (path, content) in bytes(&entry.path()) {
                result.insert(PathBuf::from(entry.file_name()).join(path), content);
            }
        } else {
            result.insert(
                PathBuf::from(entry.file_name()),
                fs::read(entry.path()).unwrap(),
            );
        }
    }
    result
}

/// Construct a typed caller ID; unknown IDs must never cause speculative filesystem discovery.
fn object(kind: MetadataKind, name: &str, parent: Option<ObjectId>) -> ObjectId {
    MetadataObject::new(kind, name.into(), "test".into(), parent)
        .unwrap()
        .id()
        .clone()
}

/// A library client walks every fixture type, retaining ancestry and every physical source.
#[test]
fn workspace_walks_four_project_types_without_writing_files() {
    for case in ["configuration", "extension", "report", "processing"] {
        let directory = TestDir::new();
        fixture(&directory.0, case);
        let before = bytes(&directory.0);
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let root = session.root().clone();
        let mut pending = vec![root.clone()];
        let mut seen = BTreeSet::new();
        let mut missing = 0;
        while let Some(id) = pending.pop() {
            assert!(seen.insert(id.clone()), "{case}: {id:?}");
            let path = session.ancestry(&id).unwrap();
            assert_eq!(path.first(), Some(&root));
            assert_eq!(path.last(), Some(&id));
            for pair in path.windows(2) {
                assert_eq!(
                    session.node(&pair[1]).unwrap().parent.as_ref(),
                    Some(&pair[0])
                );
            }
            if let NodeId::Object(owner) = &id {
                assert_eq!(&session.object(owner).unwrap().id, owner);
            }
            match session.children(
                &id,
                TreeOptions {
                    hide_empty_root_sections: false,
                },
            ) {
                Ok(children) => pending.extend(children.iter().map(|node| node.id.clone())),
                Err(WorkspaceError::MissingSource(_)) => {
                    missing += 1;
                    assert!(matches!(
                        session.source(&id),
                        Err(WorkspaceError::MissingSource(_))
                    ));
                    continue;
                }
                other => panic!("{case}: {other:?}"),
            }
            let sources = session.source(&id).unwrap();
            if matches!(id, NodeId::Collection { .. }) {
                assert!(sources.is_empty());
            } else {
                assert!(!sources.is_empty());
                for source in &sources {
                    assert!(session.project().source().join(&source.path).is_file());
                }
            }
            if let NodeId::Object(owner) = &id {
                let properties = session.properties(owner).unwrap();
                assert!(
                    properties
                        .iter()
                        .any(|property| property.property.key.name == "Name")
                );
            }
        }
        assert!(seen.len() > 5);
        assert_eq!(missing, usize::from(case == "configuration"));
        workspace.close();
        assert_eq!(bytes(&directory.0), before, "{case}");
    }
}

/// Unopened sibling XML can be broken; selection and repeated expansion use retained nodes.
#[test]
fn workspace_is_lazy_and_keeps_selection_free_of_parsing() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let catalog_file = directory.0.join("src/Catalogs/Контрагенты.xml");
    let original = fs::read(&catalog_file).unwrap();
    fs::write(&catalog_file, b"broken before open").unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let catalog = object(MetadataKind::Catalog, "Контрагенты", None);
    let id = NodeId::Object(catalog.clone());
    assert!(session.object(&catalog).unwrap().uuid.is_none());
    assert_eq!(session.node(&id).unwrap().state, ChildrenState::Unloaded);
    assert!(session.node(&id).unwrap().can_expand());
    assert!(matches!(
        session.children(&id, TreeOptions::default()),
        Err(WorkspaceError::Load(LoadError::Parse { .. }))
    ));
    assert_eq!(session.node(&id).unwrap().state, ChildrenState::Error);
    fs::write(&catalog_file, original).unwrap();
    let children: Vec<_> = session
        .children(&id, TreeOptions::default())
        .unwrap()
        .iter()
        .map(|node| node.id.clone())
        .collect();
    let ancestry = session.ancestry(&id).unwrap();
    assert!(session.object(&catalog).unwrap().uuid.is_some());
    fs::write(&catalog_file, b"broken after expansion").unwrap();
    assert_eq!(
        session
            .children(&id, TreeOptions::default())
            .unwrap()
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>(),
        children
    );
    assert_eq!(session.ancestry(&id).unwrap(), ancestry);
    assert_eq!(session.object(&catalog).unwrap().name, "Контрагенты");
    assert!(matches!(
        session.properties(&catalog),
        Err(WorkspaceError::Load(LoadError::Parse { .. }))
    ));
    let document = NodeId::Object(object(MetadataKind::Document, "Продажа", None));
    assert!(session.children(&document, TreeOptions::default()).is_ok());
}

/// Missing descriptor, missing module, unknown object and malformed XML are distinguishable.
#[test]
fn workspace_preserves_sources_and_structured_errors() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let unknown = object(MetadataKind::Catalog, "НеОбъявлен", None);
    assert!(matches!(
        session.object(&unknown),
        Err(WorkspaceError::UnknownObject(_))
    ));
    assert!(matches!(
        session.source(&NodeId::Object(unknown)),
        Err(WorkspaceError::UnknownNode(_))
    ));
    let missing = NodeId::Object(object(MetadataKind::Catalog, "БезФайла", None));
    assert!(matches!(
        session.children(&missing, TreeOptions::default()),
        Err(WorkspaceError::MissingSource(_))
    ));
    let catalog = object(MetadataKind::Catalog, "Контрагенты", None);
    let id = NodeId::Object(catalog.clone());
    session.children(&id, TreeOptions::default()).unwrap();
    let sources = session.source(&id).unwrap();
    assert!(
        sources
            .iter()
            .any(|source| source.role == SourceRole::Descriptor)
    );
    for role in [ModuleRole::Object, ModuleRole::Manager] {
        assert!(
            sources
                .iter()
                .any(|source| source.role == SourceRole::Module(role))
        );
        let module = NodeId::Module {
            owner: catalog.clone(),
            role,
        };
        assert_eq!(session.source(&module).unwrap().len(), 1);
    }
    let form = NodeId::Object(object(
        MetadataKind::Form,
        "ФормаЭлемента",
        Some(catalog.clone()),
    ));
    let form_sources = session.source(&form).unwrap();
    assert_eq!(form_sources.len(), 3);
    for role in [
        SourceRole::Descriptor,
        SourceRole::Module(ModuleRole::Module),
        SourceRole::Payload,
    ] {
        assert!(form_sources.iter().any(|source| source.role == role));
    }
    let attribute = object(MetadataKind::Attribute, "ИНН", Some(catalog.clone()));
    let sources = session.source(&NodeId::Object(attribute.clone())).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].inline.len(), 1);
    assert!(session.properties(&attribute).is_ok());
    let module = NodeId::Module {
        owner: catalog,
        role: ModuleRole::Object,
    };
    let path = session.source(&module).unwrap()[0].path.clone();
    fs::remove_file(session.project().source().join(path)).unwrap();
    assert!(matches!(
        session.source(&module),
        Err(WorkspaceError::MissingSource(_))
    ));
}

/// Filtering changes only the returned root sections, never ancestry or nested emptiness.
#[test]
fn workspace_filter_preserves_virtual_identity() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let root = session.root().clone();
    let all = session
        .children(
            &root,
            TreeOptions {
                hide_empty_root_sections: false,
            },
        )
        .unwrap()
        .len();
    let visible = session
        .children(&root, TreeOptions::default())
        .unwrap()
        .len();
    assert!(all > visible);
    let NodeId::Object(owner) = root else {
        panic!("object root")
    };
    let constant = NodeId::Collection {
        owner,
        kind: CollectionKind::Metadata(MetadataKind::Constant),
    };
    assert_eq!(session.node(&constant).unwrap().state, ChildrenState::Empty);
    assert!(!session.node(&constant).unwrap().can_expand());
    assert_eq!(session.ancestry(&constant).unwrap().len(), 2);
}

/// Identical project root IDs are scoped, and unselected malformed members stay unopened.
#[test]
fn workspace_scopes_members_and_honors_selection() {
    let directory = TestDir::new();
    for member in ["first", "second"] {
        fixture(&directory.0.join(member), "processing");
        fs::write(
            directory.0.join(member).join("eska.toml"),
            format!("[project]\nname='{member}'\ntype='processing'\n"),
        )
        .unwrap();
    }
    fs::write(
        directory.0.join("eska.toml"),
        "[workspace]\nmembers=['first','second']\n",
    )
    .unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], true).unwrap();
    assert_eq!(workspace.projects().len(), 2);
    let first = workspace.projects()[0].scope().clone();
    let second = workspace.projects()[1].scope().clone();
    assert_ne!(first, second);
    assert_eq!(
        workspace.project(&first).unwrap().root(),
        workspace.project(&second).unwrap().root()
    );
    let root = workspace.project(&first).unwrap().root().clone();
    workspace
        .project_mut(&first)
        .unwrap()
        .children(&root, TreeOptions::default())
        .unwrap();
    assert!(matches!(
        workspace.project(&ProjectScope::Standalone),
        Err(WorkspaceError::UnknownProject(_))
    ));
    let descriptor = fs::read_dir(directory.0.join("second/src"))
        .unwrap()
        .map(Result::unwrap)
        .find(|entry| entry.path().extension().is_some_and(|ext| ext == "xml"))
        .unwrap()
        .path();
    fs::write(descriptor, "broken root XML").unwrap();
    assert_eq!(
        MetadataWorkspace::open(&directory.0, &["first".into()], false)
            .unwrap()
            .projects()
            .len(),
        1
    );
}
