//! Current configured values share the ordinary guarded source lifecycle and real BSL typed receiver.

#[path = "../../tests/support/session_parameter_project.rs"]
mod fixture;

use super::*;
use crate::{
    Locale, QueryCancellation,
    session::tests::{owner, support::TestDirectory},
};

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const VALUE: &str = "SessionParameters/CurrentUser/object.yaml";

/// Every source tree belongs to this one test and retains its native comparison descriptor.
fn project(body: &str) -> (TestDirectory, std::path::PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    fixture::add_parameters(&root, &directory.0.join("native"));
    let source = root.join("src-with-parameters");
    let session =
        AnalysisSession::with_database(&source, RootDatabaseImpl::new_with_salsa_events()).unwrap();
    (directory, source, session)
}

/// Read the actual tracked native value rather than a second adapter oracle.
fn value(session: &AnalysisSession) -> Option<Arc<bsl_metadata::SessionParameter>> {
    match session.resolve(&owner(VALUE), session.revision()).unwrap() {
        Some(CanonicalMetadata::SessionParameter(value)) => Some(value),
        _ => None,
    }
}

/// The container's readonly status must not turn configured mutable values into readonly fields.
#[test]
fn session_parameter_cold_completion_is_typed_bilingual_and_family_bounded() {
    for access in [
        "SessionParameters.",
        "ПараметрыСеанса.",
        "sEsSiOnPaRaMeTeRs.",
        "Alias.",
    ] {
        let alias = if access == "Alias." {
            " Alias = SessionParameters;\n"
        } else {
            ""
        };
        let body = format!("Procedure Run()\n{alias} Value = {access}CurrentUser;\nEndProcedure\n");
        let (_directory, _source, mut session) = project(&body);
        let request = InputRequest::default();
        let module = owner(MODULE);
        session
            .load_module_saved(&module, session.revision(), &request)
            .unwrap();
        let offset = u32::try_from(body.find("CurrentUser").unwrap()).unwrap();
        session
            .prepare_completion_metadata(
                &module,
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        for locale in [Locale::Ru, Locale::En] {
            let result = session
                .module_completions(
                    &module,
                    session.revision(),
                    offset,
                    locale,
                    &QueryCancellation::default(),
                )
                .unwrap();
            let field = result
                .value
                .iter()
                .find(|item| item.label == "CurrentUser")
                .expect("configured field");
            assert!(!field.detail.as_ref().unwrap().contains("Read-only"));
            assert!(!field.detail.as_ref().unwrap().contains("Только чтение"));
        }
        assert!(matches!(
            value(&session).unwrap().attribute_type(),
            bsl_metadata::AttributeType::String { length: Some(20) }
        ));
        assert!(matches!(
            session
                .owner(&owner("Catalogs/Items/object.yaml"))
                .unwrap()
                .state,
            ModelState::Pending
        ));
        assert_eq!(
            session
                .database
                .salsa_family_churn("load_configuration")
                .unwrap()
                .0,
            0
        );
        assert_eq!(
            session
                .database
                .salsa_family_churn("parse_session_parameter")
                .unwrap()
                .0,
            0
        );
    }
}

/// Invalid/editor/deleted current input withdraws the old type; saved close restores only real disk bytes.
#[test]
fn session_parameter_overlay_watch_close_and_recovery_preserve_current_authority() {
    let (_directory, source, mut session) = project("// Current body.\n");
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let path = owner(VALUE);
    let neighbor = owner("SessionParameters/Neighbor/object.yaml");
    let saved = value(&session).unwrap();
    let untouched = session.resolve(&neighbor, session.revision()).unwrap();
    let text = std::fs::read_to_string(source.join(path.as_path())).unwrap();
    let current = text.replace("x:string", "x:boolean");
    session
        .apply_overlay(&path, session.revision(), 1, &current, &request)
        .unwrap();
    assert_eq!(
        value(&session).unwrap().attribute_type(),
        &bsl_metadata::AttributeType::Boolean
    );
    let before_watch = session.revision();
    assert!(
        session
            .invalidate_saved_inputs(std::slice::from_ref(&path), before_watch)
            .unwrap()
            .is_empty()
    );
    assert_eq!(session.revision(), before_watch);
    let invalid = current.replace("x:boolean", "c:FutureType");
    session
        .apply_overlay(&path, session.revision(), 2, &invalid, &request)
        .unwrap();
    assert!(value(&session).is_none());
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Invalid { .. }
    ));
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(value(&session).is_none());
    assert_eq!(
        session.resolve(&neighbor, session.revision()).unwrap(),
        untouched
    );
    session
        .close_overlay(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(*value(&session).unwrap(), *saved);
    assert_eq!(
        std::fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
    std::fs::remove_file(source.join(path.as_path())).unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&path), session.revision())
        .unwrap();
    assert!(value(&session).is_none());
    session
        .load_saved(&path, session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Unavailable(_)
    ));
    assert!(value(&session).is_none());
    std::fs::write(source.join(path.as_path()), &text).unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&path), session.revision())
        .unwrap();
    session
        .load_saved(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(*value(&session).unwrap(), *saved);
}

