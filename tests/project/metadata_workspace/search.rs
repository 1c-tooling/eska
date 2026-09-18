use super::{bytes, fixture, object};
use crate::support::TestDir;
use eska::project::{
    metadata_model::{MetadataKind, ProjectScope},
    metadata_workspace::{
        MetadataWorkspace, ProjectSession, WorkspaceError,
        search::{IndexState, MatchRank, SearchOptions},
    },
};
use std::{fmt::Write, fs, path::PathBuf};

/// Drain small cooperative steps with an explicit guard against a scheduling cycle.
fn finish(session: &mut ProjectSession) {
    for _ in 0..1000 {
        if session.index_search_step(2).state != IndexState::Building {
            return;
        }
    }
    panic!("index did not settle");
}

/// Search finds unopened inline elements and reveals only their real presentation ancestry.
#[test]
fn indexes_unopened_branches_without_expanding_navigation_or_writing_files() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let before = bytes(&directory.0);
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let catalog = object(MetadataKind::Catalog, "Контрагенты", None);
    let attribute = object(MetadataKind::Attribute, "ИНН", Some(catalog));
    assert_eq!(session.search_progress().state, IndexState::NotStarted);
    assert!(
        session
            .search("инн", &SearchOptions::default())
            .hits
            .is_empty()
    );
    session.start_search_index();
    finish(session);
    // The fixture deliberately references one absent catalog.
    assert_eq!(session.search_progress().state, IndexState::Incomplete);
    assert_eq!(session.search_failures().count(), 1);
    assert!(matches!(
        session.object(&attribute),
        Err(WorkspaceError::UnknownObject(_))
    ));
    let response = session.search("  инН  ", &SearchOptions::default());
    assert_eq!(response.hits.len(), 1);
    let hit = &response.hits[0];
    assert_eq!(hit.object, attribute);
    assert_eq!(hit.rank, MatchRank::ExactName);
    assert_eq!(hit.ancestry.first(), Some(session.root()));
    assert_eq!(hit.ancestry.last(), Some(&hit.node));
    let parses = session.cache_stats().parses;
    assert_eq!(
        session.search("инн", &SearchOptions::default()).hits[0].object,
        attribute
    );
    assert_eq!(session.cache_stats().parses, parses);
    assert_eq!(session.reveal_search_hit(hit).unwrap(), hit.ancestry);
    workspace.close();
    assert_eq!(bytes(&directory.0), before);
}

/// Matching ranks are stable and language filters do not conflate distinct object identities.
#[test]
fn ranks_names_and_synonyms_and_reports_truncation() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let names = ["Тест", "Другой", "Тестовый", "Атест", "Ёлка"];
    let mut children = String::new();
    for name in names {
        write!(children, "<Catalog>{name}</Catalog>").unwrap();
    }
    fs::write(directory.0.join("src/Configuration.xml"), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='id'><Properties><Name>Корень</Name></Properties><ChildObjects>{children}</ChildObjects></Configuration></MetaDataObject>")).unwrap();
    for name in names {
        let synonym = if name == "Другой" {
            "Тест"
        } else {
            "Поставка"
        };
        fs::write(directory.0.join(format!("src/Catalogs/{name}.xml")), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:v8='http://v8.1c.ru/8.1/data/core'><Catalog uuid='id'><Properties><Name>{name}</Name><Synonym><v8:item><v8:lang>ru</v8:lang><v8:content>{synonym}</v8:content></v8:item><v8:item><v8:lang>en</v8:lang><v8:content>Delivery</v8:content></v8:item></Synonym></Properties><ChildObjects><Attribute uuid='id'><Properties><Name>Общий</Name></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>")).unwrap();
    }
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    session.start_search_index();
    finish(session);
    assert_eq!(session.search_progress().state, IndexState::Ready);
    let results = session.search("тЕст", &SearchOptions::default());
    assert_eq!(
        results.hits.iter().map(|hit| hit.rank).collect::<Vec<_>>(),
        vec![
            MatchRank::ExactName,
            MatchRank::ExactSynonym,
            MatchRank::PrefixName,
            MatchRank::SubstringName
        ]
    );
    assert_eq!(
        session
            .search("Общий", &SearchOptions::default())
            .hits
            .len(),
        5
    );
    assert!(
        session
            .search("елка", &SearchOptions::default())
            .hits
            .is_empty()
    );
    let limited = SearchOptions {
        limit: 2,
        synonym_language: None,
    };
    assert!(session.search("тест", &limited).truncated);
    assert_eq!(session.search("тест", &limited).hits.len(), 2);
    assert_eq!(
        session
            .search("тест", &limited)
            .hits
            .iter()
            .map(|hit| &hit.object)
            .collect::<Vec<_>>(),
        results.hits[..2]
            .iter()
            .map(|hit| &hit.object)
            .collect::<Vec<_>>()
    );
    let russian = SearchOptions {
        limit: 50,
        synonym_language: Some("ru".into()),
    };
    assert!(session.search("delivery", &russian).hits.is_empty());
    assert_eq!(
        session
            .search("delivery", &SearchOptions::default())
            .hits
            .len(),
        5
    );
    assert!(
        session
            .search(" ", &SearchOptions::default())
            .hits
            .is_empty()
    );
}

/// Cancellation pauses IO between steps, while later events are applied before resumed work.
#[test]
fn cancels_resumes_and_reindexes_changed_unopened_objects() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    session.start_search_index();
    session.index_search_step(1);
    session.cancel_search_index();
    let parses = session.cache_stats().parses;
    assert_eq!(session.index_search_step(64).state, IndexState::Cancelled);
    assert_eq!(session.cache_stats().parses, parses);
    session.resume_search_index();
    finish(session);
    let hit = session
        .search("инн", &SearchOptions::default())
        .hits
        .remove(0);
    let path = PathBuf::from("Catalogs/Контрагенты.xml");
    let file = session.project().source().join(&path);
    let xml = fs::read_to_string(&file)
        .unwrap()
        .replace("ИНН", "НовыйРеквизит");
    fs::write(file, xml).unwrap();
    let parses = session.cache_stats().parses;
    session.changed_paths(std::slice::from_ref(&path)).unwrap();
    assert!(matches!(
        session.reveal_search_hit(&hit),
        Err(WorkspaceError::StaleGeneration { .. })
    ));
    assert!(
        session
            .search("инн", &SearchOptions::default())
            .hits
            .is_empty()
    );
    finish(session);
    assert!(
        session
            .search("инн", &SearchOptions::default())
            .hits
            .is_empty()
    );
    assert_eq!(
        session
            .search("НовыйРеквизит", &SearchOptions::default())
            .hits
            .len(),
        1
    );
    assert_eq!(session.cache_stats().parses, parses + 1);
    assert_eq!(session.cache_stats().last_parsed, Some(path));
}

