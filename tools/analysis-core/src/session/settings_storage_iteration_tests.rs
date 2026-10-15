//! Native collection iteration retains a generic callable manager without claiming a configured owner.

use super::*;
use crate::{
    Locale, ModuleState, QueryCancellation,
    session::tests::{owner, support::TestDirectory},
    test_settings_storage_project as fixture,
};
use base_db::RootQueryDb;
use bsl_types::{intern::TypeKernelDb, kind::TypeKind};
use std::fs;

const CALLER: &str = "CommonModules/Tools/Module.bsl";
const MANAGER: &str = "SettingsStorages/CurrentЯ/ManagerModule.bsl";

/// A real imported storage proves that iteration does not borrow its private configured identity or exports.
fn project(body: &str) -> (TestDirectory, AnalysisSession) {
    project_with_manager(body, fixture::SAVED)
}

/// Two real managers let the test distinguish the selected receiver's dependency from unrelated calls.
fn project_with_manager(body: &str, manager: &str) -> (TestDirectory, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    let native = directory.0.join("native");
    let configuration = native.join("Configuration.xml");
    let source = fs::read_to_string(&configuration).unwrap().replace(
        "</ChildObjects>",
        "<SettingsStorage>OtherЯ</SettingsStorage></ChildObjects>",
    );
    fs::write(configuration, source).unwrap();
    fs::create_dir_all(native.join("SettingsStorages/OtherЯ/Ext")).unwrap();
    fs::write(native.join("SettingsStorages/OtherЯ.xml"), "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'><SettingsStorage uuid='71000000-0000-4000-8000-000000000001'><Properties><Name>OtherЯ</Name></Properties><ChildObjects/></SettingsStorage></MetaDataObject>").unwrap();
    fs::write(
        native.join("SettingsStorages/OtherЯ/Ext/ManagerModule.bsl"),
        "Function OtherMethod() Export\n Return New Structure(\"ForeignKeyЯ\", 1);\nEndFunction\n",
    )
    .unwrap();
    fixture::add_storage(
        &root,
        &native,
        manager,
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let mut session = AnalysisSession::open(&root.join("src-with-storages")).unwrap();
    session
        .load_module_saved(&owner(CALLER), session.revision(), &InputRequest::default())
        .unwrap();
    (directory, session)
}

/// Current user-return shapes are prepared through the same public query route as direct manager members.
fn return_keys(session: &mut AnalysisSession, body: &str) -> Vec<String> {
    let module = owner(CALLER);
    let offset = u32::try_from(body.rfind(".SavedKeyЯ").unwrap() + 1).unwrap();
    let cancellation = QueryCancellation::default();
    session
        .prepare_completion_metadata(
            &module,
            session.revision(),
            offset,
            &InputRequest::default(),
            &cancellation,
        )
        .unwrap();
    session
        .module_completions(
            &module,
            session.revision(),
            offset,
            Locale::En,
            &cancellation,
        )
        .unwrap()
        .value
        .into_iter()
        .map(|item| item.label)
        .collect()
}

/// The same native candidate supplies signature help and the hovered platform method card.
fn assert_native_cards(session: &mut AnalysisSession, body: &str) {
    let module = owner(CALLER);
    let request = InputRequest::default();
    let cancellation = QueryCancellation::default();
    let offset = u32::try_from(body.find("Load(").unwrap() + "Load(".len()).unwrap();
    session
        .prepare_signature_help_metadata(
            &module,
            session.revision(),
            offset,
            &request,
            &cancellation,
        )
        .unwrap();
    let help = session
        .module_signature_help(&module, session.revision(), offset, &cancellation)
        .unwrap()
        .value
        .unwrap();
    assert_eq!(help.signatures[0].parameters.len(), 4, "{help:?}");
    let offset = u32::try_from(body.find("Load").unwrap()).unwrap();
    session
        .prepare_symbol_hover_metadata(&module, session.revision(), offset, &request, &cancellation)
        .unwrap();
    let card = session
        .module_symbol_hover(
            &module,
            session.revision(),
            offset,
            Locale::En,
            &cancellation,
        )
        .unwrap()
        .value
        .unwrap();
    assert_eq!(card.kind, crate::HoverKind::PlatformFunction);
    assert!(!card.symbol.contains("&lt;"), "{card:?}");
    let result = session
        .query_module(
            &module,
            session.revision(),
            &cancellation,
            |analysis, file| {
                let expression = analysis
                    .database()
                    .parse(file)
                    .syntax_node()
                    .descendants()
                    .find(|node| node.text() == "Storage.Load(\"key\")")
                    .unwrap();
                let ty = hir::Semantics::new(analysis.database()).type_of_expr(file, &expression);
                Ok(matches!(analysis.database().lookup_type(ty), TypeKind::Any))
            },
        )
        .unwrap();
    assert!(
        result.value,
        "A stored arbitrary value must not acquire a configured schema"
    );
}

/// A native generic manager exposes native methods in either language but never a particular owner's exports.
#[test]
fn settings_storage_iteration_keeps_native_methods_without_a_configured_owner() {
    for namespace in ["SettingsStorages", "ХранилищаНастроек"] {
        let body = format!(
            "Procedure Run()\n For Each Storage In {namespace} Do\n  Value=Storage.Load(\"key\");\n EndDo;\nEndProcedure\n"
        );
        let (_directory, mut session) = project(&body);
        let module = owner(CALLER);
        let offset = u32::try_from(body.find("Storage.Load").unwrap() + "Storage.".len()).unwrap();
        let request = InputRequest::default();
        let cancellation = QueryCancellation::default();
        session
            .prepare_completion_metadata(
                &module,
                session.revision(),
                offset,
                &request,
                &cancellation,
            )
            .unwrap();
        for locale in [Locale::En, Locale::Ru] {
            let items = session
                .module_completions(&module, session.revision(), offset, locale, &cancellation)
                .unwrap()
                .value;
            assert!(
                items.iter().any(|item| item.label == "Загрузить"
                    && item
                        .filter_text
                        .as_ref()
                        .is_some_and(|text| text.contains("Load"))),
                "{items:?}"
            );
            assert!(
                !items
                    .iter()
                    .any(|item| item.label == "SavedMethod" || item.label == "CurrentЯ"),
                "{items:?}"
            );
        }
        assert_native_cards(&mut session, &body);
        assert!(matches!(
            session.selected_root.modules[&owner(MANAGER)].model.state,
            ModuleState::Pending
        ));
        assert_eq!(
            fs::read_to_string(session.root().join(CALLER)).unwrap(),
            body
        );
    }
}

/// Cold aliases and parenthesized calls load their precise manager; argument and sibling managers remain unread.
#[test]
fn settings_storage_user_return_discovery_follows_receiver_spine_and_native_reaching_writes() {
    let manager = "Function ReadCurrent(Value = Undefined) Export\n Result=New Structure(\"SavedKeyЯ\", 1);\n Return Result;\nEndFunction\n";
    for receiver in [
        "Copy",
        "(SettingsStorages.CurrentЯ.ReadCurrent(SettingsStorages.OtherЯ.OtherMethod()))",
    ] {
        let body = format!(
            "Procedure Run()\n Other=SettingsStorages.OtherЯ.OtherMethod();\n Value=SettingsStorages.CurrentЯ.ReadCurrent(SettingsStorages.OtherЯ.OtherMethod());\n Copy=Value;\n Result={receiver}.SavedKeyЯ;\nEndProcedure\n"
        );
        let (_directory, mut session) = project_with_manager(&body, manager);
        let labels = return_keys(&mut session, &body);
        assert!(
            labels.iter().any(|label| label == "SavedKeyЯ"),
            "{receiver}: {labels:?}"
        );
        assert!(!labels.iter().any(|label| label == "ForeignKeyЯ"));
        let module = owner(MANAGER);
        session
            .apply_module_overlay(
                &module,
                session.revision(),
                1,
                &manager.replace("SavedKeyЯ", "CurrentKeyЯ"),
                &InputRequest::default(),
            )
            .unwrap();
        session
            .invalidate_saved_inputs(std::slice::from_ref(&module), session.revision())
            .unwrap();
        let labels = return_keys(&mut session, &body);
        assert!(
            labels.iter().any(|label| label == "CurrentKeyЯ"),
            "{labels:?}"
        );
        assert!(!labels.iter().any(|label| label == "SavedKeyЯ"));
        session
            .close_module_overlay(&module, session.revision(), &InputRequest::default())
            .unwrap();
        assert!(
            return_keys(&mut session, &body)
                .iter()
                .any(|label| label == "SavedKeyЯ")
        );
        assert!(matches!(
            session.selected_root.modules[&owner("SettingsStorages/OtherЯ/ManagerModule.bsl")]
                .model
                .state,
            ModuleState::Pending
        ));
        assert_eq!(
            fs::read_to_string(session.root().join(MANAGER)).unwrap(),
            manager
        );
        assert_eq!(
            fs::read_to_string(session.root().join(CALLER)).unwrap(),
            body
        );
    }
}

/// Self and mutually referring aliases terminate without borrowing an unrelated manager from the same body.
#[test]
fn settings_storage_self_and_cyclic_alias_preparation_keeps_unrelated_manager_cold() {
    for assignments in ["A=A;", "A=B;\n B=A;"] {
        let body = format!(
            "Procedure Run()\n {assignments}\n Other=SettingsStorages.CurrentЯ.Load(\"key\");\n Result=A.SavedKeyЯ;\nEndProcedure\n"
        );
        let (_directory, mut session) = project(&body);
        assert!(return_keys(&mut session, &body).is_empty());
        assert!(matches!(
            session.selected_root.modules[&owner(MANAGER)].model.state,
            ModuleState::Pending
        ));
        assert_eq!(
            fs::read_to_string(session.root().join(CALLER)).unwrap(),
            body
        );
    }
}

/// Removing a current exported method is an error on a proven configured manager, while its native methods remain valid.
#[test]
fn settings_storage_removed_current_export_is_an_unresolved_call() {
    let body = "Procedure Run()\n Manager=SettingsStorages.CurrentЯ;\n Manager.SavedMethod();\n Value=Manager.Load(\"key\");\nEndProcedure\n";
    let (_directory, mut session) = project(body);
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let missing = |session: &AnalysisSession| {
        session
            .module_diagnostics(
                &owner(CALLER),
                session.revision(),
                &crate::DiagnosticsConfig::default(),
                &QueryCancellation::default(),
            )
            .unwrap()
            .value
            .into_iter()
            .filter(|finding| finding.code == crate::DiagnosticCode::UnresolvedMethodCall)
            .collect::<Vec<_>>()
    };
    assert!(missing(&session).is_empty());
    let module = owner(MANAGER);
    session
        .apply_module_overlay(&module, session.revision(), 1, fixture::CURRENT, &request)
        .unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&module), session.revision())
        .unwrap();
    let findings = missing(&session);
    assert_eq!(findings.len(), 1, "{findings:?}");
    session
        .close_module_overlay(&module, session.revision(), &request)
        .unwrap();
    assert!(missing(&session).is_empty());
    assert_eq!(
        fs::read_to_string(session.root().join(MANAGER)).unwrap(),
        fixture::SAVED
    );
    assert_eq!(
        fs::read_to_string(session.root().join(CALLER)).unwrap(),
        body
    );
}

