//! Process/exchange models and task-addressing fields navigate through the same canonical session.

use super::*;
use crate::session::tests::fixture;
use eska::{
    analysis_core::InputRequest,
    project_core::source_format::{NativeUuid, import_designer},
};
use std::fs;

#[path = "../../../tests/definition_process_queries/fixture.rs"]
mod processes;

const CALLER: &str = "Catalogs/Sample/Ext/ObjectModule.bsl";

/// Only a private native copy is expanded; all declared main models remain lazily selected.
fn session() -> (
    crate::session::tests::support::TestDirectory,
    AnalysisSession,
) {
    let (directory, _) = fixture();
    processes::extend(&directory.0.join("native"));
    let source = directory.0.join("process-source");
    import_designer(&directory.0.join("native"), &source).unwrap();
    let mut session = AnalysisSession::open(&source).unwrap();
    for case in &processes::CASES {
        let owner =
            SourcePath::parse(&format!("{}/{}/object.yaml", case.folder, case.name)).unwrap();
        session
            .load_saved(&owner, session.revision(), &InputRequest::default())
            .unwrap();
        assert!(matches!(
            session.owner(&owner).unwrap().state,
            ModelState::Ready(_)
        ));
    }
    (directory, session)
}

/// The cursor is placed inside a complete fixture identifier, after the caller's current publication.
fn query(
    session: &mut AnalysisSession,
    body: &str,
    field: &str,
    version: i64,
) -> Option<MetadataLocation> {
    let caller = SourcePath::parse(CALLER).unwrap();
    session
        .apply_module_overlay(
            &caller,
            session.revision(),
            version,
            body,
            &InputRequest::default(),
        )
        .unwrap();
    let offset =
        u32::try_from(body.rfind(field).unwrap() + field.chars().next().unwrap().len_utf8())
            .unwrap();
    session
        .module_metadata_definition(
            &caller,
            session.revision(),
            offset,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
}

/// Objects, attributes, tables and row fields keep exact UUID/parent scope for each added family.
#[test]
fn process_exchange_and_task_members_use_current_native_identities() {
    let (_directory, mut session) = session();
    let mut version = 1;
    for case in &processes::CASES {
        let owner =
            SourcePath::parse(&format!("{}/{}/object.yaml", case.folder, case.name)).unwrap();
        for (receiver, field, part) in [
            ("", "ExtraField", 1),
            ("", "Rows", 2),
            ("Object=Object.Rows.Add();", "Quantity", 3),
        ] {
            let body = processes::body(case, receiver, field);
            let target = query(&mut session, &body, field, version).unwrap();
            version += 1;
            let uuid = NativeUuid::parse(&processes::uuid(case, part)).unwrap();
            assert_eq!(target.source, owner);
            assert_eq!(
                target,
                session
                    .metadata_location(&owner, session.revision(), &uuid, None)
                    .unwrap()
            );
        }
    }
    let case = &processes::CASES[1];
    let body = processes::body(case, "", "Assignee");
    assert_eq!(
        query(&mut session, &body, "Assignee", version)
            .unwrap()
            .uuid,
        processes::uuid(case, 4)
    );
}

/// An editor rename changes the actual addressing declaration; invalid/closed missing YAML clears old targets.
#[test]
fn task_addressing_navigation_tracks_editor_yaml_without_saved_fallback() {
    let (_directory, mut session) = session();
    let case = &processes::CASES[1];
    let owner = SourcePath::parse("Tasks/Work/object.yaml").unwrap();
    let request = InputRequest::default();
    let saved = fs::read_to_string(session.root().join(owner.as_path())).unwrap();
    let body = processes::body(case, "", "Assignee");
    let first = query(&mut session, &body, "Assignee", 1).unwrap();
    let draft = format!(
        "# Я😀 editor rename\r\n{}",
        saved.replace("Assignee", "Responsible")
    );
    assert_ne!(draft, saved);
    session
        .apply_overlay(&owner, session.revision(), 7, &draft, &request)
        .unwrap();
    assert!(query(&mut session, &body, "Assignee", 2).is_none());
    let renamed = processes::body(case, "", "Responsible");
    let target = query(&mut session, &renamed, "Responsible", 3).unwrap();
    assert_eq!(target.uuid, first.uuid);
    assert_ne!(target.range, first.range);
    assert_ne!(target.source_sha256, first.source_sha256);
    fs::remove_file(session.root().join(owner.as_path())).unwrap();
    let current = query(&mut session, &renamed, "Responsible", 4).unwrap();
    let mut expected = target;
    expected.revision = session.revision();
    assert_eq!(current, expected);
    session
        .apply_overlay(&owner, session.revision(), 8, "broken: [\n", &request)
        .unwrap();
    assert!(query(&mut session, &renamed, "Responsible", 5).is_none());
    session
        .close_overlay(&owner, session.revision(), &request)
        .unwrap();
    assert!(query(&mut session, &body, "Assignee", 6).is_none());
    fs::write(session.root().join(owner.as_path()), saved).unwrap();
    session
        .load_saved(&owner, session.revision(), &request)
        .unwrap();
    assert_eq!(
        query(&mut session, &body, "Assignee", 7).unwrap().uuid,
        first.uuid
    );
}

/// A same-named ordinary/addressing declaration cannot silently select one of two real task UUIDs.
#[test]
fn ambiguous_task_member_kinds_do_not_select_the_first_metadata_declaration() {
    let (_directory, mut session) = session();
    let case = &processes::CASES[1];
    let owner = SourcePath::parse("Tasks/Work/object.yaml").unwrap();
    let saved = fs::read_to_string(session.root().join(owner.as_path())).unwrap();
    let draft = saved.replace("Assignee", "ExtraField");
    let request = InputRequest::default();
    session
        .apply_overlay(&owner, session.revision(), 1, &draft, &request)
        .unwrap();
    assert!(matches!(
        session.owner(&owner).unwrap().state,
        ModelState::Ready(_)
    ));
    let caller = SourcePath::parse(CALLER).unwrap();
    let body = processes::body(case, "", "ExtraField");
    session
        .apply_module_overlay(&caller, session.revision(), 1, &body, &request)
        .unwrap();
    let at = u32::try_from(body.rfind("ExtraField").unwrap() + 1).unwrap();
    assert_eq!(
        session.module_metadata_definition(
            &caller,
            session.revision(),
            at,
            &QueryCancellation::default()
        ),
        Err(QueryError::UnsupportedNavigationTarget)
    );
}
