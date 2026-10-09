//! Shared quotas preserve current-input semantics across both languages and release failed replacements.

use std::fs;

use eska::analysis_core::{InputFailure, InputRequest, InputState};

use crate::session::tests::{fixture, owner};
use crate::{AnalysisSession, ModelState, ModuleFailure, ModuleState, SessionOptions};

/// Metadata and BSL compete for one reservation; rejected current input cannot retain an old slot.
#[test]
fn shared_source_quota_replaces_and_releases_metadata_and_bsl_reservations() {
    let (_directory, root) = fixture();
    let metadata = owner("CommonModules/Tools/object.yaml");
    let module = owner("CommonModules/Tools/Ext/Module.bsl");
    let yaml = fs::read_to_string(root.join(metadata.as_path())).unwrap();
    let body = fs::read(root.join(module.as_path())).unwrap();
    let limit = (yaml.len() + body.len()) as u64;
    let mut session = AnalysisSession::open_with_options(
        &root,
        SessionOptions {
            max_retained_source_bytes: limit,
            ..SessionOptions::default()
        },
    )
    .unwrap();
    session
        .load_saved(&metadata, session.revision(), &InputRequest::default())
        .unwrap();
    session
        .load_module_saved(&module, session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(session.resources().total_source_bytes, limit);
    let retained = session.metadata(&metadata, session.revision()).unwrap();
    let other = owner("Catalogs/Sample/Ext/ObjectModule.bsl");
    session
        .load_module_saved(&other, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(matches!(session.module(&other).unwrap().state,
        ModuleState::Unavailable(ModuleFailure::Input(InputFailure::SourceBudgetExceeded { limit: actual })) if actual == limit));
    assert_eq!(session.resources().total_source_bytes, limit);
    let larger = format!("# larger current source\n{yaml}");
    session
        .apply_overlay(
            &metadata,
            session.revision(),
            1,
            &larger,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(matches!(
        session.owner(&metadata).unwrap().state,
        ModelState::Unavailable(InputFailure::SourceBudgetExceeded { .. })
    ));
    assert!(
        session
            .resolve(&metadata, session.revision())
            .unwrap()
            .is_none()
    );
    assert_eq!(session.resources().metadata_source_bytes, 0);
    assert_eq!(session.resources().module_text_bytes, body.len() as u64);
    assert!(matches!(retained.input, InputState::Ready(_)));
    let current = session.revision();
    assert!(
        session
            .apply_overlay(&metadata, current, 1, &yaml, &InputRequest::default())
            .is_err()
    );
    assert_eq!(session.revision(), current);
    assert_eq!(session.resources().total_source_bytes, body.len() as u64);
    session
        .close_overlay(&metadata, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(
        session
            .resolve(&metadata, session.revision())
            .unwrap()
            .is_some()
    );
    assert_eq!(session.resources().total_source_bytes, limit);
    session
        .release_module_saved(&module, session.revision())
        .unwrap();
    assert_eq!(session.resources().total_source_bytes, yaml.len() as u64);
    session
        .load_saved(&metadata, session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(session.resources().total_source_bytes, yaml.len() as u64);
    assert_eq!(fs::read(root.join(module.as_path())).unwrap(), body);
    assert_eq!(
        fs::read_to_string(root.join(metadata.as_path())).unwrap(),
        yaml
    );
}
