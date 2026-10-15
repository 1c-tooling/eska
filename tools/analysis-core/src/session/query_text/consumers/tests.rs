//! Real current storage and canonical owner anchors exercise consumer facts without synthetic BSL.

mod common_attributes;
mod parameters;

use std::fs;

use eska::{
    analysis_core::{InputFailure, InputOrigin},
    project_core::{
        ProjectName,
        creation::import_query_project,
        source_format::{QueryAuthoringRange, SourcePath},
    },
};

use super::*;
use crate::{
    QueryTextStatement, query_creation_fixture as native, test_query_analysis_project as fixture,
    test_support::TestDirectory,
};

/// One current DCS binding is added through the same raw resource overlay as a real editor.
fn bind(session: &mut AnalysisSession, path: &SourcePath, field: &str) -> (SourcePath, String) {
    let request = InputRequest::default();
    session
        .prepare_query_consumers(path, session.revision(), &request)
        .unwrap();
    let context = session
        .query_context(path, session.revision(), &request)
        .unwrap();
    let resource = context.resource.source;
    let saved = session.metadata_resource(&resource).unwrap();
    let crate::MetadataResourceState::Ready(text) = &saved.state else {
        panic!("prepared raw DCS resource");
    };
    let draft = text.replace("</name>", &format!("</name><field xsi:type=\"DataSetFieldField\"><dataPath>Published</dataPath><field>{field}</field></field>"));
    session
        .apply_resource_overlay(&resource, session.revision(), 1, &draft, &request)
        .unwrap();
    (resource, draft)
}

