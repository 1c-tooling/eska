//! A cancelled source read is retryable, while an unavailable editor input stays authoritative.

use eska::project_core::source_format::SourcePath;
use serde_json::json;

use super::*;
use crate::session::tests::fixture;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";

/// Retrying a cancelled saved read obtains current bytes; it never recovers an editor from disk.
#[test]
fn cancelled_saved_read_retries_but_cancelled_overlay_does_not_fall_back_to_disk() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse(MODULE).unwrap();
    let cancelled = RequestCancellation::default();
    cancelled.cancel();
    let cancelled_input = InputRequest {
        cancellation: cancelled.input,
        ..InputRequest::default()
    };
    session
        .load_module_saved(&path, session.revision(), &cancelled_input)
        .unwrap();
    assert!(matches!(
        session.module(&path).unwrap().state,
        ModuleState::Unavailable(ModuleFailure::Input(InputFailure::Cancelled))
    ));
    let body = "Procedure Current()\nEndProcedure\n";
    std::fs::write(root.join(MODULE), body).unwrap();
    let params:Params=protocol::params(&json!({"textDocument":{"uri":url::Url::from_file_path(root.join(MODULE)).unwrap().to_string()}})).unwrap();
    let settings =
        crate::analysis_settings::load(&root, None, eska::tool_cli::localization::Locale::EnUs);
    let fresh = RequestCancellation::default();
    let report = pull(
        &mut session,
        "connection",
        &params,
        &settings,
        &InputRequest::default(),
        &fresh,
        Duration::from_secs(300),
    )
    .unwrap();
    assert_eq!(report["items"], json!([]));
    assert_eq!(
        session
            .with_module_text(&path, session.revision(), &fresh.query, str::to_owned)
            .unwrap()
            .value,
        body
    );
    session
        .apply_module_overlay(
            &path,
            session.revision(),
            1,
            "broken editor",
            &cancelled_input,
        )
        .unwrap();
    let revision = session.revision();
    let error = pull(
        &mut session,
        "connection",
        &params,
        &settings,
        &InputRequest::default(),
        &fresh,
        Duration::from_secs(300),
    )
    .unwrap_err();
    assert_eq!(error["data"]["kind"], "module_unavailable");
    assert_eq!(session.revision(), revision);
    assert!(matches!(
        session.module(&path).unwrap().origin,
        InputOrigin::Overlay {
            document_version: 1
        }
    ));
}