/// Accepted cancellation and quota failures retract the native type, while stale updates publish nothing.
#[test]
fn session_parameter_failures_retract_types_and_stale_revision_keeps_current_value() {
    use eska::analysis_core::{Cancellation, InputFailure};
    let (_directory, source, mut session) = project("// Current body.\n");
    let path = owner(VALUE);
    let saved = std::fs::read_to_string(source.join(path.as_path())).unwrap();
    let old = session.revision();
    session
        .load_saved(&path, old, &InputRequest::default())
        .unwrap();
    let current = session.summary();
    assert!(
        session
            .apply_overlay(&path, old, 1, &saved, &InputRequest::default())
            .is_err()
    );
    assert_eq!(session.summary(), current);
    for cancelled in [true, false] {
        let cancellation = Cancellation::default();
        if cancelled {
            cancellation.cancel();
        }
        let request = InputRequest {
            cancellation,
            max_document_bytes: 1,
            ..InputRequest::default()
        };
        session
            .load_saved(&path, session.revision(), &request)
            .unwrap();
        assert!(value(&session).is_none());
        assert!(matches!(
            session.owner(&path).unwrap().state,
            ModelState::Unavailable(
                InputFailure::Cancelled | InputFailure::DocumentTooLarge { limit: 1 }
            )
        ));
        session
            .load_saved(&path, session.revision(), &InputRequest::default())
            .unwrap();
        assert!(value(&session).is_some());
    }
}

/// Even a known same-named configured value cannot supply a type to an ordinary query parameter.
#[test]
fn session_parameter_does_not_invent_query_parameter_binding() {
    let (_directory, _source, mut session) = project("// Current body.\n");
    let module = owner(MODULE);
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    session
        .load_module_saved(&module, session.revision(), &request)
        .unwrap();
    assert!(value(&session).is_some());
    let model = session
        .query_module(
            &module,
            session.revision(),
            &QueryCancellation::default(),
            |analysis, file| {
                Ok(analysis.query_text_model(
                    file,
                    "SELECT &CurrentUser AS Value",
                    &ide::QueryParameterTypes::default(),
                ))
            },
        )
        .unwrap()
        .value;
    assert!(model.syntax_complete);
    assert_eq!(model.parameters[0].name, "CurrentUser");
    let ide::QueryTextStatement::Select(select) = &model.statements[0] else {
        panic!("SELECT");
    };
    assert!(!select.output_types_complete);
    assert!(!select.branches[0].fields[0].type_complete);
    assert_eq!(format!("{:?}", select.branches[0].fields[0].ty), "Unknown");
}