/// Consumer diagnostics read prepared current inputs and never mutate source reservations or clocks.
fn current(session: &AnalysisSession, path: &SourcePath) -> QueryConsumerDiagnostics {
    let revision = session.revision();
    let resources = session.resources();
    let result = session
        .query_consumer_diagnostics(
            path,
            revision,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
    assert_eq!(result.context.revision, revision);
    assert_eq!(result.context.root, session.selected_root_id());
    assert!(!result.coverage.consumer_complete());
    assert!(!result.coverage.parameters_complete());
    assert!(!result.coverage.semantic_complete());
    assert_eq!(session.resources().module_text_bytes, 0);
    let report = session
        .query_consumer_report(
            path,
            revision,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    assert_eq!(report.findings.len(), result.findings.len());
    assert_eq!(report.context.query, result.context.query);
    assert_eq!(report.context.resource, result.context.resource);
    assert_eq!(
        report.coverage.checked_bindings,
        result.coverage.checked_bindings
    );
    let machine = serde_json::to_value(report).unwrap();
    assert_eq!(machine["context"]["revision"], serde_json::json!(revision));
    assert!(machine["context"].get("consumer").is_none());
    result
}

/// Native output spelling and resource byte ranges address the exact current hash-qualified text.
#[test]
fn dcs_fields_follow_current_query_names_conditions_and_exact_locations() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let before = fixture::snapshot(&project);
    let mut session = AnalysisSession::open(&project.root.join("src")).unwrap();
    let (resource, draft) = bind(&mut session, &project.source, "vAlUe");
    let original = current(&session, &project.source);
    assert!(original.findings.is_empty());
    assert_eq!(original.coverage.checked_bindings, 1);
    assert!(original.coverage.output_names_complete);
    assert!(original.coverage.output_types_complete);
    let stale = session.revision();
    for (version, text, missing) in [
        (
            1,
            "\u{feff}// 😀\r\nSELECT T.CurrentField AS Renamed FROM Catalog.Items AS T",
            true,
        ),
        (
            2,
            "SELECT T.CurrentField AS Value FROM Catalog.Items AS T WHERE T.Code = &Параметр OR T.Code = &параметр",
            false,
        ),
    ] {
        session
            .apply_query_overlay(
                &project.source,
                session.revision(),
                version,
                text,
                &InputRequest::default(),
            )
            .unwrap();
        let result = current(&session, &project.source);
        assert_eq!(!result.findings.is_empty(), missing);
        assert_eq!(result.context.resource.source, resource);
        assert_eq!(
            result.context.resource.origin,
            InputOrigin::Overlay {
                document_version: 1
            }
        );
        assert_eq!(
            result.context.query.origin,
            InputOrigin::Overlay {
                document_version: version
            }
        );
        if missing {
            let finding = &result.findings[0];
            assert_eq!(finding.output_field, "vAlUe");
            assert_eq!(finding.location.source, resource);
            assert_eq!(
                &draft[finding.location.native_range.clone()],
                "<field>vAlUe</field>"
            );
            assert_eq!(
                finding.location.authoring,
                QueryAuthoringRange::Exact(finding.location.native_range.clone())
            );
        } else {
            assert_eq!(
                result
                    .model
                    .parameters
                    .iter()
                    .map(|parameter| parameter.name.as_str())
                    .collect::<Vec<_>>(),
                ["Параметр", "параметр"]
            );
            for parameter in &result.model.parameters {
                assert_eq!(&text[parameter.name_range], parameter.name);
            }
        }
    }
    assert!(matches!(
        session.query_consumer_diagnostics(
            &project.source,
            stale,
            &InputRequest::default(),
            &QueryCancellation::default()
        ),
        Err(QueryConsumerRequestError::Request(
            SessionRequestError::StaleRevision { .. }
        ))
    ));
    session
        .close_query_overlay(
            &project.source,
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(current(&session, &project.source).findings.is_empty());
    assert_eq!(fixture::snapshot(&project), before);
}

/// Native names remain facts even when their output rowset cannot establish absence.
#[test]
fn incomplete_query_shapes_never_report_a_missing_consumer_field() {
    use QueryConsumerIncompleteReason as Reason;
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = AnalysisSession::open(&project.root.join("src")).unwrap();
    bind(&mut session, &project.source, "Missing");
    for (index, (text, reason)) in [
        (
            "SELECT T.* FROM Catalog.Items AS T",
            Reason::OutputNamesIncomplete,
        ),
        (
            "SELECT T.Code + T.CurrentField FROM Catalog.Items AS T",
            Reason::OutputNamesIncomplete,
        ),
        ("SELECT 1 AS", Reason::InvalidQuery),
        (
            "SELECT 1 AS СуммаA, 2 AS сУмМаa",
            Reason::AmbiguousOutputNames,
        ),
        (
            "SELECT 1 AS A UNION ALL SELECT 2 AS B, 3 AS C",
            Reason::OutputNamesIncomplete,
        ),
        (
            "SELECT 1 AS A; SELECT 2 AS B",
            Reason::MultipleOutputRowsets,
        ),
        (
            "SELECT 1 AS A INTO Temp; SELECT A FROM Temp",
            Reason::TemporaryTablePackage,
        ),
        ("", Reason::NoOutputRowset),
    ]
    .into_iter()
    .enumerate()
    {
        session
            .apply_query_overlay(
                &project.source,
                session.revision(),
                i64::try_from(index).unwrap() + 1,
                text,
                &InputRequest::default(),
            )
            .unwrap();
        let result = current(&session, &project.source);
        assert!(result.findings.is_empty(), "{text}: {:?}", result.findings);
        assert!(
            result
                .coverage
                .incomplete
                .iter()
                .any(|part| part.reason == reason),
            "{text}: {:?}",
            result.coverage
        );
    }
}

/// First-branch names define UNION outputs while native branch types remain deliberately unmerged.
#[test]
fn union_output_names_do_not_merge_secondary_branch_aliases_or_types() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = AnalysisSession::open(&project.root.join("src")).unwrap();
    bind(&mut session, &project.source, "Other");
    let text = "SELECT 1 AS Value UNION ALL SELECT \"second\" AS Other";
    session
        .apply_query_overlay(
            &project.source,
            session.revision(),
            1,
            text,
            &InputRequest::default(),
        )
        .unwrap();
    let result = current(&session, &project.source);
    assert_eq!(result.findings[0].output_field, "Other");
    assert!(result.coverage.output_names_complete);
    assert!(!result.coverage.output_types_complete);
    let QueryTextStatement::Select(select) = &result.model.statements[0] else {
        panic!("SELECT");
    };
    assert_eq!(select.branches.len(), 2);
    assert_eq!(select.branches[0].fields[0].name.as_deref(), Some("Value"));
    assert_eq!(select.branches[1].fields[0].name.as_deref(), Some("Other"));
}

/// Character folding preserves native identifier equality, including non-contextual Greek sigma.
#[test]
fn output_name_keys_preserve_greek_sigma_and_mixed_cyrillic_identifier_semantics() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = AnalysisSession::open(&project.root.join("src")).unwrap();
    let (resource, draft) = bind(&mut session, &project.source, "οσ");
    let request = InputRequest::default();
    for (index, (field, text, missing, complete)) in [
        ("οσ", "SELECT 1 AS ΟΣ", false, true),
        ("ος", "SELECT 1 AS ΟΣ", true, true),
        ("Missing", "SELECT 1 AS ΟΣ, 2 AS ος", true, true),
        ("Missing", "SELECT 1 AS ΟΣ, 2 AS οσ", false, false),
        ("сУмМаA", "SELECT 1 AS Суммаa", false, true),
        ("Missing", "SELECT 1 AS СуммаA, 2 AS сУмМаa", false, false),
    ]
    .into_iter()
    .enumerate()
    {
        let version = i64::try_from(index).unwrap() + 2;
        let body = draft.replace("<field>οσ</field>", &format!("<field>{field}</field>"));
        session
            .apply_resource_overlay(&resource, session.revision(), version, &body, &request)
            .unwrap();
        session
            .apply_query_overlay(&project.source, session.revision(), version, text, &request)
            .unwrap();
        let result = current(&session, &project.source);
        assert_eq!(!result.findings.is_empty(), missing, "{field}: {text}");
        assert_eq!(
            result.coverage.output_names_complete, complete,
            "{field}: {text}"
        );
        if !complete {
            assert!(
                result
                    .coverage
                    .incomplete
                    .iter()
                    .any(|part| part.reason == QueryConsumerIncompleteReason::AmbiguousOutputNames)
            );
        }
    }
}

