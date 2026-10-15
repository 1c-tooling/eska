//! Generated sequence methods and record dimensions consume the same current canonical declarations.

use super::*;
use crate::{
    Locale, QueryCancellation, QueryError,
    session::tests::{owner, support::TestDirectory},
};
use hir::DefDatabase;
use std::{fs, path::PathBuf};

const MODULE: &str = "CommonModules/Tools/Module.bsl";
const RECORD_SET: &str = "Sequences/Chain/RecordSetModule.bsl";
const SEQUENCE: &str = "Sequences/Chain/object.yaml";

/// All imported descriptors and bodies belong to this test's unique playground directory.
fn project(body: &str, record_set_body: &str) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    crate::test_sequence_project::add_sequence(
        &root,
        &directory.0.join("native"),
        record_set_body,
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let source = root.join("src-with-sequences");
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_module_saved(&owner(MODULE), session.revision(), &InputRequest::default())
        .unwrap();
    (directory, source, session)
}

/// Real cold discovery and native completion execute through the public shared preparation route.
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

/// Configured fields retain their native type hints rather than guessing from YAML text.
fn key_hint(session: &mut AnalysisSession, module: &str, body: &str) -> Result<String, QueryError> {
    Ok(completions(session, module, body, "Row.", Locale::En)?
        .into_iter()
        .find(|item| item.label == "KeyЯ")
        .and_then(|item| item.detail)
        .unwrap_or_default())
}

/// Both global spellings expose native manager methods and a record's current typed dimensions.
#[test]
fn sequence_cold_namespace_and_generated_records_are_native() {
    for access in ["Sequences.", "Последовательности."] {
        let body = format!("Procedure Run()\n Value={access};\nEndProcedure\n");
        let (_directory, _source, mut session) = project(&body, "");
        let items = completions(&mut session, MODULE, &body, access, Locale::En).unwrap();
        assert!(items.iter().any(|item| item.label == "Chain"), "{items:?}");
    }
    let body = "\u{feff}// Я😀\r\nProcedure Run()\r\n Rows=Sequences.Chain.CreateRecordSet();\r\n Row=Rows.Add();\r\n Я=\"😀\"; Value=Row.KeyЯ;\r\nEndProcedure\r\n";
    let (_directory, source, mut session) = project(body, "");
    assert!(
        key_hint(&mut session, MODULE, body)
            .unwrap()
            .contains("String")
    );
    let methods = completions(&mut session, MODULE, body, "Sequences.Chain.", Locale::Ru).unwrap();
    assert!(
        methods
            .iter()
            .any(|item| item.label == "СоздатьНаборЗаписей"),
        "{methods:?}"
    );
    assert!(!methods.iter().any(|item| item.label.contains("&lt;")));
    assert!(matches!(
        session.selected_root.owners[&owner("Catalogs/Items/object.yaml")]
            .model
            .state,
        ModelState::Pending
    ));
    assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), body);
}

/// Dirty type/name changes, invalid refusal, watcher suppression and close never reuse an old sequence model.
#[test]
fn sequence_record_dimension_tracks_current_source_without_changing_bsl() {
    let body = "Procedure Run()\n Rows=Sequences.Chain.CreateRecordSet();\n Row=Rows.Add();\n Value=Row.KeyЯ;\nEndProcedure\n";
    let (_directory, source, mut session) = project(body, "");
    assert!(
        key_hint(&mut session, MODULE, body)
            .unwrap()
            .contains("String")
    );
    let saved = fs::read_to_string(source.join(SEQUENCE)).unwrap();
    let changed = saved.replace("xs:string", "xs:boolean");
    assert_ne!(changed, saved);
    session
        .apply_overlay(
            &owner(SEQUENCE),
            session.revision(),
            1,
            &changed,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        key_hint(&mut session, MODULE, body)
            .unwrap()
            .contains("Boolean")
    );
    assert!(
        session
            .invalidate_saved_inputs(&[owner(SEQUENCE)], session.revision())
            .unwrap()
            .is_empty()
    );
    let renamed = changed.replace("KeyЯ", "RenamedЯ");
    session
        .apply_overlay(
            &owner(SEQUENCE),
            session.revision(),
            2,
            &renamed,
            &InputRequest::default(),
        )
        .unwrap();
    let items = completions(&mut session, MODULE, body, "Row.", Locale::En).unwrap();
    assert!(
        items.iter().any(|item| item.label == "RenamedЯ")
            && !items.iter().any(|item| item.label == "KeyЯ")
    );
    session
        .apply_overlay(
            &owner(SEQUENCE),
            session.revision(),
            3,
            "broken: [",
            &InputRequest::default(),
        )
        .unwrap();
    assert!(matches!(
        key_hint(&mut session, MODULE, body),
        Err(QueryError::Request(
            SessionRequestError::CompositionInputIncomplete(_)
        ))
    ));
    assert!(
        session
            .module_syntax_diagnostics(
                &owner(MODULE),
                session.revision(),
                &crate::DiagnosticsConfig::default(),
                &QueryCancellation::default()
            )
            .is_ok()
    );
    session
        .apply_overlay(
            &owner(SEQUENCE),
            session.revision(),
            4,
            &changed,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        key_hint(&mut session, MODULE, body)
            .unwrap()
            .contains("Boolean")
    );
    session
        .close_overlay(
            &owner(SEQUENCE),
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        key_hint(&mut session, MODULE, body)
            .unwrap()
            .contains("String")
    );
    assert_eq!(fs::read_to_string(source.join(SEQUENCE)).unwrap(), saved);
    assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), body);
}

