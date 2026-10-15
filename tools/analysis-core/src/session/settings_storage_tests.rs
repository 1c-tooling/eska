//! Configured settings managers share current canonical identity and authoritative native module text.

use super::*;
use crate::{
    Locale, ModuleState, QueryCancellation, QueryError,
    session::tests::{owner, support::TestDirectory},
    test_settings_storage_project as fixture,
};
use std::{fs, path::PathBuf};

const CALLER: &str = "CommonModules/Tools/Module.bsl";
const MANAGER: &str = "SettingsStorages/CurrentЯ/ManagerModule.bsl";
const STORAGE: &str = "SettingsStorages/CurrentЯ/object.yaml";
const BODY: &str = "Procedure Run()\n Manager=SettingsStorages.CurrentЯ;\n Value=Manager.Load(\"key\");\nEndProcedure\n";

/// Every imported source and its earlier native input belongs to one unique playground directory.
fn project(body: &str, manager: &str) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    fixture::add_storage(
        &root,
        &directory.0.join("native"),
        manager,
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let source = root.join("src-with-storages");
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_module_saved(&owner(CALLER), session.revision(), &InputRequest::default())
        .unwrap();
    (directory, source, session)
}

/// Cold preparation and the final native query use the same input controller and publication revision.
fn completions(
    session: &mut AnalysisSession,
    module: &str,
    body: &str,
    needle: &str,
    locale: Locale,
) -> Result<Vec<ide::CompletionItem>, QueryError> {
    let offset = u32::try_from(body.rfind(needle).unwrap() + needle.len()).unwrap();
    session.prepare_completion_metadata(
        &owner(module),
        session.revision(),
        offset,
        &InputRequest::default(),
        &QueryCancellation::default(),
    )?;
    Ok(session
        .module_completions(
            &owner(module),
            session.revision(),
            offset,
            locale,
            &QueryCancellation::default(),
        )?
        .value)
}

/// Both platform spellings expose the real manager and only its current exported user methods.
#[test]
fn settings_storage_cold_namespace_methods_and_exports_are_native() {
    for namespace in ["SettingsStorages.", "ХранилищаНастроек."] {
        let body = format!("Procedure Run()\n Value={namespace};\nEndProcedure\n");
        let (_directory, _source, mut session) = project(&body, fixture::SAVED);
        let items = completions(&mut session, CALLER, &body, namespace, Locale::En).unwrap();
        assert!(
            items.iter().any(|item| item.label == "CurrentЯ"),
            "{items:?}"
        );
    }
    let (_directory, source, mut session) = project(BODY, fixture::SAVED);
    for locale in [Locale::En, Locale::Ru] {
        let items = completions(&mut session, CALLER, BODY, "Manager.", locale).unwrap();
        assert!(
            items.iter().any(|item| item.label == "Загрузить"
                && item
                    .filter_text
                    .as_ref()
                    .is_some_and(|text| text.contains("Load"))),
            "{items:?}"
        );
        assert!(
            items.iter().any(|item| item.label == "SavedMethod"),
            "{items:?}"
        );
        assert!(
            !items
                .iter()
                .any(|item| item.label == "PrivateMethod" || item.label.contains("&lt;"))
        );
    }
    assert!(matches!(
        session.selected_root.owners[&owner("Catalogs/Unrelated/object.yaml")]
            .model
            .state,
        ModelState::Pending
    ));
    assert_eq!(fs::read_to_string(source.join(CALLER)).unwrap(), BODY);
}

