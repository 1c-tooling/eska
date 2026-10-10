//! Local rename policy, grammar and exact native scopes are checked without applying a disk edit.

use super::*;
use eska::{analysis_core::InputRequest, project_core::source_format::accept_source};

use crate::test_project as support;

/// A complete tiny composition needs no supplier payload and uses the standard directory guard.
fn fixture() -> (
    crate::session::tests::support::TestDirectory,
    std::path::PathBuf,
) {
    let directory = crate::session::tests::support::TestDirectory::new();
    let root = support::project(&directory.0, "// Saved module.\n", false);
    (directory, root.join("src"))
}

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "\u{feff}Процедура Пример(Аргумент)\r\nПерем Локальная;\r\nЛокальная=Аргумент;\r\nСообщить(Локальная);\r\nНеявная=Локальная;\r\nСообщить(Неявная);\r\nСтрока=\"Аргумент\";\r\nКонецПроцедуры\r\nПроцедура Другой(Аргумент)\r\nСообщить(Аргумент);\r\nКонецПроцедуры\r\n";

/// Edit previews include only the selected local binding and preserve unrelated module bytes.
#[test]
fn local_plans_cover_parameters_explicit_and_implicit_locals_without_mutation() {
    let (directory, root) = fixture();
    let reference = accept_source(&root, &directory.0.join("accepted"), &[]).unwrap();
    let base = AcceptedBase::open(&directory.0.join("accepted"), &reference).unwrap();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    session
        .apply_module_overlay(&path, session.revision(), 7, BODY, &InputRequest::default())
        .unwrap();
    let revision = session.revision();
    let resources = session.resources();
    let control = QueryCancellation::default();
    for (name, count) in [("Аргумент", 2), ("Локальная", 4), ("Неявная", 2)]
    {
        let at = u32::try_from(BODY.find(name).unwrap()).unwrap();
        let input = session
            .module_local_rename_input(&path, revision, at, &control)
            .unwrap()
            .value
            .unwrap();
        let plan = input
            .rename(&session, &base, "АктуальноеИмя", &control)
            .unwrap();
        assert_eq!(plan.occurrences.len(), count);
        assert_eq!(plan.target.document_version, 7);
        assert_eq!(plan.target.permission.baseline, reference.sha256);
        let mut updated = BODY.to_owned();
        for hit in plan.occurrences.iter().rev() {
            assert_eq!(&BODY[hit.start_byte as usize..hit.end_byte as usize], name);
            updated.replace_range(
                hit.start_byte as usize..hit.end_byte as usize,
                &plan.new_name,
            );
        }
        assert!(!parser::parse(&updated).has_errors());
        assert!(updated.contains("Процедура Другой(Аргумент)\r\nСообщить(Аргумент)"));
        assert!(updated.contains("Строка=\"Аргумент\""));
        assert!(updated.starts_with('\u{feff}') && updated.contains("\r\n"));
    }
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
}

/// The native parser rejects injection and reserved names rather than a separately maintained lexer.
#[test]
fn replacement_name_uses_native_declaration_grammar() {
    for parameter in [true, false] {
        for name in ["НовоеИмя", "new_name", "_Значение"] {
            validation::identifier(name, parameter).unwrap();
        }
        for name in [
            "",
            " ",
            "Если",
            "If",
            "Новый",
            "A B",
            "A.B",
            "A,B",
            "A=1",
            "A\n",
            "A//x",
            "\"A\"",
            "😀",
        ] {
            assert!(
                matches!(
                    validation::identifier(name, parameter),
                    Err(LocalRenameError::InvalidName)
                ),
                "{name:?} {parameter}"
            );
        }
    }
    assert!(matches!(
        validation::identifier(&"A".repeat(MAX_RENAME_NAME_BYTES + 1), true),
        Err(LocalRenameError::NameTooLarge { .. })
    ));
    validation::identifier(&"A".repeat(MAX_RENAME_NAME_BYTES), true).unwrap();
}

/// A command's physical YAML owner must not bypass the command's own accepted freeze.
#[test]
fn inline_command_module_uses_its_native_identity_for_edit_permission() {
    let directory = crate::session::tests::support::TestDirectory::new();
    let project = support::inline_command_project(&directory.0, BODY);
    let source = project.join("src-with-command");
    let command = eska::project_core::source_format::NativeUuid::parse(
        "40000000-0000-4000-8000-000000000001",
    )
    .unwrap();
    let reference = accept_source(
        &source,
        &directory.0.join("accepted-command"),
        std::slice::from_ref(&command),
    )
    .unwrap();
    let base = AcceptedBase::open(&directory.0.join("accepted-command"), &reference).unwrap();
    let mut session = AnalysisSession::open(&source).unwrap();
    let path = SourcePath::parse("Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl").unwrap();
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &InputRequest::default())
        .unwrap();
    let control = QueryCancellation::default();
    let input = session
        .module_local_rename_input(
            &path,
            session.revision(),
            u32::try_from(BODY.find("Аргумент").unwrap()).unwrap(),
            &control,
        )
        .unwrap()
        .value
        .unwrap();
    let error = input.prepare(&session, &base, &control).unwrap_err();
    let LocalRenameError::PermissionDenied(permission) = error else {
        panic!("{error:?}");
    };
    assert_eq!(permission.uuid, command);
    assert_eq!(
        permission.reason,
        eska::project_core::source_format::PermissionReason::EskaFrozen
    );
    // The export consumer must make the identical decision for the same physical command body.
    std::fs::write(
        source.join(path.as_path()),
        "// Denied working command.\r\n",
    )
    .unwrap();
    let output = directory.0.join("protected-command");
    let report =
        eska::project_core::source_format::export_protected(&source, &base, &output, false)
            .unwrap();
    assert!(report.diagnostics.iter().any(|item| item.path == path
        && item.uuid.as_ref() == Some(&command)
        && item.reason == eska::project_core::source_format::PermissionReason::EskaFrozen));
    assert_eq!(
        std::fs::read_to_string(
            report
                .xml
                .join("Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl")
        )
        .unwrap(),
        BODY
    );
}

