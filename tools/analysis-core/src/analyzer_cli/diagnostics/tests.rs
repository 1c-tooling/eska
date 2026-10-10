//! A cancelled source read is retryable, while an unavailable editor input stays authoritative.

use eska::analysis_core::{InputFailure, InputOrigin};
use eska::project_core::source_format::SourcePath;
use serde_json::json;

use super::*;
use crate::session::tests::fixture;
use crate::{ModuleFailure, ModuleState};

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
    let localizer = eska::tool_cli::localization::Localizer::try_new(
        eska::tool_cli::localization::Locale::EnUs,
    )
    .unwrap();
    let report = pull(
        &mut session,
        "connection",
        &params,
        &settings,
        &Options {
            request: &InputRequest::default(),
            cancellation: &fresh,
            timeout: Duration::from_secs(300),
            localizer: &localizer,
        },
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
        &Options {
            request: &InputRequest::default(),
            cancellation: &fresh,
            timeout: Duration::from_secs(300),
            localizer: &localizer,
        },
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

/// A transient cancelled saved metadata read retries; an unavailable editor must await a new accepted input.
#[test]
fn cancelled_metadata_saved_read_recovers_without_replacing_cancelled_editor_authority() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    let path = SourcePath::parse("CommonModules/Tools/object.yaml").unwrap();
    let cancelled = RequestCancellation::default();
    cancelled.cancel();
    let request = InputRequest {
        cancellation: cancelled.input,
        ..InputRequest::default()
    };
    session
        .load_saved(&path, session.revision(), &request)
        .unwrap();
    let settings =
        crate::analysis_settings::load(&root, None, eska::tool_cli::localization::Locale::EnUs);
    let localizer = eska::tool_cli::localization::Localizer::try_new(
        eska::tool_cli::localization::Locale::EnUs,
    )
    .unwrap();
    let params:Params=protocol::params(&json!({"textDocument":{"uri":url::Url::from_file_path(root.join(path.as_path())).unwrap().as_str()}})).unwrap();
    let fresh = RequestCancellation::default();
    let options = Options {
        request: &InputRequest::default(),
        cancellation: &fresh,
        timeout: Duration::from_secs(300),
        localizer: &localizer,
    };
    let report = pull(&mut session, "connection", &params, &settings, &options).unwrap();
    assert_eq!(report["items"], json!([]));
    assert_eq!(report["eska"]["profile"], "metadata");
    session
        .apply_overlay(&path, session.revision(), 7, "broken editor", &request)
        .unwrap();
    let revision = session.revision();
    let error = pull(&mut session, "connection", &params, &settings, &options).unwrap_err();
    assert_eq!(error["data"]["kind"], "metadata_input_unavailable");
    assert_eq!(session.revision(), revision);
    assert_eq!(
        session.metadata(&path, revision).unwrap().origin,
        InputOrigin::Overlay {
            document_version: 7
        }
    );
}
