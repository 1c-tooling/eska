use super::*;
use crate::backend::project::query::constructor::{Action, ClauseKind, ItemSetOperation, JoinKind};

/// The common session owns the draft lock; a second adapter cannot update or apply the canonical text behind it.
#[test]
fn constructor_exclusively_owns_its_draft_and_finishes_without_writing_the_owner() {
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project
        .query_document(&node, &path, 7, Some(INPUT))
        .unwrap();
    let selected = identity(&document);
    let mut session = QuerySession::open(
        project,
        document,
        selected,
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions::default(),
    )
    .unwrap();
    let draft = "SELECT A FROM T";
    session
        .set_draft(session.revision(), Arc::from(draft))
        .unwrap();
    let revision = session.revision();
    let token = session.open_constructor(revision).unwrap();
    assert!(session.constructor_active());
    assert!(!session.draft_writable());
    assert!(matches!(
        session.open_constructor(revision),
        Err(QuerySessionError::ConstructorBusy)
    ));
    assert!(matches!(
        session.set_draft(revision, Arc::from("different draft")),
        Err(QuerySessionError::ConstructorBusy)
    ));
    session.set_draft(revision, Arc::from(draft)).unwrap();
    assert!(matches!(
        session.prepare(
            project,
            revision,
            editor(7, &snapshot(INPUT), false),
            &[],
            None
        ),
        Err(QuerySessionError::ConstructorBusy)
    ));
    assert!(matches!(
        session.resume(
            project,
            editor(7, &snapshot(INPUT), false),
            QueryOwnerRestrictions::default()
        ),
        Err(QuerySessionError::ConstructorBusy)
    ));
    assert!(matches!(
        session.language_projection(project, revision, editor(7, &snapshot(INPUT), false)),
        Err(QuerySessionError::ConstructorBusy)
    ));
    session
        .constructor_action(
            token,
            1,
            &Action::Append {
                statement: 0,
                clause: ClauseKind::Fields,
                text: "B".to_owned(),
                join: JoinKind::Inner,
            },
        )
        .unwrap();
    assert_eq!(session.draft(), draft);
    assert_eq!(session.revision(), revision);
    assert_eq!(
        session.constructor(token).unwrap().text(),
        "SELECT A,\nB FROM T"
    );
    assert!(matches!(
        session.finish_constructor(token + 1, 2, &snapshot(draft), true),
        Err(QuerySessionError::UnknownConstructor)
    ));
    assert!(matches!(
        session.finish_constructor(token, 2, &snapshot("new native input"), true),
        Err(QuerySessionError::StaleDraft)
    ));
    assert!(session.constructor_active());
    let plan = session
        .finish_constructor(token, 2, &snapshot(draft), true)
        .unwrap()
        .unwrap();
    assert_eq!(plan.original(), draft);
    assert_eq!(plan.output(), session.draft());
    assert!(!session.constructor_active());
    assert!(session.draft_writable());
    assert_eq!(session.document().input(), INPUT);
    assert!(!session.applying());
    assert_eq!(
        std::fs::read_to_string(directory.0.join("src").join(&path)).unwrap(),
        INPUT
    );
    check_cancel_and_close(&mut session);
}

/// A read-only query still exposes its model, but no visual action can bypass the common source/user restriction.
#[test]
fn constructor_read_only_view_cannot_change_the_draft() {
    let (_directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project
        .query_document(&node, &path, 7, Some(INPUT))
        .unwrap();
    let selected = identity(&document);
    let mut session = QuerySession::open(
        project,
        document,
        selected,
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions {
            user_read_only: true,
        },
    )
    .unwrap();
    let draft = session.draft().to_owned();
    let token = session.open_constructor(1).unwrap();
    assert!(matches!(
        session.constructor_action(
            token,
            1,
            &Action::Append {
                statement: 0,
                clause: ClauseKind::Fields,
                text: "B".to_owned(),
                join: JoinKind::Inner
            }
        ),
        Err(QuerySessionError::UserReadOnly)
    ));
    assert!(matches!(
        session.constructor_action(
            token,
            1,
            &Action::EditItems {
                statement: 0,
                clause: 0,
                items: vec![0],
                operation: ItemSetOperation::Copy { to: 1 },
                comments: None,
            }
        ),
        Err(QuerySessionError::UserReadOnly)
    ));
    assert!(
        session
            .finish_constructor(token, 1, &snapshot(&draft), true)
            .unwrap()
            .unwrap()
            .is_empty()
    );
    assert_eq!(session.draft(), draft);
    assert!(!session.draft_writable());
}

/// Cancelling private working text leaves the canonical draft and its revision untouched.
fn check_cancel_and_close(session: &mut QuerySession) {
    let final_draft = session.draft().to_owned();
    let revision = session.revision();
    let token = session.open_constructor(revision).unwrap();
    session
        .constructor_action(
            token,
            1,
            &Action::Append {
                statement: 0,
                clause: ClauseKind::Order,
                text: "A".to_owned(),
                join: JoinKind::Inner,
            },
        )
        .unwrap();
    assert!(
        session
            .finish_constructor(token, 2, &snapshot(&final_draft), false)
            .unwrap()
            .is_none()
    );
    assert_eq!(session.draft(), final_draft);
    assert_eq!(session.revision(), revision);
    let token = session.open_constructor(revision).unwrap();
    session.close();
    assert!(!session.constructor_active());
    assert!(session.constructor(token).is_err());
}