/// A syntactically named projection with native resolution errors cannot establish a runtime rowset.
#[test]
fn native_semantic_errors_block_absence_but_quality_rules_do_not() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = AnalysisSession::open(&project.root.join("src")).unwrap();
    bind(&mut session, &project.source, "Missing");
    let request = InputRequest::default();
    for (index, text) in [
        "SELECT Data.Missing AS Value FROM Catalog.Items AS Data",
        "SELECT Data.Ref AS Value FROM Catalog.Missing AS Data",
        "SELECT CurrentField AS Value FROM Catalog.Items AS A INNER JOIN Catalog.Items AS B ON A.Ref=B.Ref",
        "SELECT A.Ref AS Value FROM Catalog.Items AS A INNER JOIN Catalog.Items AS A ON A.Ref=A.Ref",
    ].into_iter().enumerate() {
        session.apply_query_overlay(&project.source, session.revision(), i64::try_from(index).unwrap() + 1, text, &request).unwrap();
        let value = current(&session, &project.source);
        assert!(value.model.syntax_complete, "{text}");
        assert!(value.model.diagnostics.iter().any(crate::QueryNativeIssue::is_error), "{text}: {:?}", value.model.diagnostics);
        assert!(value.findings.is_empty(), "{text}");
        assert!(value.coverage.incomplete.iter().any(|part| part.reason == QueryConsumerIncompleteReason::InvalidQueryMetadata));
    }
    session.apply_query_overlay(&project.source, session.revision(), 8, "SELECT T.CurrentField AS Value FROM Catalog.Items AS T WHERE T.Code=\"1\" OR T.Code=\"2\"", &request).unwrap();
    let value = current(&session, &project.source);
    assert!(
        value
            .model
            .diagnostics
            .iter()
            .any(|issue| matches!(issue, crate::QueryNativeIssue::LogicalOrInWhere { .. }))
    );
    assert_eq!(value.findings.len(), 1);
}

