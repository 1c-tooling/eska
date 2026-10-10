//! Memory introspection preserves input state and still permits queries after exclusive cache trimming.

use base_db::SourceDatabase;
use eska::analysis_core::InputRequest;

use crate::session::tests::{fixture, owner};
use crate::{AnalysisSession, DiagnosticCode, DiagnosticsConfig, QueryCancellation};

/// Sampling is read-only, and trim may discard computation without discarding current source.
#[test]
fn memory_samples_do_not_evaluate_or_modify_current_input_and_trim_preserves_diagnostics() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let initial = session.revision();
    let resources = session.resources();
    let sample = session.kernel_memory();
    assert_eq!(sample, session.kernel_memory());
    assert_eq!(session.revision(), initial);
    assert_eq!(session.resources(), resources);
    assert!(
        sample
            .tables
            .iter()
            .all(|table| !table.name.starts_with("syntax::Parse<") || table.entries == 0)
    );
    let module = owner("CommonModules/Tools/Ext/Module.bsl");
    session
        .load_module_saved(&module, session.revision(), &InputRequest::default())
        .unwrap();
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::ParseError]),
        ..DiagnosticsConfig::default()
    };
    let before = session
        .module_diagnostics(
            &module,
            session.revision(),
            &config,
            &QueryCancellation::default(),
        )
        .unwrap();
    let sample = session.kernel_memory();
    assert!(
        sample
            .tables
            .iter()
            .any(|table| table.entries > 0 && table.name.starts_with("syntax::Parse<"))
    );
    assert!(sample.tables.iter().any(|table| table.heap_bytes.is_none()));
    assert!(
        sample
            .tables
            .iter()
            .any(|table| table.heap_bytes == Some(0))
    );
    assert_eq!(sample.revision, before.revision);
    let mut independent = AnalysisSession::open(&root).unwrap();
    independent
        .load_module_saved(&module, independent.revision(), &InputRequest::default())
        .unwrap();
    independent
        .module_diagnostics(
            &module,
            independent.revision(),
            &config,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert_eq!(sample, independent.kernel_memory());
    let resources = session.resources();
    session.trim_batch_caches();
    assert_eq!(session.revision(), before.revision);
    assert_eq!(session.resources(), resources);
    let after = session
        .module_diagnostics(
            &module,
            session.revision(),
            &config,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert_eq!(before, after);
    assert!(
        session
            .kernel_memory()
            .tables
            .windows(2)
            .all(|pair| pair[0].name <= pair[1].name)
    );
}

/// Releasing source must also release the native reparse hint outside Salsa's evictable memos.
#[test]
fn released_saved_module_does_not_pin_native_parse_snapshot() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let module = owner("CommonModules/Tools/Ext/Module.bsl");
    let file = session.modules[&module].file;
    session
        .load_module_saved(&module, session.revision(), &InputRequest::default())
        .unwrap();
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::ParseError]),
        ..DiagnosticsConfig::default()
    };
    let before = session
        .module_diagnostics(
            &module,
            session.revision(),
            &config,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert!(session.database.parse_snapshot(file).is_some());
    session
        .release_module_saved(&module, session.revision())
        .unwrap();
    assert_eq!(session.resources().module_text_bytes, 0);
    assert!(session.database.parse_snapshot(file).is_none());
    session
        .load_module_saved(&module, session.revision(), &InputRequest::default())
        .unwrap();
    let after = session
        .module_diagnostics(
            &module,
            session.revision(),
            &config,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert_eq!(before.value, after.value);
}

/// Rejected overlay input discards its hint without evicting another module or restoring old text.
#[test]
fn rejected_overlay_releases_parse_hint_and_close_reloads_saved_source() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let module = owner("CommonModules/Tools/Ext/Module.bsl");
    let neighbor = owner("Catalogs/Sample/Ext/ObjectModule.bsl");
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::ParseError]),
        ..DiagnosticsConfig::default()
    };
    for path in [&module, &neighbor] {
        session
            .load_module_saved(path, session.revision(), &InputRequest::default())
            .unwrap();
        session
            .module_diagnostics(
                path,
                session.revision(),
                &config,
                &QueryCancellation::default(),
            )
            .unwrap();
    }
    let file = session.modules[&module].file;
    let other = session.modules[&neighbor].file;
    let before = session.database.parse_snapshot(other).unwrap();
    let original = session
        .module_diagnostics(
            &module,
            session.revision(),
            &config,
            &QueryCancellation::default(),
        )
        .unwrap();
    let hash = session.module(&module).unwrap().source_sha256.clone();
    session
        .apply_module_overlay(
            &module,
            session.revision(),
            1,
            "Procedure Unavailable() EndProcedure",
            &InputRequest {
                max_document_bytes: 1,
                ..InputRequest::default()
            },
        )
        .unwrap();
    assert!(session.database.parse_snapshot(file).is_none());
    assert!(session.database.file_is_unread(file));
    assert_eq!(
        session.database.parse_snapshot(other).unwrap().text,
        before.text
    );
    assert!(
        session
            .module_diagnostics(
                &module,
                session.revision(),
                &config,
                &QueryCancellation::default()
            )
            .is_err()
    );
    session
        .close_module_overlay(&module, session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(session.module(&module).unwrap().source_sha256, hash);
    assert!(session.database.parse_snapshot(file).is_none());
    let recovered = session
        .module_diagnostics(
            &module,
            session.revision(),
            &config,
            &QueryCancellation::default(),
        )
        .unwrap();
    assert_eq!(original.value, recovered.value);
}
