//! Read-only BSL lifecycle acceptance probe; incomplete input is reported, never accepted as clean.

use std::{error::Error, path::PathBuf, time::Instant};

use eska::{
    analysis_core::InputRequest,
    project_core::source_format::{SourcePath, SourceReader},
};
use eska_analysis::{
    AnalysisSession, Diagnostic, DiagnosticCode, DiagnosticsConfig, Locale, ModuleResult,
    QueryCancellation,
};

/// Load the same declared metadata snapshot for resident and independent batch consumers.
fn load(root: &std::path::Path, module: &SourcePath) -> Result<AnalysisSession, Box<dyn Error>> {
    let mut session = AnalysisSession::open(root).map_err(|error| format!("{error:?}"))?;
    let paths = session.owner_paths().cloned().collect::<Vec<_>>();
    for path in paths {
        session
            .load_saved(&path, session.revision(), &InputRequest::default())
            .map_err(|error| format!("{error:?}"))?;
    }
    session
        .load_module_saved(module, session.revision(), &InputRequest::default())
        .map_err(|error| format!("{error:?}"))?;
    Ok(session)
}

/// Isolate syntax parity from unsupported metadata families and not-yet-loaded neighbor modules.
fn diagnostics(
    session: &AnalysisSession,
    path: &SourcePath,
) -> Result<ModuleResult<Vec<Diagnostic>>, Box<dyn Error>> {
    let config = DiagnosticsConfig {
        only_enabled: Some(vec![DiagnosticCode::ParseError]),
        locale: Locale::Ru,
        ..DiagnosticsConfig::default()
    };
    session
        .module_diagnostics(
            path,
            session.revision(),
            &config,
            &QueryCancellation::default(),
        )
        .map_err(|error| format!("{error:?}").into())
}

/// Resolve only declared sources and compare saved/editor/close/batch without altering disk.
fn main() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let [root] = arguments.as_slice() else {
        return Err("expected_eska_source_root".into());
    };
    let started = Instant::now();
    let reader = SourceReader::open(root)?;
    let module = reader
        .module_paths()
        .find(|path| {
            path.as_str().starts_with("CommonModules/")
                && path.as_str().ends_with("/Ext/Module.bsl")
        })
        .ok_or("no_declared_common_module_source")?
        .clone();
    let text = reader.read_module_text(&module, 64 * 1024 * 1024)?;
    let mut session = load(root, &module)?;
    let saved = diagnostics(&session, &module)?;
    session
        .apply_module_overlay(
            &module,
            session.revision(),
            1,
            &text,
            &InputRequest::default(),
        )
        .map_err(|error| format!("{error:?}"))?;
    let editor = diagnostics(&session, &module)?;
    session
        .close_module_overlay(&module, session.revision(), &InputRequest::default())
        .map_err(|error| format!("{error:?}"))?;
    let closed = diagnostics(&session, &module)?;
    let batch = diagnostics(&load(root, &module)?, &module)?;
    for value in [&editor, &closed, &batch] {
        if value.value != saved.value || value.source_sha256 != saved.source_sha256 {
            return Err("saved_editor_close_batch_mismatch".into());
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "profile": "syntax-only", "saved_editor_close_batch_parity": true,
            "diagnostics": saved.value.len(), "revision": session.revision(),
            "modules": session.module_summary(),
            "metadata_models_complete": saved.metadata_models_complete,
            "xml_exported": false, "seconds": started.elapsed().as_secs_f64(),
        })
    );
    if !saved.metadata_models_complete || !saved.module_inputs_complete {
        std::process::exit(2);
    }
    Ok(())
}