/// Native absence of a hard diagnostic is not a proof for unknown expressions or UNION alternatives.
#[test]
fn unknown_types_in_any_branch_remain_incomplete_with_owned_native_facts() {
    use QueryConsumerIncompleteReason as Reason;

    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = AnalysisSession::open(&project.root.join("src")).unwrap();
    bind(&mut session, &project.source, "Missing");
    for (index, (text, reason)) in [
        (
            "SELECT UnknownFunction(1) AS Value",
            Reason::UnsupportedNativeExpression,
        ),
        (
            "SELECT UnknownFunction(UnknownFunction(1)) AS Value",
            Reason::UnsupportedNativeExpression,
        ),
        (
            "SELECT 1 + UnknownFunction(1) AS Value",
            Reason::UnsupportedNativeExpression,
        ),
        (
            "SELECT 1 AS Value WHERE UnknownFunction(1)=1",
            Reason::UnsupportedNativeExpression,
        ),
        (
            "SELECT 1 AS Value UNION ALL SELECT UnknownFunction(1) AS Other",
            Reason::UnsupportedNativeExpression,
        ),
        ("SELECT &Parameter AS Value", Reason::OutputTypesIncomplete),
    ]
    .into_iter()
    .enumerate()
    {
        session
            .apply_query_overlay(
                &project.source,
                session.revision(),
                i64::try_from(index).unwrap() + 1,
                text,
                &InputRequest::default(),
            )
            .unwrap();
        let result = current(&session, &project.source);
        assert!(result.model.syntax_complete, "{text}");
        assert!(result.findings.is_empty(), "{text}");
        assert!(
            result
                .coverage
                .incomplete
                .iter()
                .any(|part| part.reason == reason),
            "{text}: {:?}",
            result.coverage
        );
    }
}

/// Explicit field bindings in another dataset cannot acquire the selected query's output model.
#[test]
fn dataset_query_reference_scopes_the_checked_fields() {
    let directory = TestDirectory::new();
    let fixture = native::native(&directory.0, "configuration");
    let schema = fixture.input.join(&fixture.schema);
    let saved = fs::read_to_string(&schema).unwrap();
    let draft = saved.replace("<settingsVariant/>", "<dataSet xsi:type=\"DataSetQuery\"><name>Other</name><field xsi:type=\"DataSetFieldField\"><dataPath>Other</dataPath><field>OnlyOther</field></field><query>SELECT 3 AS OnlyOther</query></dataSet><settingsVariant/>")
        .replace("<name>Data</name>", "<name>Data</name><field xsi:type=\"DataSetFieldField\"><dataPath>Primary</dataPath><field>Primary</field></field>");
    fs::write(schema, draft).unwrap();
    let root = directory.0.join("project");
    import_query_project(
        &fixture.input,
        &root,
        ProjectName::parse("consumer-scope".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let mut session = AnalysisSession::open(&root.join("src")).unwrap();
    let source = session
        .query_paths()
        .find(|path| path.as_str().contains("Query_Data"))
        .unwrap()
        .clone();
    session
        .prepare_query_consumers(&source, session.revision(), &InputRequest::default())
        .unwrap();
    session
        .apply_query_overlay(
            &source,
            session.revision(),
            1,
            "SELECT 1 AS Value",
            &InputRequest::default(),
        )
        .unwrap();
    let result = current(&session, &source);
    assert_eq!(result.coverage.checked_bindings, 1);
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].output_field, "Primary");
    assert!(
        matches!(&result.findings[0].target, QueryConsumerTarget::Dataset { path, .. } if path == &["Data"])
    );
}

