use super::{fixture, object};
use crate::support::TestDir;
use eska::project::{
    configurator::{ChildrenState, TreeOptions},
    metadata_model::{CollectionKind, MetadataKind, NodeId, ProjectScope},
    metadata_workspace::{
        MetadataWorkspace, WorkspaceError,
        search::{IndexState, SearchOptions},
    },
};
use std::{fs, path::PathBuf};

const PATH: &str = "Catalogs/Контрагенты/Ext/Predefined.xml";

/// Include BOM, CRLF, non-BMP text, nested items and names shared across different parents.
fn payload(name: &str) -> String {
    format!(
        "\u{feff}<?xml version='1.0' encoding='UTF-8'?>\r\n<PredefinedData xmlns='http://v8.1c.ru/8.3/xcf/predef' version='2.20'>\r\n<Item id='group'><Name>Группа</Name><Description>😀 Группировка</Description><IsFolder>true</IsFolder><ChildItems><Item id='child'><Name>{name}</Name><Description>Поисковое описание</Description><Code>01</Code></Item></ChildItems></Item><Item id='other'><Name>{name}</Name><Description>Другое описание</Description></Item></PredefinedData>"
    )
}

/// Expansion is lazy; XML positions, cache invalidation, deletion and malformed-file recovery are local.
#[test]
fn predefined_items_load_on_group_expansion_and_refresh_with_their_owner() {
    let dir = TestDir::new();
    fixture(&dir.0, "configuration");
    let file = dir.0.join("src").join(PATH);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "<broken>").unwrap();
    let mut workspace = MetadataWorkspace::open(&dir.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let catalog = object(MetadataKind::Catalog, "Контрагенты", None);
    let node = NodeId::Object(catalog.clone());
    let group = NodeId::Collection {
        owner: catalog.clone(),
        kind: CollectionKind::Metadata(MetadataKind::PredefinedItem),
    };
    let children = session.children(&node, TreeOptions::default()).unwrap();
    assert_eq!(
        children[0].id,
        NodeId::Collection {
            owner: catalog,
            kind: CollectionKind::Modules
        }
    );
    assert_eq!(children[1].id, group);
    assert_eq!(children[1].state, ChildrenState::Unloaded);
    assert_eq!(
        session.cache_stats().parses,
        2,
        "payload was not read while expanding the catalog"
    );
    assert!(session.children(&group, TreeOptions::default()).is_err());
    assert_eq!(session.node(&group).unwrap().state, ChildrenState::Error);
    fs::write(&file, payload("Элемент")).unwrap();
    session.changed_paths(&[PathBuf::from(PATH)]).unwrap();
    session.children(&node, TreeOptions::default()).unwrap();
    let children = session.children(&group, TreeOptions::default()).unwrap();
    assert_eq!(children.len(), 2);
    let parent = children[0].id.clone();
    let child = session.children(&parent, TreeOptions::default()).unwrap()[0]
        .id
        .clone();
    let NodeId::Object(child_id) = &child else {
        panic!("item")
    };
    assert_eq!(session.source(&child).unwrap()[0].path, PathBuf::from(PATH));
    let properties = session.properties(child_id).unwrap();
    let name = properties
        .iter()
        .find(|p| p.property.key.name == "Name")
        .unwrap();
    let input = fs::read_to_string(&file).unwrap();
    assert_eq!(&input[name.range.clone()], "<Name>Элемент</Name>");
    assert_eq!(
        name.property.key.namespace.as_deref(),
        Some("http://v8.1c.ru/8.3/xcf/predef")
    );
    let parses = session.cache_stats().parses;
    session.children(&group, TreeOptions::default()).unwrap();
    session.properties(child_id).unwrap();
    assert_eq!(session.cache_stats().parses, parses);
    fs::write(&file, payload("Другой")).unwrap();
    session.changed_paths(&[PathBuf::from(PATH)]).unwrap();
    assert!(matches!(
        session.node(&child),
        Err(WorkspaceError::UnknownNode(_))
    ));
    session.children(&node, TreeOptions::default()).unwrap();
    session.children(&group, TreeOptions::default()).unwrap();
    let refreshed = session.children(&parent, TreeOptions::default()).unwrap()[0]
        .id
        .clone();
    assert_ne!(child, refreshed);
    fs::remove_file(&file).unwrap();
    session.changed_paths(&[PathBuf::from(PATH)]).unwrap();
    session.children(&node, TreeOptions::default()).unwrap();
    assert!(
        session
            .children(&group, TreeOptions::default())
            .unwrap()
            .is_empty()
    );
    assert_eq!(session.node(&group).unwrap().state, ChildrenState::Empty);
}