/// Designer rejects the predefined self spelling in this module; a qualified manager call remains valid.
#[test]
fn settings_storage_manager_has_no_implicit_this_object() {
    for name in ["ThisObject", "ЭтотОбъект"] {
        let manager = format!(
            "Procedure Run()\n Value={name}.Load(\"key\");\n Control=SettingsStorages.CurrentЯ.Load(\"key\");\nEndProcedure\n"
        );
        let (_directory, mut session) =
            project_with_manager("Procedure Caller()\nEndProcedure\n", &manager);
        let request = InputRequest::default();
        session
            .load_semantic_inputs(session.revision(), &request)
            .unwrap();
        let config = crate::DiagnosticsConfig {
            enabled: vec![crate::DiagnosticCode::UnresolvedName],
            ..crate::DiagnosticsConfig::default()
        };
        let diagnostics = session
            .module_diagnostics(
                &owner(MANAGER),
                session.revision(),
                &config,
                &QueryCancellation::default(),
            )
            .unwrap()
            .value;
        let missing = diagnostics
            .iter()
            .filter(|finding| finding.code == crate::DiagnosticCode::UnresolvedName)
            .count();
        assert_eq!(missing, 1, "{diagnostics:?}");
        assert!(
            !diagnostics
                .iter()
                .any(|finding| finding.code == crate::DiagnosticCode::UnresolvedMethodCall),
            "{diagnostics:?}"
        );
        let offset = u32::try_from(manager.find(name).unwrap() + name.len() + 1).unwrap();
        session
            .prepare_completion_metadata(
                &owner(MANAGER),
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        let items = session
            .module_completions(
                &owner(MANAGER),
                session.revision(),
                offset,
                Locale::En,
                &QueryCancellation::default(),
            )
            .unwrap()
            .value;
        assert!(
            !items.iter().any(|item| item
                .filter_text
                .as_ref()
                .is_some_and(|text| text.contains("Load"))),
            "{items:?}"
        );
        assert_eq!(
            fs::read_to_string(session.root().join(MANAGER)).unwrap(),
            manager
        );
    }
}

/// The same Designer-proven absence applies to a canonical catalog manager, while its qualified API stays usable.
#[test]
fn catalog_manager_has_no_implicit_this_object_in_current_source() {
    for name in ["ThisObject", "ЭтотОбъект"] {
        let manager = format!(
            "Procedure Run()\n Value={name};\n Control=Catalogs.Unrelated.CreateItem();\nEndProcedure\n"
        );
        let (directory, earlier) = project("Procedure Caller()\nEndProcedure\n");
        let native = directory.0.join("native");
        let source = earlier
            .root()
            .parent()
            .unwrap()
            .join("src-with-catalog-manager");
        drop(earlier);
        fs::create_dir_all(native.join("Catalogs/Unrelated/Ext")).unwrap();
        fs::write(
            native.join("Catalogs/Unrelated/Ext/ManagerModule.bsl"),
            &manager,
        )
        .unwrap();
        eska::project_core::source_format::import_designer_queries(&native, &source).unwrap();
        let mut session = AnalysisSession::open(&source).unwrap();
        session
            .load_semantic_inputs(session.revision(), &InputRequest::default())
            .unwrap();
        let config = crate::DiagnosticsConfig {
            enabled: vec![crate::DiagnosticCode::UnresolvedName],
            ..crate::DiagnosticsConfig::default()
        };
        let diagnostics = session
            .module_diagnostics(
                &owner("Catalogs/Unrelated/ManagerModule.bsl"),
                session.revision(),
                &config,
                &QueryCancellation::default(),
            )
            .unwrap()
            .value;
        let missing = diagnostics
            .iter()
            .filter(|finding| finding.code == crate::DiagnosticCode::UnresolvedName)
            .count();
        assert_eq!(missing, 1, "{diagnostics:?}");
        assert!(
            !diagnostics
                .iter()
                .any(|finding| finding.code == crate::DiagnosticCode::UnresolvedMethodCall),
            "{diagnostics:?}"
        );
        assert_eq!(
            fs::read_to_string(source.join("Catalogs/Unrelated/ManagerModule.bsl")).unwrap(),
            manager
        );
    }
}
