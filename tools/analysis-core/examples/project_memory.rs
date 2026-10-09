//! Read-only measurements distinguish current input quotas from kernel table estimates and RSS.

use std::{error::Error, path::PathBuf};

use eska::analysis_core::InputRequest;
use eska_analysis::{
    AnalysisSession, DiagnosticCode, DiagnosticsConfig, ModuleState, QueryCancellation,
};

/// Load declared metadata and a bounded working set, then sample explicit release and memo trim.
fn main() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let [root] = arguments.as_slice() else {
        return Err("expected_eska_source_root".into());
    };
    let mut session = AnalysisSession::open(root).map_err(|error| format!("{error:?}"))?;
    let opened = session.kernel_memory();
    let owners = session.owner_paths().cloned().collect::<Vec<_>>();
    for path in owners {
        session
            .load_saved(&path, session.revision(), &InputRequest::default())
            .map_err(|error| format!("{error:?}"))?;
    }
    let metadata = session.kernel_memory();
    let modules = session
        .module_paths()
        .filter(|path| {
            path.as_path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bsl"))
        })
        .take(40)
        .cloned()
        .collect::<Vec<_>>();
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::ParseError]),
        ..DiagnosticsConfig::default()
    };
    let mut checked = 0;
    for path in &modules {
        session
            .load_module_saved(path, session.revision(), &InputRequest::default())
            .map_err(|error| format!("{error:?}"))?;
        if matches!(
            session.module(path).map(|input| &input.state),
            Some(ModuleState::Ready { .. })
        ) {
            session
                .module_diagnostics(
                    path,
                    session.revision(),
                    &config,
                    &QueryCancellation::default(),
                )
                .map_err(|error| format!("{error:?}"))?;
            checked += 1;
        }
    }
    let queried = session.kernel_memory();
    let loaded_resources = session.resources();
    for path in &modules {
        session
            .release_module_saved(path, session.revision())
            .map_err(|error| format!("{error:?}"))?;
    }
    let released = session.kernel_memory();
    session.trim_batch_caches();
    println!(
        "{}",
        serde_json::json!({
            "profile":"syntax", "selected_modules":modules.len(), "checked_modules":checked,
            "opened":opened, "metadata":metadata,
            "queried":queried, "released":released, "trimmed":session.kernel_memory(),
            "loaded_resources":loaded_resources, "current_resources":session.resources(),
            "metadata_models_complete":session.summary().descriptor_models_complete,
        })
    );
    if !session.summary().descriptor_models_complete {
        std::process::exit(2);
    }
    Ok(())
}