/// Search covers unopened payloads by name and description and reveals their real grouped ancestry.
#[test]
fn predefined_search_reveals_unopened_nested_items_and_discards_removed_records() {
    let dir = TestDir::new();
    fixture(&dir.0, "configuration");
    let file = dir.0.join("src").join(PATH);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, payload("УникальныйЭлемент")).unwrap();
    let mut workspace = MetadataWorkspace::open(&dir.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    session.start_search_index();
    for _ in 0..100 {
        if session.index_search_step(64).state != IndexState::Building {
            break;
        }
    }
    let results = session.search("Поисковое описание", &SearchOptions::default());
    assert_eq!(results.hits.len(), 1);
    let hit = &results.hits[0];
    assert_eq!(hit.kind, MetadataKind::PredefinedItem);
    assert!(
        session.node(&hit.node).is_err(),
        "indexing does not expand navigation"
    );
    assert_eq!(session.reveal_search_hit(hit).unwrap(), hit.ancestry);
    assert_eq!(
        session.reveal_indexed_object(&hit.object).unwrap(),
        hit.ancestry
    );
    assert_eq!(
        session
            .search("УникальныйЭлемент", &SearchOptions::default())
            .hits
            .len(),
        2
    );
    fs::remove_file(&file).unwrap();
    session.changed_paths(&[PathBuf::from(PATH)]).unwrap();
    assert!(
        session
            .search("УникальныйЭлемент", &SearchOptions::default())
            .hits
            .is_empty()
    );
    for _ in 0..100 {
        if session.index_search_step(64).state != IndexState::Building {
            break;
        }
    }
    assert!(
        session
            .search("УникальныйЭлемент", &SearchOptions::default())
            .hits
            .is_empty()
    );
}

/// Reopening reuses the shared disk cache, while offline payload changes cannot resurrect old items.
#[test]
fn predefined_disk_cache_reuses_parses_and_detects_offline_changes() {
    let dir = TestDir::new();
    fixture(&dir.0, "configuration");
    let file = dir.0.join("src").join(PATH);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, payload("Первый")).unwrap();
    for phase in 0..3 {
        if phase == 2 {
            fs::write(&file, payload("Второй")).unwrap();
        }
        let mut workspace = MetadataWorkspace::open_cached(&dir.0, &[], false).unwrap();
        let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let owner = object(MetadataKind::Catalog, "Контрагенты", None);
        session
            .children(&NodeId::Object(owner.clone()), TreeOptions::default())
            .unwrap();
        let group = NodeId::Collection {
            owner,
            kind: CollectionKind::Metadata(MetadataKind::PredefinedItem),
        };
        let rows = session.children(&group, TreeOptions::default()).unwrap();
        assert_eq!(rows.len(), 2);
        let parent = rows[0].id.clone();
        let item = session.children(&parent, TreeOptions::default()).unwrap()[0]
            .id
            .clone();
        let expected = if phase == 2 {
            "Второй"
        } else {
            "Первый"
        };
        let NodeId::Object(id) = item else {
            panic!("item")
        };
        assert_eq!(session.object(&id).unwrap().name, expected);
        if phase == 1 {
            assert_eq!(session.cache_stats().parses, 0);
        }
        if phase == 2 {
            assert_eq!(session.cache_stats().parses, 1);
        }
    }
}