/// Root membership removes indexed descendants without rereading independent descriptors.
#[test]
fn root_deletion_removes_results_and_failed_descriptors_can_recover() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    session.start_search_index();
    finish(session);
    let path = PathBuf::from("Configuration.xml");
    let file = session.project().source().join(&path);
    fs::write(
        &file,
        fs::read_to_string(&file)
            .unwrap()
            .replace("<Catalog>Контрагенты</Catalog>", ""),
    )
    .unwrap();
    let parses = session.cache_stats().parses;
    session.changed_paths(&[path]).unwrap();
    finish(session);
    assert!(
        session
            .search("инн", &SearchOptions::default())
            .hits
            .is_empty()
    );
    assert_eq!(session.cache_stats().parses, parses + 1);
    let missing = PathBuf::from("Catalogs/БезФайла.xml");
    fs::write(session.project().source().join(&missing), "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Catalog uuid='id'><Properties><Name>БезФайла</Name></Properties></Catalog></MetaDataObject>").unwrap();
    session.changed_paths(&[missing]).unwrap();
    finish(session);
    assert_eq!(session.search_progress().state, IndexState::Ready);
    assert_eq!(
        session
            .search("БезФайла", &SearchOptions::default())
            .hits
            .len(),
        1
    );
}

/// Equal names and identities from separate members are still separate scoped results.
#[test]
fn keeps_projects_distinct_and_payloads_unread() {
    let directory = TestDir::new();
    for member in ["first", "second"] {
        fixture(&directory.0.join(member), "processing");
        fs::write(
            directory.0.join(member).join("eska.toml"),
            format!("[project]\nname='{member}'\ntype='processing'\n"),
        )
        .unwrap();
        fs::write(
            directory
                .0
                .join(member)
                .join("src/Выгрузка/Ext/ObjectModule.bsl"),
            [0xff],
        )
        .unwrap();
    }
    fs::write(
        directory.0.join("eska.toml"),
        "[workspace]\nmembers=['first','second']\n",
    )
    .unwrap();
    let before = bytes(&directory.0);
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], true).unwrap();
    let scopes: Vec<_> = workspace
        .projects()
        .iter()
        .map(|project| project.scope().clone())
        .collect();
    let mut hits = Vec::new();
    for scope in scopes {
        let session = workspace.project_mut(&scope).unwrap();
        session.start_search_index();
        finish(session);
        assert_eq!(session.search_progress().state, IndexState::Ready);
        hits.push(
            session
                .search("Параметр", &SearchOptions::default())
                .hits
                .remove(0),
        );
    }
    assert_eq!(hits[0].object, hits[1].object);
    assert_ne!(hits[0].project, hits[1].project);
    workspace.close();
    assert_eq!(bytes(&directory.0), before);
}

/// Broken XML remains an explicit partial result and recovers after a file event.
#[test]
fn malformed_descriptor_recovers_without_reopening_workspace() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let path = PathBuf::from("Catalogs/Контрагенты.xml");
    let file = directory.0.join("src").join(&path);
    let original = fs::read(&file).unwrap();
    fs::write(&file, "<broken").unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    session.start_search_index();
    finish(session);
    assert_eq!(session.search_progress().state, IndexState::Incomplete);
    assert_eq!(session.search_failures().count(), 2);
    assert!(
        session
            .search("инн", &SearchOptions::default())
            .hits
            .is_empty()
    );
    fs::write(file, original).unwrap();
    session.changed_paths(&[path]).unwrap();
    finish(session);
    assert_eq!(session.search_failures().count(), 1);
    assert_eq!(
        session.search("инн", &SearchOptions::default()).hits.len(),
        1
    );
}
