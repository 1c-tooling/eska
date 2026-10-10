//! Call signatures use current kernel binding and validated byte spans within generated labels.

use eska::analysis_core::InputRequest;

use super::*;
use crate::session::tests::fixture;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "\u{feff}// Я😀\r\n// Назначение вызова.\r\nФункция Вызов(Вызов, Другой = \"😀\") Экспорт\r\nВозврат Вызов;\r\nКонецФункции\r\nПроцедура Проверка()\r\nВызов(1, );\r\nКонецПроцедуры\r\n";

/// A parameter named like its method highlights the parameter occurrence, not the header.
#[test]
fn current_call_signature_tracks_selection_defaults_docs_and_editor_changes() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &InputRequest::default())
        .unwrap();
    let control = QueryCancellation::default();
    let at = u32::try_from(BODY.find("1, ").unwrap() + 3).unwrap();
    let first = session
        .module_signature_help(&path, session.revision(), at, &control)
        .unwrap();
    let help = first.value.as_ref().unwrap();
    assert_eq!(help.active_parameter, Some(1));
    let signature = &help.signatures[0];
    assert!(
        signature
            .documentation
            .as_ref()
            .unwrap()
            .contains("Назначение вызова")
    );
    assert_eq!(signature.parameters.len(), 2);
    let parameter = &signature.parameters[0];
    assert_eq!(parameter.label, "Вызов");
    assert_ne!(
        parameter.byte_range.start as usize,
        signature.label.find("Вызов").unwrap()
    );
    for parameter in &signature.parameters {
        assert_eq!(
            &signature.label
                [parameter.byte_range.start as usize..parameter.byte_range.end as usize],
            parameter.label
        );
    }
    assert!(signature.parameters[1].label.contains('😀'));
    let previous = session.revision();
    let changed = BODY.replace("Другой =", "Актуальный =");
    session
        .apply_module_overlay(&path, previous, 2, &changed, &InputRequest::default())
        .unwrap();
    let at = u32::try_from(changed.find("1, ").unwrap() + 3).unwrap();
    let current = session
        .module_signature_help(&path, session.revision(), at, &control)
        .unwrap();
    assert!(
        current.value.unwrap().signatures[0].parameters[1]
            .label
            .contains("Актуальный")
    );
    assert!(matches!(
        session.module_signature_help(&path, previous, at, &control),
        Err(QueryError::Request(_))
    ));
    assert_eq!(session.resources().metadata_source_bytes, 0);
}

/// Native projection failures never guess a matching header or emit invalid active indices.
#[test]
fn inconsistent_display_spans_and_active_indices_are_explicit_errors() {
    let signature = ide::SignatureInformation {
        signature: "Функция Вызов(Вызов)".into(),
        doc: None,
        parameters: vec![ide::ParameterInfo {
            label: "Вызов".into(),
            documentation: None,
        }],
    };
    let control = QueryCancellation::default();
    let help = ide::SignatureHelp {
        signatures: vec![signature.clone()],
        active_signature: Some(0),
        active_parameter: Some(0),
    };
    assert!(project(help.clone(), &control).is_ok());
    let mut bad = help.clone();
    bad.active_signature = Some(1);
    assert!(matches!(
        project(bad, &control),
        Err(QueryError::InconsistentSignatureHelp { signature: 1, .. })
    ));
    let mut bad = help;
    bad.active_parameter = Some(1);
    assert!(matches!(
        project(bad, &control),
        Err(QueryError::InconsistentSignatureHelp {
            parameter: Some(1),
            ..
        })
    ));
    for label in ["Вызов", "Функция Вызов(Другой)", "Функция Вызов(Вызов, )"]
    {
        let mut bad = signature.clone();
        bad.signature = label.into();
        assert!(matches!(
            project_signature(0, bad, &control),
            Err(QueryError::InconsistentSignatureHelp { .. })
        ));
    }
    control.cancel();
    assert!(matches!(
        project_signature(0, signature, &control),
        Err(QueryError::Cancelled)
    ));
}

/// Exact UTF-8 offset, current input and cancellation guards are shared with all module queries.
#[test]
fn call_signature_preconditions_preserve_source_and_clock() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let control = QueryCancellation::default();
    assert!(matches!(
        session.module_signature_help(&path, session.revision(), 0, &control),
        Err(QueryError::ModuleUnavailable { .. })
    ));
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &InputRequest::default())
        .unwrap();
    let revision = session.revision();
    let resources = session.resources();
    for at in [1, u32::MAX] {
        assert!(matches!(
            session.module_signature_help(&path, revision, at, &control),
            Err(QueryError::InvalidOffset { .. })
        ));
    }
    assert!(
        session
            .module_signature_help(&path, revision, 0, &control)
            .unwrap()
            .value
            .is_none()
    );
    control.cancel();
    assert!(matches!(
        session.module_signature_help(&path, revision, 0, &control),
        Err(QueryError::Cancelled)
    ));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
}