/// Supporting this semantic family does not grant it a canonical model in a source-only session.
#[test]
fn session_parameter_local_syntax_keeps_source_only_admission() {
    let (_directory, source, _semantic) = project("// Current body.\n");
    let mut session = AnalysisSession::with_inputs(
        MetadataInputs::open(&source).unwrap(),
        RootDatabaseImpl::new_with_salsa_events(),
        SessionOptions::default(),
        AnalysisScope::LocalSyntax,
    )
    .unwrap();
    session
        .load_saved(&owner(VALUE), session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(
        session.owner(&owner(VALUE)).unwrap().state,
        ModelState::SourceOnly
    );
    assert!(value(&session).is_none());
    assert!(!session.summary().descriptor_models_complete);
    assert!(session.database.all_config_paths().is_empty());
}

/// Platform-only execution contexts also bound configured members, even though their values are mutable.
#[test]
fn session_parameter_unavailable_client_contexts_do_not_prepare_or_offer_members() {
    for context in ["ThinClient", "WebClient"] {
        let directory = TestDirectory::new();
        let body = format!(
            "#If {context} Then\nProcedure Run()\n Value = SessionParameters.CurrentUser;\nEndProcedure\n#EndIf\n"
        );
        let root = crate::test_project::project(&directory.0, &body, false);
        let native = directory.0.join("native");
        let common = native.join("CommonModules/Tools.xml");
        let text = std::fs::read_to_string(&common).unwrap().replace(
            "<Server>true</Server>",
            "<Server>false</Server><ClientManagedApplication>true</ClientManagedApplication>",
        );
        std::fs::write(common, text).unwrap();
        fixture::add_parameters(&root, &native);
        let mut session = AnalysisSession::open(&root.join("src-with-parameters")).unwrap();
        let module = owner(MODULE);
        let offset = u32::try_from(body.find("CurrentUser").unwrap()).unwrap();
        let request = InputRequest::default();
        session
            .load_module_saved(&module, session.revision(), &request)
            .unwrap();
        session
            .prepare_completion_metadata(
                &module,
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        assert!(
            value(&session).is_none(),
            "{context}: unavailable receiver must not cause family I/O"
        );
        assert!(
            !session
                .module_completions(
                    &module,
                    session.revision(),
                    offset,
                    Locale::En,
                    &QueryCancellation::default()
                )
                .unwrap()
                .value
                .iter()
                .any(|item| item.label == "CurrentUser"),
            "{context}"
        );
        session
            .load_semantic_inputs(session.revision(), &request)
            .unwrap();
        assert!(value(&session).is_some());
        assert!(
            !session
                .module_completions(
                    &module,
                    session.revision(),
                    offset,
                    Locale::En,
                    &QueryCancellation::default()
                )
                .unwrap()
                .value
                .iter()
                .any(|item| item.label == "CurrentUser"),
            "{context}: prepared values stay unavailable"
        );
    }
}

/// A local declaration or parameter named like the platform global cannot acquire configured fields.
#[test]
fn session_parameter_completion_respects_native_root_shadowing_and_static_methods() {
    for body in [
        "Procedure Run(SessionParameters)\n Value = SessionParameters.CurrentUser;\nEndProcedure\n",
        "Procedure Run()\n Var SessionParameters;\n Value = SessionParameters.CurrentUser;\nEndProcedure\n",
        "Var SessionParameters;\nProcedure Run()\n Value = SessionParameters.CurrentUser;\nEndProcedure\n",
    ] {
        let (_directory, _source, mut session) = project(body);
        let request = InputRequest::default();
        let module = owner(MODULE);
        session
            .load_module_saved(&module, session.revision(), &request)
            .unwrap();
        let offset = u32::try_from(body.find("CurrentUser").unwrap()).unwrap();
        session
            .prepare_completion_metadata(
                &module,
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        assert!(
            value(&session).is_none(),
            "shadowed name should not trigger family I/O"
        );
        assert!(
            !session
                .module_completions(
                    &module,
                    session.revision(),
                    offset,
                    Locale::En,
                    &QueryCancellation::default()
                )
                .unwrap()
                .value
                .iter()
                .any(|item| item.label == "CurrentUser")
        );
    }
    let body = "Procedure Run()\n SessionParameters.Clear();\nEndProcedure\n";
    let (_directory, _source, mut session) = project(body);
    let request = InputRequest::default();
    let module = owner(MODULE);
    session
        .load_module_saved(&module, session.revision(), &request)
        .unwrap();
    let offset = u32::try_from(body.find("Clear").unwrap()).unwrap();
    session
        .prepare_completion_metadata(
            &module,
            session.revision(),
            offset,
            &request,
            &QueryCancellation::default(),
        )
        .unwrap();
    let result = session
        .module_completions(
            &module,
            session.revision(),
            offset,
            Locale::En,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert!(
        result
            .value
            .iter()
            .any(|item| matches!(item.label.as_str(), "Clear" | "Очистить")),
        "{:?}",
        result.value
    );
}
