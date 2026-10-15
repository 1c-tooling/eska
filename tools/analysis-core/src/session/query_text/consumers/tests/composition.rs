//! Explicit source topology anchors current output types and refuses invalid foreign contributions.

use super::*;
use crate::SessionOptions;
use crate::composition::{SourceKey, admit};
use eska::{analysis_core::InputState, project_core::source_format::SourceReader};

/// The selected extension sees base+A but cannot resolve an unrelated same-path sibling field.
#[test]
fn consumer_native_model_uses_selected_composition_owner_and_current_foreign_inputs() {
    let directory = TestDirectory::new();
    let (project, foreign, catalog) = fixture::composition(&directory.0);
    let settings =
        project_model::ProjectConfig::load_from_file(&project.root.join("bsl-analyzer.toml"))
            .unwrap();
    let roots = std::iter::once(SourceReader::open(&project.root.join("src")).unwrap())
        .chain(
            ["base", "a", "sibling"]
                .map(|name| SourceReader::open(&directory.0.join(name).join("src")).unwrap()),
        )
        .collect();
    let composition = admit(&project.root, &settings, roots, &project.root.join("src")).unwrap();
    let mut session =
        AnalysisSession::from_composition(composition, SessionOptions::default()).unwrap();
    bind(&mut session, &project.source, "Missing");
    let request = InputRequest::default();
    session
        .apply_query_overlay(
            &project.source,
            session.revision(),
            1,
            "SELECT Data.BaseField AS Base, Data.AField AS Extension FROM Catalog.Items AS Data",
            &request,
        )
        .unwrap();
    let result = current(&session, &project.source);
    assert_eq!(result.context.root.index(), 2);
    assert_eq!(
        result.context.composition_sha256.as_deref(),
        session.composition_sha256()
    );
    assert_eq!(result.findings.len(), 1);
    assert!(result.coverage.output_types_complete);
    let QueryTextStatement::Select(select) = &result.model.statements[0] else {
        panic!("SELECT");
    };
    assert!(
        select.branches[0]
            .fields
            .iter()
            .all(|field| field.type_complete)
    );
    let sibling = session.root_ids().find(|id| id.index() == 3).unwrap();
    assert!(
        session
            .root_inputs(sibling)
            .unwrap()
            .inputs
            .owner_paths()
            .all(|path| matches!(
                session
                    .root_inputs(sibling)
                    .unwrap()
                    .inputs
                    .owner(path)
                    .unwrap()
                    .state,
                InputState::Pending
            ))
    );
    let key = SourceKey {
        root: session.root_ids().find(|id| id.index() == 1).unwrap(),
        source: catalog,
    };
    let saved = fs::read_to_string(foreign.join("src").join(key.source.as_path())).unwrap();
    let stale = session.revision();
    session
        .apply_overlay_at(&key, stale, 1, "broken: [", &request)
        .unwrap();
    assert!(matches!(
        session.query_consumer_diagnostics(
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
        .apply_overlay_at(&key, session.revision(), 2, &saved, &request)
        .unwrap();
    assert_eq!(current(&session, &project.source).findings.len(), 1);
    assert!(matches!(
        session.query_consumer_diagnostics(
            &project.source,
            stale,
            &request,
            &QueryCancellation::default()
        ),
        Err(QueryConsumerRequestError::Request(
            SessionRequestError::StaleRevision { .. }
        ))
    ));
}

/// A selected CFE reads explicit schema parameter types from its current resource in the admitted context.
#[test]
fn explicit_current_parameter_types_follow_resource_revision_in_selected_cfe() {
    let directory = TestDirectory::new();
    let (project, _, _) = fixture::composition(&directory.0);
    let settings =
        project_model::ProjectConfig::load_from_file(&project.root.join("bsl-analyzer.toml"))
            .unwrap();
    let roots = std::iter::once(SourceReader::open(&project.root.join("src")).unwrap())
        .chain(
            ["base", "a", "sibling"]
                .map(|name| SourceReader::open(&directory.0.join(name).join("src")).unwrap()),
        )
        .collect();
    let composition = admit(&project.root, &settings, roots, &project.root.join("src")).unwrap();
    let session =
        AnalysisSession::from_composition(composition, SessionOptions::default()).unwrap();
    super::parameters::current_parameter_types(session, &project.source);
}

/// A selected CFE contributes current common fields to its visible base query and the same consumer report.
#[test]
fn current_common_fields_reach_selected_cfe_query_consumers() {
    let directory = TestDirectory::new();
    let (project, _, _) = fixture::composition_with_common_attribute(&directory.0);
    let settings =
        project_model::ProjectConfig::load_from_file(&project.root.join("bsl-analyzer.toml"))
            .unwrap();
    let roots = std::iter::once(SourceReader::open(&project.root.join("src")).unwrap())
        .chain(
            ["base", "a", "sibling"]
                .map(|name| SourceReader::open(&directory.0.join(name).join("src")).unwrap()),
        )
        .collect();
    let topology = admit(&project.root, &settings, roots, &project.root.join("src")).unwrap();
    let session = AnalysisSession::from_composition(topology, SessionOptions::default()).unwrap();
    super::common_attributes::check(&project, session);
}