/// Unsaved manager exports survive watcher invalidation, then close restores actual saved bytes.
#[test]
fn settings_storage_manager_exports_track_current_overlay_watch_and_close() {
    let (_directory, source, mut session) = project(BODY, fixture::SAVED);
    let labels = |session: &mut AnalysisSession| {
        completions(session, CALLER, BODY, "Manager.", Locale::En)
            .unwrap()
            .into_iter()
            .map(|item| item.label)
            .collect::<Vec<_>>()
    };
    assert!(
        labels(&mut session)
            .iter()
            .any(|label| label == "SavedMethod")
    );
    let path = owner(MANAGER);
    let request = InputRequest::default();
    session
        .apply_module_overlay(&path, session.revision(), 1, fixture::CURRENT, &request)
        .unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&path), session.revision())
        .unwrap();
    let current = labels(&mut session);
    assert!(
        current.iter().any(|label| label == "CurrentMethod"),
        "{current:?}"
    );
    assert!(!current.iter().any(|label| label == "SavedMethod"));
    session
        .close_module_overlay(&path, session.revision(), &request)
        .unwrap();
    assert!(
        labels(&mut session)
            .iter()
            .any(|label| label == "SavedMethod")
    );
    assert_eq!(
        fs::read_to_string(source.join(MANAGER)).unwrap(),
        fixture::SAVED
    );
}