/// A sequence record-set module and a For Each record keep the same native generated record identity.
#[test]
fn sequence_record_set_this_object_and_iteration_keep_dimensions() {
    let body = "Procedure BeforeWrite(Cancel,Replace)\n Rows=ThisObject;\n For Each Row In Rows Do\n Value=Row.KeyЯ;\n EndDo;\nEndProcedure\n";
    let (_directory, _source, mut session) = project("", body);
    session
        .load_module_saved(
            &owner(RECORD_SET),
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        key_hint(&mut session, RECORD_SET, body)
            .unwrap()
            .contains("String")
    );
    let file = session.selected_root.modules[&owner(RECORD_SET)].file;
    let metadata = session.database.module_metadata(hir::ModuleId::new(file));
    assert_eq!(metadata.sequence.as_ref().unwrap().name(), "Chain");
    assert!(metadata.register.is_none() && metadata.mdo.is_none());
}

/// Native diagnostics consume the configured record field rather than treating a generated sequence as unknown.
#[test]
fn sequence_record_dimension_types_reach_native_diagnostics() {
    let body = "Procedure Run()\n Rows=Sequences.Chain.CreateRecordSet();\n Row=Rows.Add();\n Value=Month(Row.KeyЯ);\n Missing=Row.Absent;\nEndProcedure\n";
    let (_directory, source, mut session) = project(body, "");
    let path = source.join(SEQUENCE);
    let saved = fs::read_to_string(&path)
        .unwrap()
        .replace("xs:string", "xs:dateTime");
    fs::write(&path, &saved).unwrap();
    session
        .load_semantic_inputs(session.revision(), &InputRequest::default())
        .unwrap();
    let findings = |session: &AnalysisSession| {
        session
            .module_diagnostics(
                &owner(MODULE),
                session.revision(),
                &crate::DiagnosticsConfig::default(),
                &QueryCancellation::default(),
            )
            .unwrap()
            .value
    };
    let first = findings(&session);
    assert!(
        first
            .iter()
            .any(|finding| finding.code == crate::DiagnosticCode::UnresolvedField),
        "{first:?}"
    );
    assert!(
        !first
            .iter()
            .any(|finding| finding.code == crate::DiagnosticCode::TypeMismatch),
        "{first:?}"
    );
    let saved = fs::read_to_string(source.join(SEQUENCE)).unwrap();
    let changed = saved.replace("xs:dateTime", "xs:boolean");
    session
        .apply_overlay(
            &owner(SEQUENCE),
            session.revision(),
            1,
            &changed,
            &InputRequest::default(),
        )
        .unwrap();
    let current = findings(&session);
    assert!(
        current
            .iter()
            .any(|finding| finding.code == crate::DiagnosticCode::TypeMismatch),
        "{current:?}"
    );
    session
        .close_overlay(
            &owner(SEQUENCE),
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        !findings(&session)
            .iter()
            .any(|finding| finding.code == crate::DiagnosticCode::TypeMismatch)
    );
}

/// A same-spelled local and a platform use in another body cannot authorize reading sequence payloads.
#[test]
fn sequence_shadowed_container_and_other_body_stay_cold() {
    let body = "Procedure Run()\n Sequences=New Structure;\n Value=Sequences.Chain;\nEndProcedure\nProcedure Other()\n Value=Последовательности.Chain.CreateRecordSet();\nEndProcedure\n";
    let (_directory, _source, mut session) = project(body, "");
    let items = completions(&mut session, MODULE, body, "Sequences.", Locale::En).unwrap();
    assert!(!items.iter().any(|item| item.label == "Chain"));
    assert!(matches!(
        session.selected_root.owners[&owner(SEQUENCE)].model.state,
        ModelState::Pending
    ));
}

