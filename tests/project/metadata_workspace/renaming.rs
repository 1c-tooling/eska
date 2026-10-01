//! Structural previews must be useful without granting publication of unverified references.

use super::*;
use eska::project::metadata_workspace::RenameError;

mod bsl;
mod history;

/// A static configuration contains both readable code and an opaque protected module.
fn rename_fixture() -> (TestDir, MetadataWorkspace) {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    (directory, workspace)
}

/// A rename preview includes physical companions while preserving the complete filesystem snapshot.
#[test]
fn preview_rename_moves_catalog_and_payload_but_never_writes_sources() {
    let (directory, mut workspace) = rename_fixture();
    let before = bytes(&directory.0);
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Catalog, "Контрагенты", None);
    let plan = project.preview_rename(&id, "Партнеры").unwrap();
    assert_eq!(plan.new_object_id.as_str(), "catalog:Партнеры");
    assert_eq!(plan.moves.len(), 2);
    assert_eq!(plan.moves[0].from, Path::new("Catalogs/Контрагенты.xml"));
    assert_eq!(plan.moves[0].to, Path::new("Catalogs/Партнеры.xml"));
    assert_eq!(plan.moves[1].to, Path::new("Catalogs/Партнеры"));
    let changes: Vec<_> = plan
        .files
        .iter()
        .filter(|file| !file.replacements.is_empty())
        .collect();
    assert_eq!(changes.len(), 2);
    for file in changes {
        assert_eq!(file.replacements.len(), 1);
        assert_eq!(file.replacements[0].after, "Партнеры");
    }
    assert!(
        plan.issues
            .iter()
            .any(|issue| issue.reason == "opaque_code_or_form")
    );
    assert_eq!(bytes(&directory.0), before);
}

/// New references and changed modules invalidate the source-wide preview snapshot.
#[test]
fn preview_rename_distinguishes_nested_fields_and_detects_new_source_files() {
    let (directory, mut workspace) = rename_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let owner = object(MetadataKind::Catalog, "Контрагенты", None);
    let id = object(MetadataKind::Attribute, "ИНН", Some(owner));
    let first = project.preview_rename(&id, "Номер").unwrap();
    assert!(first.moves.is_empty());
    let changes: Vec<_> = first
        .files
        .iter()
        .filter(|file| !file.replacements.is_empty())
        .collect();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].replacements.len(), 1);
    let module = directory.0.join("src/Ext/SessionModule.bsl");
    fs::write(&module, "// ИНН\r\nСообщить(\"ИНН\");\r\n").unwrap();
    let next = project.preview_rename(&id, "Номер").unwrap();
    assert_ne!(next.snapshot, first.snapshot);
    let code = next
        .files
        .iter()
        .find(|file| file.path == Path::new("Ext/SessionModule.bsl"))
        .unwrap();
    assert!(code.replacements.is_empty());
    assert_eq!(code.uncertain.len(), 2);
    fs::write(directory.0.join("src/new.xml"), "<r/>").unwrap();
    assert_ne!(
        project.preview_rename(&id, "Номер").unwrap().snapshot,
        next.snapshot
    );
}

/// Both declared metadata and undeclared filesystem collisions prevent a misleading preview.
#[test]
fn preview_rename_rejects_declared_and_physical_case_insensitive_collisions() {
    let (directory, mut workspace) = rename_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Catalog, "Контрагенты", None);
    assert!(matches!(
        project.preview_rename(&id, "безфайла"),
        Err(RenameError::Collision(_))
    ));
    fs::create_dir(directory.0.join("src/Catalogs/Новый")).unwrap();
    assert!(matches!(
        project.preview_rename(&id, "новый"),
        Err(RenameError::Collision(_))
    ));
    assert!(matches!(
        project.preview_rename(&id, "Недопустимое.Имя"),
        Err(RenameError::Name(_))
    ));
    let plan = project.preview_rename(&id, "контрагенты").unwrap();
    assert_eq!(plan.moves.len(), 2);
}

/// External artifacts keep their own root identity and nested form declarations.
#[test]
fn preview_rename_accepts_root_and_nested_forms_of_external_artifacts() {
    for case in ["processing", "report", "extension"] {
        let directory = TestDir::new();
        fixture(&directory.0, case);
        let before = bytes(&directory.0);
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let NodeId::Object(root) = project.root().clone() else {
            panic!("root")
        };
        let plan = project.preview_rename(&root, "НовоеИмя").unwrap();
        assert_eq!(
            plan.files
                .iter()
                .map(|file| file.replacements.len())
                .sum::<usize>(),
            1
        );
        assert!(plan.moves.is_empty());
        if case == "processing" {
            let form = object(MetadataKind::Form, "Основная", Some(root));
            let plan = project.preview_rename(&form, "НоваяФорма").unwrap();
            assert_eq!(plan.moves.len(), 2);
            assert_eq!(
                plan.files
                    .iter()
                    .map(|file| file.replacements.len())
                    .sum::<usize>(),
                2
            );
        }
        assert_eq!(bytes(&directory.0), before);
    }
}