/// A registered current identity cannot silently fall back to its old name or an XML descriptor.
#[test]
fn settings_storage_invalid_identity_retracts_current_namespace_and_repairs() {
    let (_directory, source, mut session) = project(BODY, fixture::SAVED);
    completions(&mut session, CALLER, BODY, "Manager.", Locale::En).unwrap();
    let path = owner(STORAGE);
    let saved = fs::read_to_string(source.join(STORAGE)).unwrap();
    let request = InputRequest::default();
    for (version, changed) in [
        (1, saved.replace("CurrentЯ", "ChangedЯ")),
        (2, "broken: [".into()),
    ] {
        session
            .apply_overlay(&path, session.revision(), version, &changed, &request)
            .unwrap();
        assert!(matches!(
            completions(&mut session, CALLER, BODY, "Manager.", Locale::En),
            Err(QueryError::Request(
                SessionRequestError::CompositionInputIncomplete(_)
            ))
        ));
    }
    session
        .apply_overlay(&path, session.revision(), 3, &saved, &request)
        .unwrap();
    assert!(
        completions(&mut session, CALLER, BODY, "Manager.", Locale::En)
            .unwrap()
            .iter()
            .any(|item| item.label == "Загрузить"
                && item
                    .filter_text
                    .as_ref()
                    .is_some_and(|text| text.contains("Load")))
    );
    session
        .close_overlay(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(fs::read_to_string(source.join(STORAGE)).unwrap(), saved);
}

/// The manager module uses its qualified storage namespace and the real native method card.
#[test]
fn settings_storage_own_manager_qualified_namespace_and_native_method_cards_are_current() {
    let manager =
        "Procedure Run()\n Value=SettingsStorages.CurrentЯ.Load(\"key\");\nEndProcedure\n";
    let (_directory, _source, mut session) = project(BODY, manager);
    session
        .load_module_saved(
            &owner(MANAGER),
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    let items = completions(
        &mut session,
        MANAGER,
        manager,
        "SettingsStorages.CurrentЯ.",
        Locale::En,
    )
    .unwrap();
    assert!(
        items.iter().any(|item| item.label == "Загрузить"
            && item
                .filter_text
                .as_ref()
                .is_some_and(|text| text.contains("Load"))),
        "{items:?}"
    );
    let offset = u32::try_from(manager.find("Load(").unwrap() + "Load(".len()).unwrap();
    session
        .prepare_signature_help_metadata(
            &owner(MANAGER),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    let signatures = session
        .module_signature_help(
            &owner(MANAGER),
            session.revision(),
            offset,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .unwrap();
    assert_eq!(
        signatures.signatures[0].parameters.len(),
        4,
        "{signatures:?}"
    );
    let offset = u32::try_from(manager.find("Load").unwrap()).unwrap();
    session
        .prepare_symbol_hover_metadata(
            &owner(MANAGER),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    let card = session
        .module_symbol_hover(
            &owner(MANAGER),
            session.revision(),
            offset,
            Locale::En,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .unwrap();
    assert_eq!(card.kind, crate::HoverKind::PlatformFunction);
    assert!(!card.symbol.contains("&lt;"), "{card:?}");
}

/// A local shadow and a platform reference in another procedure do not authorize unrelated metadata reads.
#[test]
fn settings_storage_shadowed_namespace_and_other_body_stay_cold() {
    let body = "Procedure Run()\n SettingsStorages=New Structure;\n Value=SettingsStorages.;\nEndProcedure\nProcedure Other()\n Value=ХранилищаНастроек.CurrentЯ.Load(\"key\");\nEndProcedure\n";
    let (_directory, _source, mut session) = project(body, fixture::SAVED);
    let items = completions(&mut session, CALLER, body, "SettingsStorages.", Locale::En).unwrap();
    assert!(!items.iter().any(|item| item.label == "CurrentЯ"));
    assert!(matches!(
        session.selected_root.owners[&owner(STORAGE)].model.state,
        ModelState::Pending
    ));
}

/// A protected binary manager remains a real unread body instead of a falsely absent optional module.
#[test]
fn settings_storage_protected_manager_preserves_unread_barrier() {
    let body = "Procedure Run()\n Manager=SettingsStorages.CurrentЯ;\n Manager.SavedMethod();\nEndProcedure\n";
    let (directory, source, _session) = project(body, fixture::SAVED);
    let root = source.parent().unwrap();
    let native = directory.0.join("native");
    fs::remove_file(native.join("SettingsStorages/CurrentЯ/Ext/ManagerModule.bsl")).unwrap();
    fs::write(
        native.join("SettingsStorages/CurrentЯ/Ext/ManagerModule.bin"),
        [0xff, 0xfe],
    )
    .unwrap();
    let source = root.join("src-protected");
    eska::project_core::source_format::import_designer_queries(&native, &source).unwrap();
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_module_saved(&owner(CALLER), session.revision(), &InputRequest::default())
        .unwrap();
    let items = completions(&mut session, CALLER, body, "Manager.", Locale::En).unwrap();
    assert!(items.iter().any(|item| {
        item.label == "Загрузить"
            && item
                .filter_text
                .as_ref()
                .is_some_and(|text| text.contains("Load"))
    }));
    assert!(!items.iter().any(|item| item.label == "SavedMethod"));
    let module =
        &session.selected_root.modules[&owner("SettingsStorages/CurrentЯ/ManagerModule.bin")];
    assert_eq!(module.model.state, ModuleState::Binary);
    let caller = session.selected_root.modules[&owner(CALLER)].file;
    let bodies = hir::ConfigsDatabase::resolve_settings_storage_manager_module_file_candidates(
        &session.database,
        caller,
        "CurrentЯ",
    )
    .unwrap();
    assert_eq!(bodies.unread().collect::<Vec<_>>(), [module.file]);
    session
        .load_semantic_inputs(session.revision(), &InputRequest::default())
        .unwrap();
    let diagnostics = session
        .module_diagnostics(
            &owner(CALLER),
            session.revision(),
            &crate::DiagnosticsConfig::default(),
            &QueryCancellation::default(),
        )
        .unwrap()
        .value;
    assert!(
        !diagnostics
            .iter()
            .any(|item| item.code == crate::DiagnosticCode::UnresolvedMethodCall),
        "{diagnostics:?}"
    );
}

/// One explicitly admitted base, selected extension and hidden sibling share immutable topology only.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
fn composition() -> (TestDirectory, PathBuf, PathBuf, AnalysisSession) {
    use eska::project_core::source_format::SourceReader;
    let directory = TestDirectory::new();
    let create = |name, extension, uuid, extra| {
        let own = directory.0.join(name);
        let root = crate::test_project::project(&own, BODY, false);
        fixture::add_storage(
            &root,
            &own.join("native"),
            fixture::SAVED,
            extension,
            uuid,
            extra,
        );
        root
    };
    let base = create("base", false, "70000000-0000-4000-8000-000000000001", "");
    let selected = create(
        "selected",
        true,
        "70000000-0000-4000-8000-000000000002",
        "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>70000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>",
    );
    let hidden = create("hidden", true, "70000000-0000-4000-8000-000000000003", "");
    let file = selected.join("bsl-analyzer.toml");
    fs::write(&file,"[source]\nroot='../../base/project/src-with-storages'\nextensions=[{name='Selected',path='src-with-storages'},{name='Hidden',path='../../hidden/project/src-with-storages'}]\n").unwrap();
    let settings = project_model::ProjectConfig::load_from_file(&file).unwrap();
    let readers = [&base, &selected, &hidden]
        .map(|root| SourceReader::open(&root.join("src-with-storages")).unwrap())
        .into_iter()
        .collect();
    let topology = crate::composition::admit(
        &selected,
        &settings,
        readers,
        &selected.join("src-with-storages"),
    )
    .unwrap();
    let session =
        AnalysisSession::from_composition(topology, crate::SessionOptions::default()).unwrap();
    (directory, base, selected, session)
}

/// Foreign current input and original UUID control selected-CFE methods while a sibling stays unread.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
#[test]
fn settings_storage_cfe_requires_current_base_and_exact_original_uuid() {
    use crate::composition::SourceKey;
    let (_directory, base, selected, mut session) = composition();
    let request = InputRequest::default();
    session
        .load_module_saved(&owner(CALLER), session.revision(), &request)
        .unwrap();
    assert!(
        completions(&mut session, CALLER, BODY, "Manager.", Locale::En)
            .unwrap()
            .iter()
            .any(|item| item.label == "Загрузить"
                && item
                    .filter_text
                    .as_ref()
                    .is_some_and(|text| text.contains("Load")))
    );
    let hidden = session.root_ids().find(|root| root.index() == 2).unwrap();
    assert!(matches!(
        session.root_inputs(hidden).unwrap().owners[&owner(STORAGE)]
            .model
            .state,
        ModelState::Pending
    ));
    let key = SourceKey {
        root: session.root_ids().find(|root| root.index() == 0).unwrap(),
        source: owner(STORAGE),
    };
    let base_saved = fs::read_to_string(base.join("src-with-storages").join(STORAGE)).unwrap();
    session
        .apply_overlay_at(&key, session.revision(), 1, "broken: [", &request)
        .unwrap();
    assert!(matches!(
        completions(&mut session, CALLER, BODY, "Manager.", Locale::En),
        Err(QueryError::Request(
            SessionRequestError::CompositionInputIncomplete(_)
        ))
    ));
    session
        .close_overlay_at(&key, session.revision(), &request)
        .unwrap();
    let own = owner(STORAGE);
    let saved = fs::read_to_string(selected.join("src-with-storages").join(STORAGE)).unwrap();
    let wrong = saved.replace(
        "70000000-0000-4000-8000-000000000001",
        "70000000-0000-4000-8000-000000000099",
    );
    assert_ne!(saved, wrong);
    session
        .apply_overlay(&own, session.revision(), 1, &wrong, &request)
        .unwrap();
    assert!(matches!(
        completions(&mut session, CALLER, BODY, "Manager.", Locale::En),
        Err(QueryError::Request(
            SessionRequestError::CompositionInputIncomplete(_)
        ))
    ));
    session
        .close_overlay(&own, session.revision(), &request)
        .unwrap();
    assert!(
        completions(&mut session, CALLER, BODY, "Manager.", Locale::En)
            .unwrap()
            .iter()
            .any(|item| item.label == "Загрузить"
                && item
                    .filter_text
                    .as_ref()
                    .is_some_and(|text| text.contains("Load")))
    );
    assert_eq!(
        fs::read_to_string(base.join("src-with-storages").join(STORAGE)).unwrap(),
        base_saved
    );
    assert_eq!(
        fs::read_to_string(selected.join("src-with-storages").join(STORAGE)).unwrap(),
        saved
    );
}
