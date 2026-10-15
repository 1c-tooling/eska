//! Saved/editor BSL and YAML reach actual IDE queries in one source generation.

use std::fs;

use base_db::SourceDatabase;
use eska::analysis_core::{Cancellation, InputFailure, InputOrigin, InputRequest};

use super::{
    AnalysisSession, SessionOptions, SessionRequestError, SourcePath,
    tests::{draft, fixture, load, owner},
};
use crate::{
    DiagnosticCode, DiagnosticsConfig, Locale, ModuleFailure, ModuleState, QueryCancellation,
    QueryError,
};

/// Keep body locations explicit, independent of the working directory.
fn tools() -> SourcePath {
    owner("CommonModules/Tools/Ext/Module.bsl")
}

/// Use one stable syntax profile for disk/editor parity; semantic coverage has separate tests.
fn syntax_profile(locale: Locale) -> DiagnosticsConfig {
    DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::ParseError]),
        locale,
        ..DiagnosticsConfig::default()
    }
}

/// Read syntax findings with the same profile used by the independent batch session.
fn syntax(
    session: &AnalysisSession,
    path: &SourcePath,
    locale: Locale,
) -> crate::ModuleResult<Vec<crate::Diagnostic>> {
    session
        .module_diagnostics(
            path,
            session.revision(),
            &syntax_profile(locale),
            &QueryCancellation::default(),
        )
        .unwrap()
}

/// One listing survives pending/readable/unread transitions and retains the common-module backlink.
#[test]
fn lazy_module_states_and_current_readability_are_not_empty_success() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = tools();
    let caller = owner("Catalogs/Sample/Ext/ObjectModule.bsl");
    let caller_file = session.selected_root.modules[&caller].file;
    let pending = session.revision();
    assert_eq!(session.module_summary().pending, 2);
    assert_eq!(session.module_summary().binary, 1);
    assert!(!session.module_summary().module_input_complete);
    assert_eq!(
        session.module_owner(&path),
        Some(&owner("CommonModules/Tools/object.yaml"))
    );
    assert_eq!(
        session
            .database
            .resolve_common_module_files_for_file(caller_file, "Tools")
            .unread()
            .count(),
        1
    );
    assert!(matches!(
        session.module_diagnostics(
            &path,
            pending,
            &syntax_profile(Locale::Ru),
            &QueryCancellation::default()
        ),
        Err(QueryError::ModuleUnavailable { .. })
    ));
    session
        .load_module_saved(&path, pending, &InputRequest::default())
        .unwrap();
    assert_eq!(
        session
            .database
            .resolve_common_module_files_for_file(caller_file, "Tools")
            .unread()
            .count(),
        0
    );
    assert!(
        !session
            .database
            .file_is_unread(session.selected_root.modules[&path].file)
    );
    assert!(matches!(
        session.resolve(&owner("CommonModules/Tools/object.yaml"), pending),
        Err(SessionRequestError::StaleRevision { .. })
    ));
    fs::remove_file(root.join(path.as_path())).unwrap();
    session
        .load_module_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(
        matches!(&session.module(&path).unwrap().state, ModuleState::Unavailable(ModuleFailure::Input(InputFailure::Source(error))) if error.code == "source_file_missing")
    );
    assert_eq!(
        session
            .database
            .resolve_common_module_files_for_file(caller_file, "Tools")
            .unread()
            .count(),
        1
    );
    assert!(
        session
            .database
            .file_is_unread(session.selected_root.modules[&path].file)
    );
    assert_eq!(session.module_summary().loaded_bytes, 0);
    let binary = owner("Catalogs/Sample/Ext/ManagerModule.bin");
    let current = session.revision();
    assert!(matches!(
        session.apply_module_overlay(
            &binary,
            current,
            1,
            "readable but still binary",
            &InputRequest::default()
        ),
        Err(SessionRequestError::BinaryModule(_))
    ));
    assert_eq!(session.revision(), current);
}

