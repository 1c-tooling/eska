//! Structure queries pin current text and preserve native declaration details without extra IO.

use std::fmt::Write as _;
use std::fs;

use eska::analysis_core::InputRequest;
use ide::{ParamDefault, SymbolDetail, SymbolKind, TextRange};
use sha2::{Digest, Sha256};

use super::*;
use crate::session::tests::fixture;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "\u{feff}// Я😀\r\n#Область Интерфейс\r\nПерем Состояние Экспорт;\r\n#Область Вычисления\r\n&НаСервере\r\nФункция Значение(Знач Параметр = 1) Экспорт\r\nВозврат Параметр;\r\nКонецФункции\r\n#КонецОбласти\r\nПроцедура Вызов()\r\n#Область Локальная\r\n// Current body.\r\n#КонецОбласти\r\nКонецПроцедуры\r\n#КонецОбласти\r\n";

/// Independently hash exact source bytes, including their BOM and line endings.
fn hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::new(), |mut hash, byte| {
            write!(hash, "{byte:02x}").unwrap();
            hash
        })
}

/// The same item/region trees retain kinds, parameters and exact names in editor text.
#[test]
fn symbols_use_current_kernel_structure_without_reading_siblings_or_yaml() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &InputRequest::default())
        .unwrap();
    let revision = session.revision();
    let resources = session.resources();
    let result = session
        .module_document_symbols(&path, revision, &QueryCancellation::default())
        .unwrap();
    assert_eq!(result.revision, revision);
    assert_eq!(result.source_sha256, hash(BODY));
    assert!(!result.metadata_models_complete);
    assert!(!result.module_inputs_complete);
    assert_eq!(result.value.len(), 1);
    let region = &result.value[0];
    assert_eq!(region.name, "Интерфейс");
    assert_eq!(region.kind(), SymbolKind::Region);
    assert_eq!(
        region
            .children
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>(),
        ["Состояние", "Вычисления", "Вызов"]
    );
    assert_eq!(region.children[0].kind(), SymbolKind::Variable);
    let function = &region.children[1].children[0];
    let SymbolDetail::Function(detail) = &function.detail else {
        panic!("{function:?}")
    };
    assert!(detail.is_export);
    assert_eq!(detail.params.len(), 1);
    assert_eq!(detail.params[0].name, "Параметр");
    assert!(detail.params[0].by_value);
    assert_eq!(detail.params[0].default, ParamDefault::Value("1".into()));
    assert_eq!(&BODY[function.selection_range], "Значение");
    assert!(region.children[2].children.is_empty());
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
    assert_eq!(session.resources().metadata_source_bytes, 0);
}

/// Closing a draft reads current saved bytes; stale and cancelled queries leave the session intact.
#[test]
fn saved_editor_close_and_query_preconditions_share_one_revision() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let cancellation = QueryCancellation::default();
    assert!(matches!(
        session.module_document_symbols(&path, session.revision(), &cancellation),
        Err(QueryError::ModuleUnavailable { .. })
    ));
    let input = InputRequest::default();
    session
        .apply_module_overlay(&path, session.revision(), 1, BODY, &input)
        .unwrap();
    let previous = session.revision();
    fs::write(
        root.join(MODULE),
        "Процедура Сохранённая()\nКонецПроцедуры\n",
    )
    .unwrap();
    let result = session
        .module_document_symbols(&path, previous, &cancellation)
        .unwrap();
    assert_eq!(result.value[0].name, "Интерфейс");
    session
        .close_module_overlay(&path, session.revision(), &input)
        .unwrap();
    assert!(matches!(
        session.module_document_symbols(&path, previous, &cancellation),
        Err(QueryError::Request(_))
    ));
    let saved = session
        .module_document_symbols(&path, session.revision(), &cancellation)
        .unwrap();
    assert_eq!(saved.value[0].name, "Сохранённая");
    assert_ne!(saved.source_sha256, result.source_sha256);
    let revision = session.revision();
    let resources = session.resources();
    cancellation.cancel();
    assert!(matches!(
        session.module_document_symbols(&path, revision, &cancellation),
        Err(QueryError::Cancelled)
    ));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
    fs::remove_file(root.join(MODULE)).unwrap();
    session.load_module_saved(&path, revision, &input).unwrap();
    assert!(matches!(
        session.module_document_symbols(&path, session.revision(), &QueryCancellation::default()),
        Err(QueryError::ModuleUnavailable { .. })
    ));
}

/// Empty ready text is a valid empty structure; unknown, binary and over-quota inputs are failures.
#[test]
fn empty_structure_does_not_mask_unavailable_input() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    session
        .apply_module_overlay(&path, session.revision(), 1, "", &InputRequest::default())
        .unwrap();
    assert!(
        session
            .module_document_symbols(&path, session.revision(), &QueryCancellation::default())
            .unwrap()
            .value
            .is_empty()
    );
    for unavailable in [
        "Unknown/Module.bsl",
        "Catalogs/Sample/Ext/ManagerModule.bin",
    ] {
        assert!(
            session
                .module_document_symbols(
                    &SourcePath::parse(unavailable).unwrap(),
                    session.revision(),
                    &QueryCancellation::default()
                )
                .is_err()
        );
    }
    let request = InputRequest {
        max_document_bytes: 0,
        ..InputRequest::default()
    };
    session
        .apply_module_overlay(&path, session.revision(), 2, BODY, &request)
        .unwrap();
    assert!(matches!(
        session.module_document_symbols(&path, session.revision(), &QueryCancellation::default()),
        Err(QueryError::ModuleUnavailable { .. })
    ));
}

/// An invalid kernel range/name is a structured failure, including a child outside its parent.
#[test]
fn invalid_symbol_ranges_are_rejected_without_clamping() {
    let range = |start: u32, end: u32| TextRange::new(start.into(), end.into());
    let symbol = DocumentSymbol {
        name: "Я".into(),
        range: range(0, 6),
        selection_range: range(0, 2),
        detail: SymbolDetail::Region,
        children: vec![],
    };
    for invalid in [
        DocumentSymbol {
            name: " ".into(),
            ..symbol.clone()
        },
        DocumentSymbol {
            selection_range: range(0, 1),
            ..symbol.clone()
        },
        DocumentSymbol {
            selection_range: range(6, 7),
            ..symbol.clone()
        },
        DocumentSymbol {
            range: range(0, 7),
            ..symbol.clone()
        },
        DocumentSymbol {
            range: range(0, 2),
            children: vec![symbol.clone()],
            ..symbol.clone()
        },
    ] {
        assert!(matches!(
            validate(&[invalid], "Я😀", &QueryCancellation::default()),
            Err(QueryError::InconsistentDocumentSymbol { .. })
        ));
    }
    validate(&[symbol], "Я😀", &QueryCancellation::default()).unwrap();
}
