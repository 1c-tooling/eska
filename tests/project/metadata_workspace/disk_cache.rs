use super::{bytes, fixture, object};
use crate::support::TestDir;
use eska::project::{
    configurator::TreeOptions,
    metadata_model::{MetadataKind, NodeId, ProjectScope},
    metadata_workspace::{
        MetadataWorkspace, ProjectSession,
        search::{IndexState, SearchOptions},
    },
};
use std::{fs, path::Path};

/// Build all declared metadata without relying on visible navigation branches.
fn index(session: &mut ProjectSession) {
    session.start_search_index();
    while session.index_search_step(64).state == IndexState::Building {}
}

/// Cached reopen bypasses both root discovery parsing and descriptor parsing in every format.
#[test]
fn cached_reopen_skips_xml_parsing_and_preserves_sources() {
    for case in ["configuration", "extension", "processing", "report"] {
        let directory = TestDir::new();
        fixture(&directory.0, case);
        let before = bytes(&directory.0);
        let mut first = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
        index(first.project_mut(&ProjectScope::Standalone).unwrap());
        first.close();
        let mut second = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
        let session = second.project_mut(&ProjectScope::Standalone).unwrap();
        index(session);
        assert_eq!(session.cache_stats().parses, 0, "{case}");
        assert_eq!(session.source_io_stats().root_parses, 0, "{case}");
        assert!(session.disk_cache_stats().unwrap().hits > 0);
        let after = bytes(&directory.0)
            .into_iter()
            .filter(|(path, _)| !path.starts_with(".eska"))
            .collect();
        assert_eq!(before, after);
    }
}

/// Offline edits are detected even when file length and modification time are unchanged.
#[test]
fn offline_changes_and_deleted_descriptors_never_restore_stale_results() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let mut first = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
    index(first.project_mut(&ProjectScope::Standalone).unwrap());
    first.close();
    let path = directory.0.join("src/Catalogs/Контрагенты.xml");
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let xml = fs::read_to_string(&path).unwrap().replace("ИНН", "КПП");
    fs::write(&path, xml).unwrap();
    fs::File::open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let mut second = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
    let session = second.project_mut(&ProjectScope::Standalone).unwrap();
    index(session);
    assert_eq!(
        session.search("КПП", &SearchOptions::default()).hits.len(),
        1
    );
    assert!(
        session
            .search("ИНН", &SearchOptions::default())
            .hits
            .is_empty()
    );
    assert_eq!(session.cache_stats().parses, 1);
    second.close();
    fs::remove_file(&path).unwrap();
    let mut third = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
    let session = third.project_mut(&ProjectScope::Standalone).unwrap();
    index(session);
    assert!(
        session
            .search("КПП", &SearchOptions::default())
            .hits
            .is_empty()
    );
    assert_eq!(session.search_progress().state, IndexState::Incomplete);
}

/// Disposable invalid or foreign-version entries are replaced, never surfaced as source failures.
#[test]
fn corrupted_and_old_cache_entries_rebuild() {
    let directory = TestDir::new();
    fixture(&directory.0, "processing");
    for corruption in ["truncated", "version", "checksum"] {
        MetadataWorkspace::open_cached(&directory.0, &[], false)
            .unwrap()
            .close();
        for entry in fs::read_dir(directory.0.join(".eska/cache/metadata")).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if corruption == "truncated" {
                    fs::write(path, "{").unwrap();
                } else {
                    let mut value: serde_json::Value =
                        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                    if corruption == "version" {
                        value["version"] = "old".into();
                    } else {
                        value["payload"] = "{}".into();
                    }
                    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
                }
            }
        }
        let workspace = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
        assert!(workspace.projects()[0].cache_stats().parses > 0);
    }
}

/// A cache path collision disables persistence without overwriting user files.
#[test]
fn unavailable_cache_falls_back_and_modules_are_probed_after_restart() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    fs::write(directory.0.join(".eska"), "user file").unwrap();
    let workspace = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
    assert!(workspace.projects()[0].disk_cache_stats().unwrap().failures > 0);
    assert_eq!(
        fs::read_to_string(directory.0.join(".eska")).unwrap(),
        "user file"
    );
}