/// A real form owner supplies both native event/model XML and honest enclosing-node YAML locations.
#[test]
fn manual_dynamic_list_checks_only_direct_current_ui_bindings() {
    let directory = TestDirectory::new();
    let fixture = native::native(&directory.0, "configuration");
    let dialog = fixture.input.join(&fixture.dialog);
    let xml = fs::read_to_string(&dialog).unwrap().replace("<Table name=\"Grid\" id=\"5\"/>", "<Table name=\"Grid\" id=\"5\"><DataPath>Rows</DataPath><ChildItems><InputField name=\"Value\" id=\"6\"><DataPath>rOwS.vAlUe</DataPath></InputField><InputField name=\"Other\" id=\"7\"><DataPath>Object.Other</DataPath></InputField><InputField name=\"Nested\" id=\"8\"><DataPath>Rows.Ref.Name</DataPath></InputField></ChildItems></Table>");
    fs::write(dialog, xml).unwrap();
    let root = directory.0.join("project");
    import_query_project(
        &fixture.input,
        &root,
        ProjectName::parse("list-consumers".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let mut session = AnalysisSession::open(&root.join("src")).unwrap();
    let source = session
        .query_paths()
        .find(|path| path.as_str().contains("Attribute_"))
        .unwrap()
        .clone();
    session
        .prepare_query_consumers(&source, session.revision(), &InputRequest::default())
        .unwrap();
    let request = InputRequest::default();
    session
        .apply_query_overlay(
            &source,
            session.revision(),
            1,
            "SELECT 1 AS Value",
            &request,
        )
        .unwrap();
    let good = current(&session, &source);
    assert!(good.findings.is_empty());
    assert_eq!(good.coverage.checked_bindings, 1);
    assert!(
        good.coverage
            .incomplete
            .iter()
            .any(|part| part.reason == QueryConsumerIncompleteReason::UnsupportedFieldPath)
    );
    session
        .apply_query_overlay(
            &source,
            session.revision(),
            2,
            "SELECT 1 AS Renamed",
            &request,
        )
        .unwrap();
    let missing = current(&session, &source);
    assert_eq!(missing.findings.len(), 1);
    let finding = &missing.findings[0];
    assert_eq!(finding.output_field, "vAlUe");
    assert!(matches!(
        finding.target,
        QueryConsumerTarget::DynamicList { .. }
    ));
    let QueryAuthoringRange::EnclosingNode(range) = &finding.location.authoring else {
        panic!("honest YAML node precision");
    };
    let owner = root.join("src").join(finding.location.source.as_path());
    let saved = fs::read_to_string(&owner).unwrap();
    assert!(saved[range.clone()].contains("DataPath"));
    let changed = saved.replace("rOwS.vAlUe", "Rows.Renamed");
    session
        .apply_overlay(
            &finding.location.source,
            session.revision(),
            1,
            &changed,
            &request,
        )
        .unwrap();
    assert!(current(&session, &source).findings.is_empty());
    assert_eq!(fs::read_to_string(owner).unwrap(), saved);
}

/// Failed inputs, cancelled requests and missing external base context never expose an old finding set.
#[test]
fn consumers_refuse_failed_current_authority_stale_quota_and_external_context() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = AnalysisSession::open(&project.root.join("src")).unwrap();
    let (resource, _) = bind(&mut session, &project.source, "Missing");
    assert_eq!(current(&session, &project.source).findings.len(), 1);
    let request = InputRequest::default();
    let cancellation = QueryCancellation::default();
    cancellation.cancel();
    assert!(matches!(
        session.query_consumer_diagnostics(
            &project.source,
            session.revision(),
            &request,
            &cancellation
        ),
        Err(QueryConsumerRequestError::Native(QueryError::Cancelled))
    ));
    session
        .apply_resource_overlay(&resource, session.revision(), 2, "invalid XML", &request)
        .unwrap();
    assert!(matches!(
        session.query_consumer_diagnostics(
            &project.source,
            session.revision(),
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryConsumerRequestError::Context(
            QueryContextRequestError::Source(_)
        ))
    ));
    session.options.max_retained_source_bytes = 0;
    session
        .apply_query_overlay(
            &project.source,
            session.revision(),
            1,
            "SELECT 1 AS Value",
            &request,
        )
        .unwrap();
    assert!(matches!(
        session.query_consumer_diagnostics(
            &project.source,
            session.revision(),
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryConsumerRequestError::Context(
            QueryContextRequestError::Unavailable {
                failure: InputFailure::SourceBudgetExceeded { .. },
                ..
            }
        ))
    ));
    for kind in ["processing", "report"] {
        let directory = TestDirectory::new();
        let project = fixture::project(&directory.0, kind);
        let mut session = AnalysisSession::open(&project.root.join("src")).unwrap();
        let before = session.resources();
        assert!(matches!(
            session.prepare_query_consumers(&project.source, session.revision(), &request),
            Err(QueryConsumerRequestError::Request(
                SessionRequestError::ConfigurationContextRequired
            ))
        ));
        assert_eq!(session.resources(), before);
        assert!(
            session
                .selected_root
                .inputs
                .owner(&project.catalog)
                .is_none()
        );
    }
}

#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
mod composition;
