use super::*;
use crate::backend::project::metadata_workspace::search::IndexState;

/// Reverse lookup must agree with every declared forward mapping in all supported project layouts.
#[test]
fn source_navigation_matches_forward_sources_without_an_index() {
    for case in ["configuration", "extension", "processing", "report"] {
        let directory = TestDir::new();
        fixture(&directory.0, case);
        let before = bytes(&directory.0);
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let mut pending = vec![session.root().clone()];
        let mut sources = BTreeMap::new();
        while let Some(id) = pending.pop() {
            match session.children(&id, TreeOptions::default()) {
                Ok(children) => pending.extend(children.iter().map(|node| node.id.clone())),
                Err(WorkspaceError::MissingSource(_)) => continue,
                other => panic!("{case}: {other:?}"),
            }
            if let Ok(locations) = session.source(&id) {
                for source in locations {
                    if source.inline.is_empty() {
                        sources.insert(source.path.clone(), source);
                    }
                }
            }
        }
        assert!(!sources.is_empty());
        for (path, source) in sources {
            let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
            let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
            let found = session
                .reveal_source(&path)
                .unwrap()
                .unwrap_or_else(|| panic!("{case}: {}", path.display()));
            assert_eq!(found.source, source);
            assert_eq!(found.ancestry.first(), Some(session.root()));
            let target = found.ancestry.last().unwrap();
            assert_eq!(
                matches!(target, NodeId::Module { .. }),
                matches!(source.role, SourceRole::Module(_))
            );
            assert_eq!(session.search_progress().state, IndexState::NotStarted);
            for pair in found.ancestry.windows(2) {
                assert_eq!(
                    session.node(&pair[1]).unwrap().parent.as_ref(),
                    Some(&pair[0])
                );
            }
        }
        assert_eq!(before, bytes(&directory.0));
    }
}

/// Unknown physical files and undeclared descriptors do not acquire metadata identity.
#[test]
fn source_navigation_rejects_unknown_escaping_and_removed_sources() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let source = directory.0.join("src");
    fs::copy(
        source.join("Catalogs/Контрагенты.xml"),
        source.join("Catalogs/НеОбъявлен.xml"),
    )
    .unwrap();
    let unknown = "Catalogs/Контрагенты/Ext/Unknown.bsl";
    fs::write(source.join(unknown), "// unknown").unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let session = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    for path in [
        unknown,
        "Catalogs/НеОбъявлен.xml",
        "Catalogs/Missing.xml",
        "eska.toml",
    ] {
        assert!(
            session.reveal_source(Path::new(path)).unwrap().is_none(),
            "{path}"
        );
    }
    for path in [
        "",
        "../eska.toml",
        "/Configuration.xml",
        "Catalogs/../Configuration.xml",
    ] {
        assert!(matches!(
            session.reveal_source(Path::new(path)),
            Err(WorkspaceError::InvalidChangedPath(_))
        ));
    }
    let path = Path::new("Catalogs/Контрагенты/Ext/ObjectModule.bsl");
    assert!(session.reveal_source(path).unwrap().is_some());
    fs::remove_file(source.join(path)).unwrap();
    session.changed_paths(&[path.to_path_buf()]).unwrap();
    assert!(session.reveal_source(path).unwrap().is_none());
}

/// A source-shaped symlink may not navigate to a file outside the selected source root.
#[cfg(unix)]
#[test]
fn source_navigation_rejects_symlink_escape() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let path = Path::new("Catalogs/Контрагенты/Ext/ObjectModule.bsl");
    let physical = directory.0.join("src").join(path);
    fs::remove_file(&physical).unwrap();
    std::os::unix::fs::symlink(directory.0.join("eska.toml"), physical).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    assert!(matches!(
        workspace
            .project_mut(&ProjectScope::Standalone)
            .unwrap()
            .reveal_source(path),
        Err(WorkspaceError::Source(_))
    ));
}