/// Closing an editor re-reads disk, and identical saved bytes/profile give identical diagnostics.
#[test]
fn saved_editor_close_and_locales_share_current_diagnostics() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    load(&mut session);
    let path = tools();
    session
        .load_module_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let original = fs::read(root.join(path.as_path())).unwrap();
    let saved = syntax(&session, &path, Locale::Ru);
    assert!(saved.value.is_empty());
    assert!(!saved.metadata_models_complete);
    assert!(!saved.module_inputs_complete);
    let invalid = "\u{feff}Процедура Ошибка(\r\n";
    session
        .apply_module_overlay(
            &path,
            session.revision(),
            1,
            invalid,
            &InputRequest::default(),
        )
        .unwrap();
    let current = session.revision();
    let ru = syntax(&session, &path, Locale::Ru);
    let en = syntax(&session, &path, Locale::En);
    assert!(!ru.value.is_empty());
    assert_eq!(ru.revision, current);
    assert_eq!(
        ru.source_sha256,
        session
            .module(&path)
            .unwrap()
            .source_sha256
            .clone()
            .unwrap()
    );
    assert_eq!(
        ru.value
            .iter()
            .map(|d| (d.code, d.range))
            .collect::<Vec<_>>(),
        en.value
            .iter()
            .map(|d| (d.code, d.range))
            .collect::<Vec<_>>()
    );
    for diagnostic in &ru.value {
        assert!(
            invalid
                .get(
                    u32::from(diagnostic.range.start()) as usize
                        ..u32::from(diagnostic.range.end()) as usize
                )
                .is_some()
        );
    }
    assert_eq!(fs::read(root.join(path.as_path())).unwrap(), original);
    assert!(matches!(
        session.apply_module_overlay(&path, current, 1, "ignored", &InputRequest::default()),
        Err(SessionRequestError::StaleDocumentVersion { .. })
    ));
    let changed_saved = "\u{feff}Процедура НовыйТест()\r\nКонецПроцедуры\r\n";
    fs::write(root.join(path.as_path()), changed_saved).unwrap();
    assert!(matches!(
        session.load_module_saved(&path, current, &InputRequest::default()),
        Err(SessionRequestError::OverlayActive { .. })
    ));
    assert_eq!(
        session
            .module_diagnostics(
                &path,
                current,
                &syntax_profile(Locale::Ru),
                &QueryCancellation::default()
            )
            .unwrap(),
        ru
    );
    session
        .close_module_overlay(&path, current, &InputRequest::default())
        .unwrap();
    assert_eq!(session.module(&path).unwrap().origin, InputOrigin::Saved);
    let closed = syntax(&session, &path, Locale::Ru);
    let mut batch = AnalysisSession::open(&root).unwrap();
    load(&mut batch);
    batch
        .load_module_saved(&path, batch.revision(), &InputRequest::default())
        .unwrap();
    let batch = syntax(&batch, &path, Locale::Ru);
    assert_eq!(closed.value, batch.value);
    assert_eq!(closed.source_sha256, batch.source_sha256);
    assert_ne!(closed.source_sha256, saved.source_sha256);
    assert_eq!(
        fs::read_to_string(root.join(path.as_path())).unwrap(),
        changed_saved
    );
}