/// Cache-derived branches must still check actual BSL availability after restart.
#[test]
fn module_creation_and_removal_are_visible_after_cached_restart() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let owner = object(MetadataKind::Catalog, "Контрагенты", None);
    let node = NodeId::Object(owner);
    let module = directory
        .0
        .join("src/Catalogs/Контрагенты/Ext/ManagerModule.bsl");
    for exists in [false, true, false] {
        if exists {
            fs::write(&module, "new module").unwrap();
        } else if module.exists() {
            fs::remove_file(&module).unwrap();
        }
        let mut workspace = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
        let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        session.children(&node, TreeOptions::default()).unwrap();
        let sources = session.source(&node).unwrap();
        assert_eq!(
            sources
                .iter()
                .any(|source| source.path.ends_with(Path::new("ManagerModule.bsl"))),
            exists
        );
    }
}

/// New declarations and manifest changes cannot reuse a stale root from a prior opening.
#[test]
fn root_membership_and_manifest_changes_are_validated() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    MetadataWorkspace::open_cached(&directory.0, &[], false)
        .unwrap()
        .close();
    let path = directory.0.join("src/Configuration.xml");
    let xml = fs::read_to_string(&path)
        .unwrap()
        .replace("<Catalog>БезФайла</Catalog>", "<Catalog>Новый</Catalog>");
    fs::write(&path, xml).unwrap();
    fs::write(directory.0.join("src/Catalogs/Новый.xml"), "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Catalog uuid='id'><Properties><Name>Новый</Name></Properties></Catalog></MetaDataObject>").unwrap();
    let mut workspace = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    index(session);
    assert!(
        session
            .search("Новый", &SearchOptions::default())
            .hits
            .iter()
            .any(|hit| hit.name == "Новый")
    );
    assert!(
        session
            .search("БезФайла", &SearchOptions::default())
            .hits
            .is_empty()
    );
    workspace.close();
    fs::write(
        directory.0.join("eska.toml"),
        "[project]\ntype='extension'\n",
    )
    .unwrap();
    assert!(MetadataWorkspace::open_cached(&directory.0, &[], false).is_err());
}

/// A cache symlink must not redirect writes into another directory.
#[cfg(unix)]
#[test]
fn cache_directory_symlink_is_not_followed() {
    let directory = TestDir::new();
    let outside = TestDir::new();
    fixture(&directory.0, "configuration");
    std::os::unix::fs::symlink(&outside.0, directory.0.join(".eska")).unwrap();
    let workspace = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
    assert!(workspace.projects()[0].disk_cache_stats().unwrap().failures > 0);
    assert!(fs::read_dir(&outside.0).unwrap().next().is_none());
}

/// Source discovery is declaration-driven even when unrelated XML and large BSL files exist.
#[test]
fn unrelated_sources_do_not_increase_root_or_search_io() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let mut first = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
    let session = first.project_mut(&ProjectScope::Standalone).unwrap();
    index(session);
    let before = session.source_io_stats();
    first.close();
    fs::write(directory.0.join("src/Catalogs/НеОбъявлен.xml"), "<broken").unwrap();
    fs::create_dir(directory.0.join("src/Unused")).unwrap();
    fs::write(
        directory.0.join("src/Unused/Module.bsl"),
        vec![0xff; 1024 * 1024],
    )
    .unwrap();
    let mut second = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
    let session = second.project_mut(&ProjectScope::Standalone).unwrap();
    index(session);
    let after = session.source_io_stats();
    assert_eq!(after.xml_reads, before.xml_reads);
    assert_eq!(after.xml_bytes, before.xml_bytes);
    assert_eq!(after.path_checks, before.path_checks);
    assert_eq!(after.directory_reads, 0);
    assert_eq!(session.cache_stats().parses, 0);
}

/// Concurrent sessions publish complete cache files without sharing mutable session state.
#[test]
fn concurrent_cache_writers_leave_readable_entries() {
    let directory = TestDir::new();
    fixture(&directory.0, "processing");
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                let mut workspace =
                    MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
                index(workspace.project_mut(&ProjectScope::Standalone).unwrap());
            });
        }
    });
    let mut workspace = MetadataWorkspace::open_cached(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    index(session);
    assert_eq!(session.cache_stats().parses, 0);
    assert_eq!(session.disk_cache_stats().unwrap().failures, 0);
}
