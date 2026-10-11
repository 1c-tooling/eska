//! Signature preparation reuses native direct targets and current input authority without reading argument receivers.

use std::{fs, path::PathBuf};

use super::*;
use crate::{
    CallSignatures,
    session::tests::{owner, support::TestDirectory},
    test_common_module_project, test_manager_project,
};

const CALLER: &str = "CommonModules/Tools/Ext/Module.bsl";

/// The caller is ready before dependency preparation, matching the transport's UTF-16 conversion precondition.
fn fixture(body: &str, manager: bool) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    let folder = if manager {
        test_manager_project::add_managers(&root, &directory.0.join("native"));
        "src-with-managers"
    } else {
        test_common_module_project::add_common_modules(&root, &directory.0.join("native"));
        "src-with-common-modules"
    };
    let source = root.join(folder);
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_module_saved(&owner(CALLER), session.revision(), &InputRequest::default())
        .unwrap();
    (directory, source, session)
}

/// Query the original argument position after native discovery prepared its separate callee dependency.
fn signatures(session: &mut AnalysisSession, offset: u32) -> Option<CallSignatures> {
    session
        .prepare_signature_help_metadata(
            &owner(CALLER),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    session
        .module_signature_help(
            &owner(CALLER),
            session.revision(),
            offset,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
}

/// A cold direct common-module signature reads exactly its owner/body and follows current declaration parameters.
#[test]
fn cold_common_module_signature_prepares_target_and_tracks_current_parameters() {
    let prefix = "\u{feff}// Я😀\r\nProcedure Run()\r\n Utility.SavedUtility(";
    let body = format!("{prefix});\r\nEndProcedure\r\n");
    let (_directory, source, mut session) = fixture(&body, false);
    let offset = u32::try_from(prefix.len()).unwrap();
    assert!(
        session
            .module_signature_help(
                &owner(CALLER),
                session.revision(),
                offset,
                &QueryCancellation::default()
            )
            .unwrap()
            .value
            .is_none()
    );
    let saved = signatures(&mut session, offset).unwrap();
    assert_eq!(saved.signatures.len(), 1);
    assert_eq!(saved.signatures[0].parameters.len(), 1);
    assert!(saved.signatures[0].label.contains("SavedUtility"));
    let bytes = [
        CALLER,
        "CommonModules/Tools/object.yaml",
        "CommonModules/Utility/object.yaml",
        "CommonModules/Utility/Ext/Module.bsl",
    ]
    .into_iter()
    .map(|p| fs::metadata(source.join(p)).unwrap().len())
    .sum::<u64>();
    assert_eq!(session.resources().total_source_bytes, bytes);
    let before = session.summary();
    assert_eq!(signatures(&mut session, offset), Some(saved.clone()));
    assert_eq!(session.summary(), before);
    let target = owner("CommonModules/Utility/Ext/Module.bsl");
    let request = InputRequest::default();
    let current = "Procedure SavedUtility(Value = \"😀\", Next = 2) Export\r\nEndProcedure\r\n";
    session
        .apply_module_overlay(&target, session.revision(), 1, current, &request)
        .unwrap();
    let changed = signatures(&mut session, offset).unwrap();
    assert_eq!(changed.signatures[0].parameters.len(), 2);
    assert!(changed.signatures[0].label.contains('😀'));
    session
        .apply_module_overlay(&target, session.revision(), 2, "???", &request)
        .unwrap();
    assert!(signatures(&mut session, offset).is_none());
    session
        .close_module_overlay(&target, session.revision(), &request)
        .unwrap();
    assert_eq!(signatures(&mut session, offset), Some(saved));
    assert_eq!(
        session
            .module(&owner("CommonModules/Neighbor/Ext/Module.bsl"))
            .unwrap()
            .state,
        ModuleState::Pending
    );
}

/// Direct manager calls prepare the same canonical object, predefined resource and independent manager body as completion.
#[test]
fn cold_manager_signature_prepares_only_the_declared_direct_dependencies() {
    let prefix = "\u{feff}// Я😀\r\nProcedure Run()\r\n Catalogs.Items.SavedMethod(";
    let body = format!("{prefix});\r\nEndProcedure\r\n");
    let (_directory, source, mut session) = fixture(&body, true);
    let offset = u32::try_from(prefix.len()).unwrap();
    let saved = signatures(&mut session, offset).unwrap();
    assert!(saved.signatures[0].label.contains("SavedMethod"));
    let bytes = [
        CALLER,
        "CommonModules/Tools/object.yaml",
        "Catalogs/Items/object.yaml",
        "Catalogs/Items/Ext/Predefined.xml",
        "Catalogs/Items/Ext/ManagerModule.bsl",
    ]
    .into_iter()
    .map(|p| fs::metadata(source.join(p)).unwrap().len())
    .sum::<u64>();
    assert_eq!(session.resources().total_source_bytes, bytes);
    assert_eq!(
        session
            .module(&owner("Catalogs/Neighbor/Ext/ManagerModule.bsl"))
            .unwrap()
            .state,
        ModuleState::Pending
    );
    assert_eq!(
        session
            .metadata_resource(&owner("Catalogs/Items/Forms/Shared/Ext/Form.xml"))
            .unwrap()
            .state,
        crate::MetadataResourceState::Pending
    );
    let context = session.summary();
    signatures(&mut session, offset);
    assert_eq!(session.summary(), context);
}

/// Incidental argument receivers, unqualified calls and local shadowing never cause a foreign namespace read.
#[test]
fn signature_preparation_ignores_argument_namespace_and_shadowed_member_targets() {
    for template in [
        "Procedure Local(Value)\nEndProcedure\nProcedure Run()\n Local(Utility.$0);\nEndProcedure\n",
        "Procedure Run(Utility)\n Utility.SavedUtility($0);\nEndProcedure\n",
        "Procedure Run()\n Value=New Structure($0);\nEndProcedure\n",
    ] {
        let offset = u32::try_from(template.find("$0").unwrap()).unwrap();
        let body = template.replace("$0", "");
        let (_directory, _source, mut session) = fixture(&body, false);
        signatures(&mut session, offset);
        assert_eq!(
            session
                .inputs
                .owner(&owner("CommonModules/Utility/object.yaml"))
                .unwrap()
                .state,
            InputState::Pending
        );
        assert_eq!(
            session
                .module(&owner("CommonModules/Utility/Ext/Module.bsl"))
                .unwrap()
                .state,
            ModuleState::Pending
        );
    }
}

/// Stale, broken Unicode boundaries and pre-cancelled inputs reject preparation before any metadata publication.
#[test]
fn signature_preparation_preserves_source_state_on_rejected_preflight() {
    let prefix = "\u{feff}// Я😀\r\nProcedure Run()\r\n Utility.SavedUtility(";
    let body = format!("{prefix});\r\nEndProcedure\r\n");
    let (_directory, _source, mut session) = fixture(&body, false);
    let offset = u32::try_from(prefix.len()).unwrap();
    let before = session.summary();
    let current = session.revision();
    let path = owner(CALLER);
    assert!(matches!(
        session.prepare_signature_help_metadata(
            &path,
            SessionRevision(0),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default()
        ),
        Err(QueryError::Request(_))
    ));
    let broken = u32::try_from(body.find('😀').unwrap() + 1).unwrap();
    assert_eq!(
        session.prepare_signature_help_metadata(
            &path,
            current,
            broken,
            &InputRequest::default(),
            &QueryCancellation::default()
        ),
        Err(QueryError::InvalidOffset { offset: broken })
    );
    let cancellation = QueryCancellation::default();
    cancellation.cancel();
    assert_eq!(
        session.prepare_signature_help_metadata(
            &path,
            current,
            offset,
            &InputRequest::default(),
            &cancellation
        ),
        Err(QueryError::Cancelled)
    );
    assert_eq!(session.summary(), before);
}
