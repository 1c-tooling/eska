//! Source-derived global context and the standalone resolver serve the same public rename preview.

use super::*;

/// Add explicit global flags to compact fixtures without touching the checked-in Designer exports.
fn code_fixture() -> (TestDir, MetadataWorkspace, PathBuf) {
    let (directory, workspace) = rename_fixture();
    drop(workspace);
    for name in ["ОбщийМодуль", "ЗащищенныйМодуль"] {
        let file = directory.0.join(format!("src/CommonModules/{name}.xml"));
        let input = fs::read_to_string(&file)
            .unwrap()
            .replace("</Properties>", "<Global>false</Global></Properties>");
        fs::write(file, input).unwrap();
    }
    let module = directory
        .0
        .join("src/CommonModules/ОбщийМодуль/Ext/Module.bsl");
    fs::write(&module, "\u{feff}Procedure Run()\r\n A = Catalogs.Контрагенты;\r\nEndProcedure\r\nProcedure Local(Catalogs)\r\n A = Catalogs.Контрагенты;\r\nEndProcedure\r\n").unwrap();
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    (directory, workspace, module)
}

/// The public preview proves global accesses but retains shadowed local occurrences and original bytes.
#[test]
fn rename_preview_resolves_bsl_globals_without_an_external_analyzer() {
    let (directory, mut workspace, module) = code_fixture();
    let before = bytes(&directory.0);
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Catalog, "Контрагенты", None);
    let plan = project.preview_rename(&id, "Партнеры").unwrap();
    let file = plan
        .files
        .iter()
        .find(|file| file.path == module.strip_prefix(directory.0.join("src")).unwrap())
        .unwrap();
    assert_eq!(file.replacements.len(), 1);
    assert_eq!(file.uncertain.len(), 1);
    assert_eq!(file.uncertain[0].reason, "bsl_shadowed_name");
    let input = fs::read_to_string(&module).unwrap();
    assert_eq!(&input[file.replacements[0].range.clone()], "Контрагенты");
    assert_eq!(bytes(&directory.0), before);
}

/// A new global declaration invalidates a previously proven binding, including a hidden global module.
#[test]
fn global_declarations_and_protected_exports_block_guessed_bindings() {
    let (directory, mut workspace, _) = code_fixture();
    let id = object(MetadataKind::Catalog, "Контрагенты", None);
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let first = project.preview_rename(&id, "Партнеры").unwrap();
    let module = Path::new("CommonModules/ОбщийМодуль/Ext/Module.bsl");
    fs::write(
        directory.0.join("src/Ext/ManagedApplicationModule.bsl"),
        "Var Catalogs Export;\r\n",
    )
    .unwrap();
    let second = project.preview_rename(&id, "Партнеры").unwrap();
    assert_ne!(first.snapshot, second.snapshot);
    let file = second
        .files
        .iter()
        .find(|file| file.path == module)
        .unwrap();
    assert!(file.replacements.is_empty());
    assert!(
        file.uncertain
            .iter()
            .all(|item| item.reason == "bsl_shadowed_name")
    );
    fs::write(
        directory.0.join("src/Ext/ManagedApplicationModule.bsl"),
        "// no globals",
    )
    .unwrap();
    let descriptor = directory.0.join("src/CommonModules/ЗащищенныйМодуль.xml");
    let input = fs::read_to_string(&descriptor)
        .unwrap()
        .replace("<Global>false</Global>", "<Global>true</Global>");
    fs::write(descriptor, input).unwrap();
    let third = project.preview_rename(&id, "Партнеры").unwrap();
    let file = third.files.iter().find(|file| file.path == module).unwrap();
    assert!(file.replacements.is_empty());
    assert!(
        file.uncertain
            .iter()
            .all(|item| item.reason == "bsl_global_context_unverified")
    );
}

/// The common-module property exists only for a non-global module, regardless of its display name.
#[test]
fn common_module_binding_uses_descriptor_global_flag() {
    let (directory, mut workspace, _) = code_fixture();
    let path = Path::new("Ext/SessionModule.bsl");
    fs::write(
        directory.0.join("src").join(path),
        "ОбщийМодуль.Выполнить();",
    )
    .unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::CommonModule, "ОбщийМодуль", None);
    let plan = project.preview_rename(&id, "Сервис").unwrap();
    assert_eq!(
        plan.files
            .iter()
            .find(|file| file.path == path)
            .unwrap()
            .replacements
            .len(),
        1
    );
    let descriptor = directory.0.join("src/CommonModules/ОбщийМодуль.xml");
    let input = fs::read_to_string(&descriptor)
        .unwrap()
        .replace("<Global>false</Global>", "<Global>true</Global>");
    fs::write(descriptor, input).unwrap();
    let plan = project.preview_rename(&id, "Сервис").unwrap();
    assert!(
        plan.files
            .iter()
            .find(|file| file.path == path)
            .unwrap()
            .replacements
            .is_empty()
    );
}

/// An already proven call cannot keep its binding when the new module name is a local variable.
#[test]
fn renamed_module_destination_shadow_is_a_blocking_plan_issue() {
    let (_directory, mut workspace, module) = code_fixture();
    fs::write(
        module,
        "Procedure Run(Сервис)\r\n ОбщийМодуль.Выполнить();\r\nEndProcedure",
    )
    .unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::CommonModule, "ОбщийМодуль", None);
    let plan = project.preview_rename(&id, "Сервис").unwrap();
    assert!(
        plan.issues
            .iter()
            .any(|item| item.reason == "bsl_destination_shadowed")
    );
}
