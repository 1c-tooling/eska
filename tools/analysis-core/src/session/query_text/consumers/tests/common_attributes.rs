//! Current explicit common fields participate in the same native query and resource-binding proof.

use super::*;

/// Known common types permit the existing proof; removed membership or an invalid current declaration retracts it.
#[test]
fn current_common_membership_and_type_reach_query_outputs_and_consumer_report() {
    let directory = TestDirectory::new();
    let project = fixture::project_with_common_attribute(&directory.0);
    let session = AnalysisSession::open(&project.root.join("src")).unwrap();
    check(&project, session);
}

/// The same report contract applies to a standalone CF or an explicitly admitted selected CFE.
pub(super) fn check(project: &fixture::Project, mut session: AnalysisSession) {
    let before = fixture::snapshot(project);
    bind(&mut session, &project.source, "Value");
    let request = InputRequest::default();
    session
        .apply_query_overlay(
            &project.source,
            session.revision(),
            1,
            "\u{feff}// Я😀\r\nSELECT Data.SharedЯ AS Renamed FROM Catalog.Items AS Data",
            &request,
        )
        .unwrap();
    let common = SourcePath::parse("CommonAttributes/SharedЯ/object.yaml").unwrap();
    let saved = fs::read_to_string(project.root.join("src").join(common.as_path())).unwrap();
    let first = current(&session, &project.source);
    assert!(first.coverage.output_types_complete);
    assert_eq!(first.findings.len(), 1);
    session
        .apply_overlay(
            &common,
            session.revision(),
            1,
            &saved.replace("x:string", "x:boolean"),
            &request,
        )
        .unwrap();
    let changed = current(&session, &project.source);
    assert!(changed.coverage.output_types_complete);
    assert_eq!(changed.findings.len(), 1);
    assert_eq!(changed.context.query, first.context.query);
    let QueryTextStatement::Select(select) = &changed.model.statements[0] else {
        panic!("SELECT");
    };
    assert_eq!(
        select.branches[0].fields[0].ty,
        crate::QueryOutputType::Boolean
    );
    session
        .apply_overlay(
            &common,
            session.revision(),
            2,
            &saved.replace("<r:Use>Use</r:Use>", "<r:Use>DontUse</r:Use>"),
            &request,
        )
        .unwrap();
    let removed = current(&session, &project.source);
    assert!(!removed.coverage.output_types_complete);
    assert!(removed.findings.is_empty());
    session
        .apply_overlay(&common, session.revision(), 3, "broken: [", &request)
        .unwrap();
    assert!(matches!(
        session.query_consumer_report(
            &project.source,
            session.revision(),
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryConsumerRequestError::Request(
            SessionRequestError::CompositionInputIncomplete(_)
        ))
    ));
    session
        .close_overlay(&common, session.revision(), &request)
        .unwrap();
    assert_eq!(current(&session, &project.source).findings.len(), 1);
    assert_eq!(fixture::snapshot(project), before);
}
