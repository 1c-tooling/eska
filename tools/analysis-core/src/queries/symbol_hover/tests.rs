//! Typed hover cards resolve current names/docs/return types without exposing native locations.

use eska::analysis_core::InputRequest;

use super::*;
use crate::session::tests::fixture;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "\u{feff}// Я😀\r\n// Текущая документация.\r\nФункция Значение() Экспорт\r\nВозврат 1;\r\nКонецФункции\r\nРезультат=Значение();\r\n";

/// One current BSL method has the same source span and different localized type labels.
#[test]
fn current_symbol_hover_follows_method_docs_return_type_and_identifier_boundaries() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let request = InputRequest::default();
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &request)
        .unwrap();
    let at = u32::try_from(BODY.rfind("Значение").unwrap()).unwrap();
    let end = at + u32::try_from("Значение".len()).unwrap();
    for (locale, expected) in [(Locale::Ru, "Число"), (Locale::En, "Number")] {
        let current = session
            .module_symbol_hover(
                &path,
                session.revision(),
                at,
                locale,
                &QueryCancellation::default(),
            )
            .unwrap();
        let info = current.value.as_ref().unwrap();
        assert_eq!(info.symbol, "Значение");
        assert_eq!(info.kind, HoverKind::Method);
        assert_eq!(info.start_byte, at);
        assert_eq!(info.end_byte, end);
        assert_eq!(info.return_type.as_deref(), Some(expected));
        assert!(info.declaration.as_ref().unwrap().contains("Значение"));
        assert!(
            info.documentation
                .as_ref()
                .unwrap()
                .contains("Текущая документация")
        );
        assert_eq!(
            session
                .module_symbol_hover(
                    &path,
                    session.revision(),
                    end,
                    locale,
                    &QueryCancellation::default()
                )
                .unwrap(),
            current
        );
        assert!(!current.metadata_models_complete);
    }
    let previous = session.revision();
    let changed = BODY.replace("Возврат 1;", "Возврат \"новый тип\";");
    session
        .apply_module_overlay(&path, previous, 2, &changed, &request)
        .unwrap();
    let at = u32::try_from(changed.rfind("Значение").unwrap()).unwrap();
    let current = session
        .module_symbol_hover(
            &path,
            session.revision(),
            at,
            Locale::En,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert_eq!(
        current.value.unwrap().return_type.as_deref(),
        Some("String")
    );
    assert!(matches!(
        session.module_symbol_hover(
            &path,
            previous,
            at,
            Locale::En,
            &QueryCancellation::default()
        ),
        Err(QueryError::Request(_))
    ));
    assert_eq!(session.resources().metadata_source_bytes, 0);
}

/// Explicit and implicit local/parameter cards do not forward native prose as a code declaration.
#[test]
fn parameter_local_and_module_variable_cards_keep_typed_kinds_and_current_cursor_ranges() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let body = "Перем Состояние Экспорт;\r\nПроцедура Вызов(Параметр)\r\nПерем Локальная;\r\nЛокальная=1;\r\nРезультат=Локальная;\r\nРезультат=Параметр;\r\nРезультат=Состояние;\r\nКонецПроцедуры\r\n";
    session
        .apply_module_overlay(&path, session.revision(), 1, body, &InputRequest::default())
        .unwrap();
    for (name, expected) in [
        ("Локальная", HoverKind::LocalVariable),
        ("Параметр", HoverKind::Parameter),
        ("Состояние", HoverKind::ModuleVariable),
        ("Результат", HoverKind::LocalVariable),
    ] {
        let at = u32::try_from(body.rfind(name).unwrap()).unwrap();
        let info = session
            .module_symbol_hover(
                &path,
                session.revision(),
                at,
                Locale::En,
                &QueryCancellation::default(),
            )
            .unwrap()
            .value
            .unwrap_or_else(|| panic!("Missing card for {name}"));
        assert_eq!(info.symbol, name);
        assert_eq!(info.kind, expected);
        assert!(info.declaration.is_none());
        assert_eq!(
            &body[info.start_byte as usize..info.end_byte as usize],
            name
        );
    }
}

/// Unread/unknown/invalid/cancelled/stale input never masquerades as a successful missing symbol.
#[test]
fn hover_preconditions_leave_current_source_and_resources_intact() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let control = QueryCancellation::default();
    assert!(matches!(
        session.module_symbol_hover(&path, session.revision(), 0, Locale::En, &control),
        Err(QueryError::ModuleUnavailable { .. })
    ));
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &InputRequest::default())
        .unwrap();
    let revision = session.revision();
    let resources = session.resources();
    for invalid in [1, u32::MAX] {
        assert!(matches!(
            session.module_symbol_hover(&path, revision, invalid, Locale::En, &control),
            Err(QueryError::InvalidOffset { .. })
        ));
    }
    let comment = u32::try_from(BODY.find("документация").unwrap()).unwrap();
    assert!(
        session
            .module_symbol_hover(&path, revision, comment, Locale::En, &control)
            .unwrap()
            .value
            .is_none()
    );
    control.cancel();
    assert!(matches!(
        session.module_symbol_hover(&path, revision, 0, Locale::En, &control),
        Err(QueryError::Cancelled)
    ));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
    assert!(matches!(
        kind("new pinned kind"),
        Err(QueryError::UnsupportedHoverSymbol { .. })
    ));
}

/// Internal kernel coordinates must invert their own line index, including lone CR and non-BMP text.
#[test]
fn symbol_cards_keep_byte_identity_across_current_line_endings() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let source = "\u{feff}Функция Значение() Экспорт\r\nВозврат 1;\r\nКонецФункции\r\nЯ=\"😀\";Значение();\r\n";
    for (version, ending) in [(1, "\r\n"), (2, "\n"), (3, "\r")] {
        let text = source.replace("\r\n", ending);
        session
            .apply_module_overlay(
                &path,
                session.revision(),
                version,
                &text,
                &InputRequest::default(),
            )
            .unwrap();
        let at = u32::try_from(text.rfind("Значение").unwrap()).unwrap();
        let card = session
            .module_symbol_hover(
                &path,
                session.revision(),
                at,
                Locale::En,
                &QueryCancellation::default(),
            )
            .unwrap()
            .value
            .unwrap_or_else(|| panic!("Missing current card for {ending:?}"));
        assert_eq!(card.symbol, "Значение");
        assert_eq!(card.start_byte, at);
        assert_eq!(
            &text[card.start_byte as usize..card.end_byte as usize],
            "Значение"
        );
    }
}