/// Actual BSL completion sees a YAML draft immediately, and an invalid draft drops the old fields.
#[test]
fn yaml_and_bsl_editor_changes_use_one_clock_and_current_inference() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    load(&mut session);
    let path = owner("Catalogs/Sample/Ext/ObjectModule.bsl");
    let text = "\u{feff}Процедура Проверка()\r\n\tОбъект = Справочники.Sample.СоздатьЭлемент();\r\n\tОбъект.\r\nКонецПроцедуры\r\n";
    session
        .apply_module_overlay(&path, session.revision(), 1, text, &InputRequest::default())
        .unwrap();
    let offset = u32::try_from(text.find("Объект.\r\n").unwrap() + "Объект.".len()).unwrap();
    let saved = session
        .module_completions(
            &path,
            session.revision(),
            offset,
            Locale::Ru,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert!(
        saved.value.iter().any(|item| item.label == "Description"),
        "{:?}",
        saved.value
    );
    let prior = session.revision();
    let catalog = owner("Catalogs/Sample/object.yaml");
    let text = draft(&root, &catalog, "Description", "LiveField");
    session
        .apply_overlay(&catalog, prior, 1, &text, &InputRequest::default())
        .unwrap();
    assert!(matches!(
        session.module_completions(
            &path,
            prior,
            offset,
            Locale::Ru,
            &QueryCancellation::default()
        ),
        Err(QueryError::Request(
            SessionRequestError::StaleRevision { .. }
        ))
    ));
    let current = session
        .module_completions(
            &path,
            session.revision(),
            offset,
            Locale::Ru,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert!(current.value.iter().any(|item| item.label == "LiveField"));
    assert!(!current.value.iter().any(|item| item.label == "Description"));
    assert_eq!(current.source_sha256, saved.source_sha256);
    session
        .apply_overlay(
            &catalog,
            session.revision(),
            2,
            "broken: [",
            &InputRequest::default(),
        )
        .unwrap();
    let invalid = session
        .module_completions(
            &path,
            session.revision(),
            offset,
            Locale::Ru,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert!(!invalid.value.iter().any(|item| item.label == "LiveField"));
    assert!(matches!(
        session.module_completions(
            &path,
            session.revision(),
            1,
            Locale::Ru,
            &QueryCancellation::default()
        ),
        Err(QueryError::InvalidOffset { .. })
    ));
}

/// Text reservations are replaced or released, not accumulated across overlay revisions.
#[test]
fn failed_budget_replacements_release_old_text_reservations() {
    let (_directory, root) = fixture();
    let path = tools();
    let saved = fs::read_to_string(root.join(path.as_path())).unwrap();
    let mut session = AnalysisSession::open_with_options(
        &root,
        SessionOptions {
            max_loaded_module_bytes: saved.len() as u64 + 1,
            ..SessionOptions::default()
        },
    )
    .unwrap();
    session
        .load_module_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let object = owner("Catalogs/Sample/Ext/ObjectModule.bsl");
    session
        .load_module_saved(&object, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(matches!(
        session.module(&object).unwrap().state,
        ModuleState::Unavailable(ModuleFailure::LoadedTextBudgetExceeded { .. })
    ));
    assert_eq!(session.module_summary().loaded_bytes, saved.len() as u64);
    session
        .apply_module_overlay(
            &path,
            session.revision(),
            1,
            "123",
            &InputRequest::default(),
        )
        .unwrap();
    session
        .load_module_saved(&object, session.revision(), &InputRequest::default())
        .unwrap();
    let object_bytes = session.module(&object).unwrap().bytes();
    assert!(object_bytes > 0);
    session
        .apply_module_overlay(
            &path,
            session.revision(),
            2,
            &saved,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(matches!(
        session.module(&path).unwrap().state,
        ModuleState::Unavailable(ModuleFailure::LoadedTextBudgetExceeded { .. })
    ));
    assert_eq!(session.module_summary().loaded_bytes, object_bytes);
    assert!(!session.module_summary().module_input_complete);
}

/// Cancellation and a document limit replace current readable input with an explicit failure.
#[test]
fn cancelled_queries_and_bounded_saved_reads_never_use_previous_text() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let object = owner("Catalogs/Sample/Ext/ObjectModule.bsl");
    session
        .load_module_saved(&object, session.revision(), &InputRequest::default())
        .unwrap();
    let cancel = QueryCancellation::default();
    cancel.cancel();
    assert_eq!(
        session.module_diagnostics(
            &object,
            session.revision(),
            &syntax_profile(Locale::Ru),
            &cancel
        ),
        Err(QueryError::Cancelled)
    );
    session
        .load_module_saved(
            &object,
            session.revision(),
            &InputRequest {
                max_document_bytes: 1,
                cancellation: {
                    let input = Cancellation::default();
                    input.cancel();
                    input
                },
                ..InputRequest::default()
            },
        )
        .unwrap();
    assert!(matches!(
        session.module(&object).unwrap().state,
        ModuleState::Unavailable(ModuleFailure::Input(InputFailure::Cancelled))
    ));
    assert_eq!(session.module_summary().loaded_bytes, 0);
    assert!(matches!(
        session.module_diagnostics(
            &object,
            session.revision(),
            &syntax_profile(Locale::Ru),
            &QueryCancellation::default()
        ),
        Err(QueryError::ModuleUnavailable { .. })
    ));
    session
        .load_module_saved(
            &object,
            session.revision(),
            &InputRequest {
                max_document_bytes: 1,
                cancellation: Cancellation::default(),
                ..InputRequest::default()
            },
        )
        .unwrap();
    assert!(matches!(
        session.module(&object).unwrap().state,
        ModuleState::Unavailable(ModuleFailure::Input(InputFailure::DocumentTooLarge {
            limit: 1
        }))
    ));
}

/// Coverage counters recover after accepted failures; rejected paths never advance the session.
#[test]
fn complete_small_session_counts_each_current_metadata_and_module_input() {
    let (directory, _root) = fixture();
    let native = directory.0.join("small-native");
    fs::create_dir_all(native.join("CommonModules/Tools/Ext")).unwrap();
    fs::write(native.join("Configuration.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>CounterProbe</Name></Properties><ChildObjects><CommonModule>Tools</CommonModule></ChildObjects></Configuration></MetaDataObject>").unwrap();
    fs::write(native.join("CommonModules/Tools.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><CommonModule uuid=\"20000000-0000-4000-8000-000000000001\"><Properties><Name>Tools</Name><Server>true</Server></Properties></CommonModule></MetaDataObject>").unwrap();
    fs::write(
        native.join("CommonModules/Tools/Ext/Module.bsl"),
        "// Проверяет ввод сессии.\r\nПроцедура Тест() Экспорт\r\nКонецПроцедуры\r\n",
    )
    .unwrap();
    let root = directory.0.join("small-source");
    eska::project_core::source_format::import_designer(&native, &root).unwrap();
    let mut session = AnalysisSession::open(&root).unwrap();
    assert!(!session.summary().descriptor_models_complete);
    load(&mut session);
    assert!(session.summary().descriptor_models_complete);
    assert!(!session.module_summary().module_input_complete);
    let path = tools();
    session
        .load_module_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    let result = syntax(&session, &path, Locale::Ru);
    assert!(result.metadata_models_complete);
    assert!(result.module_inputs_complete);
    let metadata = owner("CommonModules/Tools/object.yaml");
    session
        .apply_overlay(
            &metadata,
            session.revision(),
            1,
            "broken: [",
            &InputRequest::default(),
        )
        .unwrap();
    assert!(!session.summary().descriptor_models_complete);
    assert!(!syntax(&session, &path, Locale::Ru).metadata_models_complete);
    session
        .close_overlay(&metadata, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(syntax(&session, &path, Locale::Ru).metadata_models_complete);
    session
        .load_module_saved(
            &path,
            session.revision(),
            &InputRequest {
                max_document_bytes: 1,
                cancellation: Cancellation::default(),
                ..InputRequest::default()
            },
        )
        .unwrap();
    assert!(!session.module_summary().module_input_complete);
    session
        .load_module_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(syntax(&session, &path, Locale::Ru).module_inputs_complete);
    let current = session.revision();
    let model = session.module(&path).unwrap().clone();
    assert!(matches!(
        session.load_module_saved(
            &owner("stray/Ext/Module.bsl"),
            current,
            &InputRequest::default()
        ),
        Err(SessionRequestError::UnknownModule(_))
    ));
    assert_eq!(session.revision(), current);
    assert_eq!(session.module(&path), Some(&model));
}
