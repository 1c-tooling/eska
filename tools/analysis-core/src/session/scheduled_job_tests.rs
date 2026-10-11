//! The current scheduled-job value reaches a tracked canonical slot through the shared lifecycle.

#[path = "../../tests/support/scheduled_job_project.rs"]
mod fixture;

use super::*;
use crate::session::tests::{owner, support::TestDirectory};

/// Read the actual canonical resolver rather than reconstructing the expected current model.
fn job(session: &AnalysisSession, path: &SourcePath) -> Arc<bsl_metadata::ScheduledJob> {
    let Some(CanonicalMetadata::ScheduledJob(value)) =
        session.resolve(path, session.revision()).unwrap()
    else {
        panic!("expected current scheduled job");
    };
    value
}

/// Callback and flags update together; invalid or unavailable editor input never revives saved data.
#[test]
fn scheduled_jobs_use_current_yaml_and_preserve_unrelated_models() {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Current body.\n", false);
    fixture::add_jobs(&root, &directory.0.join("native"));
    let source = root.join("src-with-jobs");
    let mut session = AnalysisSession::open(&source).unwrap();
    let path = owner("ScheduledJobs/BeforeRun/object.yaml");
    let neighbor = owner("ScheduledJobs/Neighbor/object.yaml");
    let request = InputRequest::default();
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(session.summary().descriptor_models_complete);
    let saved = job(&session, &path);
    let untouched = job(&session, &neighbor);
    let text = std::fs::read_to_string(source.join(path.as_path())).unwrap();
    let native =
        std::fs::read_to_string(directory.0.join("native/ScheduledJobs/BeforeRun.xml")).unwrap();
    assert_eq!(
        *saved,
        bsl_metadata::xml_parser::parse_scheduled_job_xml(&native).unwrap()
    );
    let draft = text
        .replace("CommonModule.Tools.Saved", "CommonModule.Tools.Current")
        .replace(
            "<Predefined>false</Predefined>",
            "<Predefined>true</Predefined>",
        )
        .replace("<Use>true</Use>", "<Use>false</Use>");
    session
        .apply_overlay(&path, session.revision(), 7, &draft, &request)
        .unwrap();
    let current = job(&session, &path);
    assert_eq!(current.method_name(), "CommonModule.Tools.Current");
    assert!(current.is_predefined());
    assert!(!current.is_enabled());
    assert_eq!(saved.method_name(), "CommonModule.Tools.Saved");
    assert!(!saved.is_predefined());
    assert!(saved.is_enabled());
    assert!(Arc::ptr_eq(&untouched, &job(&session, &neighbor)));
    let invalid = draft.replace("<Use>false</Use>", "<Use>Future</Use>");
    session
        .apply_overlay(&path, session.revision(), 8, &invalid, &request)
        .unwrap();
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Invalid { .. }
    ));
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    assert!(!session.summary().descriptor_models_complete);
    session
        .apply_overlay(&path, session.revision(), 9, "document: [", &request)
        .unwrap();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session
        .close_overlay(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(*job(&session, &path), *saved);
    assert!(session.summary().descriptor_models_complete);
    assert_eq!(
        std::fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
}
