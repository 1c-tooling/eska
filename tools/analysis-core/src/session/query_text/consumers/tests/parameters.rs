//! Current schema declarations drive native parameter types without changing their query input.

use super::*;

/// Primitive `QName` spellings are resolved at the exact current XML declaration.
fn declaration(name: &str, ty: &str, extra: &str) -> String {
    format!(
        "<parameter xmlns:t=\"http://v8.1c.ru/8.1/data/core\" xmlns:q=\"http://www.w3.org/2001/XMLSchema\"><name>{name}</name><valueType>{ty}</valueType>{extra}</parameter>"
    )
}

/// A standalone CF uses exactly the same native parameter context as a selected extension.
#[test]
fn explicit_current_parameter_types_follow_resource_revision_in_cf() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let session = AnalysisSession::open(&project.root.join("src")).unwrap();
    current_parameter_types(session, &project.source);
}

/// Changing only the current resource enables the existing absence proof without changing the query.
pub(super) fn current_parameter_types(mut session: AnalysisSession, source: &SourcePath) {
    let (resource, draft) = bind(&mut session, source, "Value");
    let request = InputRequest::default();
    session
        .apply_query_overlay(
            source,
            session.revision(),
            1,
            "\u{feff}// Я😀\r\nSELECT &параметр AS Renamed",
            &request,
        )
        .unwrap();
    let query_revision = session.query_input(source).unwrap().revision;
    let initial = current(&session, source);
    assert_eq!(initial.model.parameters[0].declared_type, None);
    assert!(initial.findings.is_empty());
    assert!(!initial.coverage.output_types_complete);
    for (version, declaration, expected) in [
        (
            2,
            declaration(
                "Параметр",
                "<t:Type>q:boolean</t:Type>",
                "<valueListAllowed>false</valueListAllowed>",
            ),
            Some(bsl_metadata::AttributeType::Boolean),
        ),
        (
            3,
            declaration(
                "Параметр",
                "<t:Type>q:string</t:Type><t:StringQualifiers><t:Length>40</t:Length><t:AllowedLength>Variable</t:AllowedLength></t:StringQualifiers>",
                "",
            ),
            Some(bsl_metadata::AttributeType::String { length: Some(40) }),
        ),
        (
            4,
            declaration(
                "Параметр",
                "<t:TypeId>10000000-0000-4000-8000-000000000001</t:TypeId>",
                "",
            ),
            None,
        ),
        (
            5,
            declaration(
                "Параметр",
                "<t:Type>q:boolean</t:Type>",
                "<valueListAllowed>true</valueListAllowed>",
            ),
            None,
        ),
        (6, String::new(), None),
    ] {
        let text = draft.replace(
            "</DataCompositionSchema>",
            &format!("{declaration}</DataCompositionSchema>"),
        );
        session
            .apply_resource_overlay(&resource, session.revision(), version, &text, &request)
            .unwrap();
        let result = current(&session, source);
        assert_eq!(result.model.parameters[0].declared_type, expected);
        assert_eq!(result.coverage.output_types_complete, expected.is_some());
        assert_eq!(result.findings.len(), usize::from(expected.is_some()));
        assert_eq!(
            session.query_input(source).unwrap().revision,
            query_revision
        );
        assert_eq!(
            result.context.resource.origin,
            InputOrigin::Overlay {
                document_version: version
            }
        );
        assert!(!result.coverage.parameters_complete());
    }
}

/// An unsupported duplicate cannot let a well-typed declaration win; an unrelated name grants no type.
#[test]
fn parameter_ambiguity_unknown_names_and_rebound_namespaces_never_supply_a_fallback_type() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = AnalysisSession::open(&project.root.join("src")).unwrap();
    let (resource, draft) = bind(&mut session, &project.source, "Value");
    let request = InputRequest::default();
    session
        .apply_query_overlay(
            &project.source,
            session.revision(),
            1,
            "SELECT &P AS Renamed",
            &request,
        )
        .unwrap();
    for (version, declarations) in [
        (
            2,
            format!(
                "{}{}",
                declaration("P", "<t:Type>q:boolean</t:Type>", ""),
                declaration("p", "<t:Unknown/>", "")
            ),
        ),
        (
            3,
            declaration("Unrelated", "<t:Type>q:boolean</t:Type>", ""),
        ),
        (
            4,
            declaration(
                "P",
                "<t:Type xmlns:q=\"urn:foreign\">q:boolean</t:Type>",
                "",
            ),
        ),
    ] {
        let text = draft.replace(
            "</DataCompositionSchema>",
            &format!("{declarations}</DataCompositionSchema>"),
        );
        session
            .apply_resource_overlay(&resource, session.revision(), version, &text, &request)
            .unwrap();
        let result = current(&session, &project.source);
        assert_eq!(result.model.parameters[0].declared_type, None);
        assert!(!result.coverage.output_types_complete);
        assert!(result.findings.is_empty());
    }
}