/// Case-only edits preserve the binding; a different existing name is refused conservatively.
#[test]
fn rename_rejects_case_insensitive_capture_and_preserves_true_noops() {
    let (directory, root) = fixture();
    let reference = accept_source(&root, &directory.0.join("accepted"), &[]).unwrap();
    let base = AcceptedBase::open(&directory.0.join("accepted"), &reference).unwrap();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &InputRequest::default())
        .unwrap();
    let control = QueryCancellation::default();
    let at = u32::try_from(BODY.find("Аргумент").unwrap()).unwrap();
    for name in ["локальная", "ПРИМЕР", "Неявная"] {
        let input = session
            .module_local_rename_input(&path, session.revision(), at, &control)
            .unwrap()
            .value
            .unwrap();
        let error = input.rename(&session, &base, name, &control).unwrap_err();
        let LocalRenameError::NameConflict {
            start_byte,
            end_byte,
        } = error
        else {
            panic!("{error:?}");
        };
        assert!(
            hir::Name::new(&BODY[start_byte as usize..end_byte as usize])
                .eq_ignore_case(&hir::Name::new(name))
        );
    }
    for (name, count) in [("Аргумент", 0), ("АРГУМЕНТ", 2)] {
        let input = session
            .module_local_rename_input(&path, session.revision(), at, &control)
            .unwrap()
            .value
            .unwrap();
        assert_eq!(
            input
                .rename(&session, &base, name, &control)
                .unwrap()
                .occurrences
                .len(),
            count
        );
    }
}

/// A captured target cannot survive a source revision, close or stopped query.
#[test]
fn retained_rename_inputs_and_invalid_modules_are_refused_without_mutation() {
    let (directory, root) = fixture();
    let reference = accept_source(&root, &directory.0.join("accepted"), &[]).unwrap();
    let base = AcceptedBase::open(&directory.0.join("accepted"), &reference).unwrap();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let control = QueryCancellation::default();
    assert!(matches!(
        session.module_local_rename_input(&path, session.revision(), 0, &control),
        Err(LocalRenameError::Query(
            QueryError::ModuleUnavailable { .. }
        ))
    ));
    session
        .load_module_saved(&path, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(matches!(
        session.module_local_rename_input(&path, session.revision(), 0, &control),
        Err(LocalRenameError::DocumentNotOpen)
    ));
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &InputRequest::default())
        .unwrap();
    let at = u32::try_from(BODY.find("Аргумент").unwrap()).unwrap();
    let input = session
        .module_local_rename_input(&path, session.revision(), at, &control)
        .unwrap()
        .value
        .unwrap();
    session
        .apply_module_overlay(&path, session.revision(), 2, BODY, &InputRequest::default())
        .unwrap();
    assert!(matches!(
        input.rename(&session, &base, "НовыйПараметр", &control),
        Err(LocalRenameError::Query(QueryError::Request(_)))
    ));
    let input = session
        .module_local_rename_input(&path, session.revision(), at, &control)
        .unwrap()
        .value
        .unwrap();
    control.cancel();
    assert!(matches!(
        input.prepare(&session, &base, &control),
        Err(LocalRenameError::Query(QueryError::Cancelled))
    ));
    let control = QueryCancellation::default();
    for at in [1, u32::MAX] {
        assert!(matches!(
            session.module_local_rename_input(&path, session.revision(), at, &control),
            Err(LocalRenameError::Query(QueryError::InvalidOffset { .. }))
        ));
    }
    session
        .apply_module_overlay(
            &path,
            session.revision(),
            3,
            "Процедура Ошибка(",
            &InputRequest::default(),
        )
        .unwrap();
    let revision = session.revision();
    let resources = session.resources();
    assert!(matches!(
        session.module_local_rename_input(&path, revision, 0, &control),
        Err(LocalRenameError::SyntaxInvalid)
    ));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
}

/// File-local module symbols are excluded because their runtime/form entry points need wider plans.
#[test]
fn module_symbols_and_top_level_implicit_locals_do_not_become_method_local() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let text = "Перем Модульная;\nПроцедура Пример(Параметр)\nСообщить(Параметр);\nКонецПроцедуры\nВерхняя=1;\nСообщить(Верхняя);\n";
    session
        .apply_module_overlay(&path, session.revision(), 1, text, &InputRequest::default())
        .unwrap();
    let control = QueryCancellation::default();
    for name in ["Модульная", "Пример", "Верхняя", "Сообщить"] {
        let at = u32::try_from(text.find(name).unwrap()).unwrap();
        assert!(
            matches!(
                session.module_local_rename_input(&path, session.revision(), at, &control),
                Err(LocalRenameError::UnsupportedSymbol)
            ),
            "{name}"
        );
    }
    assert!(
        session
            .module_local_rename_input(&path, session.revision(), 0, &control)
            .unwrap()
            .value
            .is_none()
    );
}
