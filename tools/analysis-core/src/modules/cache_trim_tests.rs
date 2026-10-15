//! Deep trimming drops method diagnostic values while preserving current editor authority.

use std::{fmt::Write as _, fs};

use base_db::SourceDatabase;
use eska::analysis_core::InputRequest;

use crate::{
    AnalysisSession, DiagnosticCode, DiagnosticsConfig, QueryCancellation,
    session::tests::{fixture, owner},
};

/// Exceed the native sweep window without exceeding the interactive diagnostics window.
#[test]
fn deep_trim_releases_method_diagnostics_and_preserves_saved_and_editor_inputs() {
    const METHODS: usize = 2200;
    let (_directory, root) = fixture();
    let saved = owner("CommonModules/Tools/Ext/Module.bsl");
    let editor = owner("Catalogs/Sample/Ext/ObjectModule.bsl");
    let mut text = String::new();
    for method in 0..METHODS {
        writeln!(
            text,
            "// Saved diagnostic fixture.\nProcedure Method{method}()\n    Value = 1;\n    Value = Value;\nEndProcedure"
        )
        .unwrap();
    }
    fs::write(root.join(saved.as_path()), text).unwrap();
    let mut session = AnalysisSession::open(&root).unwrap();
    session
        .load_module_saved(&saved, session.revision(), &InputRequest::default())
        .unwrap();
    session
        .apply_module_overlay(
            &editor,
            session.revision(),
            7,
            "// Retained editor fixture.\nProcedure CurrentDraft()\n    Draft = 1;\n    Draft = Draft;\nEndProcedure\n",
            &InputRequest::default(),
        )
        .unwrap();
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::SelfAssign]),
        ..DiagnosticsConfig::default()
    };
    let before = [&saved, &editor].map(|path| {
        session
            .module_diagnostics(
                path,
                session.revision(),
                &config,
                &QueryCancellation::default(),
            )
            .unwrap()
    });
    assert_eq!(before[0].value.len(), METHODS);
    assert_eq!(before[1].value.len(), 1);
    let before_heap = method_diagnostic_heap(&session);
    assert!(before_heap > 0);
    let revision = session.revision();
    let resources = session.resources();
    let inputs = [&saved, &editor].map(|path| session.module(path).unwrap().clone());
    let editor_hint = session
        .database
        .parse_snapshot(session.modules[&editor].file)
        .unwrap();

    session.trim_batch_caches();

    // Each native hook counts the diagnostic vector and owned messages. A lower aggregate here
    // proves eviction of computed values; memo instance counts or process RSS would not.
    assert!(method_diagnostic_heap(&session) < before_heap);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
    for (path, input) in [&saved, &editor].into_iter().zip(inputs) {
        assert_eq!(session.module(path), Some(&input));
    }
    assert!(
        session
            .database
            .parse_snapshot(session.modules[&saved].file)
            .is_none()
    );
    let retained = session
        .database
        .parse_snapshot(session.modules[&editor].file)
        .unwrap();
    assert_eq!(retained.text, editor_hint.text);
    assert_eq!(retained.parse, editor_hint.parse);
    for (path, expected) in [&saved, &editor].into_iter().zip(before) {
        assert_eq!(
            session
                .module_diagnostics(path, revision, &config, &QueryCancellation::default(),)
                .unwrap(),
            expected
        );
    }
}

/// The pinned method and slab query tables share this value type; sum rather than merge names.
fn method_diagnostic_heap(session: &AnalysisSession) -> usize {
    let tables = session.kernel_memory().tables;
    let method_tables = tables
        .iter()
        .filter(|table| {
            table
                .name
                .contains("Diagnostic<cfg_types::local_range::LocalRange>")
        })
        .collect::<Vec<_>>();
    assert!(
        !method_tables.is_empty(),
        "native diagnostic table not found"
    );
    method_tables
        .into_iter()
        .map(|table| {
            table
                .heap_bytes
                .expect("native diagnostics have a heap hook")
        })
        .sum()
}