/// Hover and zero-argument signature help use native platform method IDs and configured return types.
#[test]
fn sequence_manager_signature_and_hover_keep_native_identity() {
    let body = "Procedure Run()\n Rows=Sequences.Chain.CreateRecordSet();\nEndProcedure\n";
    let (_directory, _source, mut session) = project(body, "");
    let offset =
        u32::try_from(body.find("CreateRecordSet(").unwrap() + "CreateRecordSet(".len()).unwrap();
    session
        .prepare_signature_help_metadata(
            &owner(MODULE),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    let signature = session
        .module_signature_help(
            &owner(MODULE),
            session.revision(),
            offset,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .unwrap();
    assert_eq!(signature.signatures[0].parameters.len(), 0);
    assert!(
        !signature.signatures[0].label.contains("&lt;"),
        "{signature:?}"
    );
    let offset = u32::try_from(body.find("CreateRecordSet").unwrap()).unwrap();
    session
        .prepare_symbol_hover_metadata(
            &owner(MODULE),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    let card = session
        .module_symbol_hover(
            &owner(MODULE),
            session.revision(),
            offset,
            Locale::En,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .unwrap();
    assert_eq!(card.kind, crate::HoverKind::PlatformFunction);
    assert!(
        card.return_type
            .as_deref()
            .is_some_and(|value| value.contains("Chain")),
        "{card:?}"
    );
}

/// Build one explicit base/selected/hidden topology without activating unrelated payloads.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
fn composition(body: &str) -> (TestDirectory, PathBuf, PathBuf, AnalysisSession) {
    use eska::project_core::source_format::SourceReader;
    let directory = TestDirectory::new();
    let create = |name, extension, uuid, extra| {
        let own = directory.0.join(name);
        let root = crate::test_project::project(&own, body, false);
        crate::test_sequence_project::add_sequence(
            &root,
            &own.join("native"),
            "",
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
    let settings_file = selected.join("bsl-analyzer.toml");
    fs::write(&settings_file,"[source]\nroot='../../base/project/src-with-sequences'\nextensions=[{name='Selected',path='src-with-sequences'},{name='Hidden',path='../../hidden/project/src-with-sequences'}]\n").unwrap();
    let settings = project_model::ProjectConfig::load_from_file(&settings_file).unwrap();
    let readers = [&base, &selected, &hidden]
        .map(|root| SourceReader::open(&root.join("src-with-sequences")).unwrap())
        .into_iter()
        .collect();
    let topology = crate::composition::admit(
        &selected,
        &settings,
        readers,
        &selected.join("src-with-sequences"),
    )
    .unwrap();
    let session =
        AnalysisSession::from_composition(topology, crate::SessionOptions::default()).unwrap();
    (directory, base, selected, session)
}

/// Explicit composition admits the base and selected CFE while preserving an unread hidden sibling.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
#[test]
fn sequence_cfe_inherits_current_dimensions_and_refuses_wrong_original() {
    use crate::composition::SourceKey;
    let body = "Procedure Run()\n Rows=Sequences.Chain.CreateRecordSet();\n Row=Rows.Add();\n Value=Row.KeyЯ;\nEndProcedure\n";
    let (_directory, base, selected, mut session) = composition(body);
    session
        .load_module_saved(&owner(MODULE), session.revision(), &InputRequest::default())
        .unwrap();
    assert!(
        key_hint(&mut session, MODULE, body)
            .unwrap()
            .contains("String")
    );
    let hidden_id = session.root_ids().find(|root| root.index() == 2).unwrap();
    assert!(matches!(
        session.root_inputs(hidden_id).unwrap().owners[&owner(SEQUENCE)]
            .model
            .state,
        ModelState::Pending
    ));
    let base_id = session.root_ids().find(|root| root.index() == 0).unwrap();
    let key = SourceKey {
        root: base_id,
        source: owner(SEQUENCE),
    };
    let saved = fs::read_to_string(base.join("src-with-sequences").join(SEQUENCE)).unwrap();
    let changed = saved.replace("xs:string", "xs:boolean");
    session
        .apply_overlay_at(
            &key,
            session.revision(),
            1,
            &changed,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        key_hint(&mut session, MODULE, body)
            .unwrap()
            .contains("Boolean")
    );
    let own_saved = fs::read_to_string(selected.join("src-with-sequences").join(SEQUENCE)).unwrap();
    let wrong = own_saved.replace(
        "70000000-0000-4000-8000-000000000001",
        "70000000-0000-4000-8000-000000000099",
    );
    session
        .apply_overlay(
            &owner(SEQUENCE),
            session.revision(),
            1,
            &wrong,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(key_hint(&mut session, MODULE, body).is_err());
    session
        .close_overlay(
            &owner(SEQUENCE),
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        key_hint(&mut session, MODULE, body)
            .unwrap()
            .contains("Boolean")
    );
    session
        .close_overlay_at(&key, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(
        key_hint(&mut session, MODULE, body)
            .unwrap()
            .contains("String")
    );
    assert_eq!(
        fs::read_to_string(base.join("src-with-sequences").join(SEQUENCE)).unwrap(),
        saved
    );
    assert_eq!(
        fs::read_to_string(selected.join("src-with-sequences").join(SEQUENCE)).unwrap(),
        own_saved
    );
}
