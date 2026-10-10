//! Memory introspection preserves input state and still permits queries after exclusive cache trimming.

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