#[cfg(unix)]
/// A textual candidate cannot authorize following a source alias.
#[test]
fn preview_rename_rejects_source_symlinks_before_scanning_their_targets() {
    let (directory, mut workspace) = rename_fixture();
    std::os::unix::fs::symlink("Configuration.xml", directory.0.join("src/alias.xml")).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Catalog, "Контрагенты", None);
    assert!(matches!(
        project.preview_rename(&id, "НовоеИмя"),
        Err(RenameError::Io { .. })
    ));
}

/// A malformed unrelated payload remains an explicit obstacle instead of silently hiding references.
#[test]
fn preview_rename_reports_unreadable_xml_with_its_path() {
    let (directory, mut workspace) = rename_fixture();
    fs::write(directory.0.join("src/broken.xml"), "<broken>Контрагенты").unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Catalog, "Контрагенты", None);
    let plan = project.preview_rename(&id, "Партнеры").unwrap();
    assert!(plan.issues.iter().any(|issue| issue.path == Path::new("broken.xml") && issue.reason == "xml_unavailable"));
    assert!(plan.files.iter().any(|file| !file.replacements.is_empty()));
}

/// Git state and generated artifacts cannot invalidate a source snapshot when source equals project root.
#[test]
fn preview_rename_excludes_service_directories_for_project_root_sources() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    copy(&directory.0.join("src"), &directory.0);
    fs::remove_dir_all(directory.0.join("src")).unwrap();
    fs::write(
        directory.0.join("eska.toml"),
        "[project]\ntype='configuration'\nsource='.'\n",
    )
    .unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Catalog, "Контрагенты", None);
    let first = project.preview_rename(&id, "Партнеры").unwrap();
    for folder in [".git", ".eska", "build"] {
        fs::create_dir_all(directory.0.join(folder)).unwrap();
        fs::write(directory.0.join(folder).join("bad.xml"), "<bad>Контрагенты").unwrap();
    }
    let next = project.preview_rename(&id, "Партнеры").unwrap();
    assert_eq!(first.snapshot, next.snapshot);
    assert!(
        !next
            .issues
            .iter()
            .any(|issue| issue.path.ends_with("bad.xml"))
    );
}

/// Oversized unrelated text is hashed in bounded memory and reported as an obstacle, not a missing reference.
#[test]
fn preview_rename_reports_large_text_without_discarding_the_remaining_plan() {
    let (directory, mut workspace) = rename_fixture();
    fs::File::create(directory.0.join("src/large.bsl"))
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Catalog, "Контрагенты", None);
    let plan = project.preview_rename(&id, "Партнеры").unwrap();
    assert!(
        plan.issues
            .iter()
            .any(|issue| issue.path == Path::new("large.bsl") && issue.reason == "text_too_large")
    );
    assert!(plan.files.iter().any(|file| !file.replacements.is_empty()));
}

/// Register field kinds share one namespace, while fields of unrelated owners remain independent.
#[test]
fn rename_checks_cross_kind_register_names() {
    let (directory, workspace) = rename_fixture();
    drop(workspace);
    let root = directory.0.join("src/Configuration.xml");
    let input = fs::read_to_string(&root).unwrap().replace(
        "</ChildObjects>",
        "<InformationRegister>Регистр</InformationRegister></ChildObjects>",
    );
    fs::write(root, input).unwrap();
    let folder = directory.0.join("src/InformationRegisters");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("Регистр.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><InformationRegister uuid="register"><Properties><Name>Регистр</Name></Properties><ChildObjects><Dimension uuid="dimension"><Properties><Name>Измерение</Name></Properties></Dimension><Resource uuid="resource"><Properties><Name>Ресурс</Name></Properties></Resource><Attribute uuid="attribute"><Properties><Name>Реквизит</Name></Properties></Attribute></ChildObjects></InformationRegister></MetaDataObject>"#).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let owner = object(MetadataKind::InformationRegister, "Регистр", None);
    for (kind, name) in [
        (MetadataKind::Dimension, "Измерение"),
        (MetadataKind::Resource, "Ресурс"),
        (MetadataKind::Attribute, "Реквизит"),
    ] {
        for destination in ["измерение", "ресурс", "реквизит"] {
            if name.to_lowercase() == destination {
                continue;
            }
            let id = object(kind, name, Some(owner.clone()));
            assert!(
                matches!(
                    project.preview_rename(&id, destination),
                    Err(RenameError::Collision(_))
                ),
                "{name} -> {destination}"
            );
        }
    }
}

/// A missed watcher event must not allow a cached root to hide a newly declared collision.
#[test]
fn rename_preview_rereads_declarations_before_checking_collisions() {
    let (directory, mut workspace) = rename_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Catalog, "Контрагенты", None);
    project.preview_rename(&id, "Партнеры").unwrap();
    let root = directory.0.join("src/Configuration.xml");
    let input = fs::read_to_string(&root).unwrap().replace(
        "</ChildObjects>",
        "<Catalog>Партнеры</Catalog></ChildObjects>",
    );
    fs::write(root, input).unwrap();
    assert!(matches!(
        project.preview_rename(&id, "партнеры"),
        Err(RenameError::Collision(_))
    ));
}
