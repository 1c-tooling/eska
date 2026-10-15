//! Native configured managers and Find results follow current canonical criteria without XML fallback.

use super::*;
use crate::{
    Locale, QueryCancellation, QueryError,
    session::tests::{owner, support::TestDirectory},
};
use std::{collections::BTreeSet, fs, path::PathBuf};

const MODULE: &str = "CommonModules/Tools/Module.bsl";
const CRITERION: &str = "FilterCriteria/Linked/object.yaml";

/// Each imported source belongs exclusively to the test and starts with cold canonical inputs.
fn project(body: &str) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    crate::test_filter_criterion_project::add_criteria(
        &root,
        &directory.0.join("native"),
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let source = root.join("src-with-criteria");
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_module_saved(&owner(MODULE), session.revision(), &InputRequest::default())
        .unwrap();
    (directory, source, session)
}

/// Exercise public cold preparation and completion together, as the client does.
fn labels(
    session: &mut AnalysisSession,
    body: &str,
    needle: &str,
) -> Result<BTreeSet<String>, QueryError> {
    let offset = u32::try_from(body.rfind(needle).unwrap() + needle.len()).unwrap();
    session.prepare_completion_metadata(
        &owner(MODULE),
        session.revision(),
        offset,
        &InputRequest::default(),
        &QueryCancellation::default(),
    )?;
    let result = session.module_completions(
        &owner(MODULE),
        session.revision(),
        offset,
        Locale::En,
        &QueryCancellation::default(),
    )?;
    Ok(result.value.into_iter().map(|item| item.label).collect())
}

/// The actual platform namespace and generated manager preserve bilingual callable native methods.
#[test]
fn criterion_cold_namespace_and_generated_manager_methods_are_native() {
    for (access, expected) in [
        ("FilterCriteria.", "Linked"),
        ("КритерииОтбора.", "Linked"),
        ("FilterCriteria.Linked.", "Найти"),
        ("КритерииОтбора.Linked.", "Найти"),
    ] {
        let body = format!("Procedure Run()\n Value = {access};\nEndProcedure\n");
        let (_directory, _source, mut session) = project(&body);
        let result = labels(&mut session, &body, access).unwrap();
        assert!(result.contains(expected), "{access}: {result:?}");
        assert!(
            !result
                .iter()
                .any(|name| name.contains("&lt;") || name.contains("<Criterion"))
        );
    }
}

/// A changed Content selects different reference fields without changing or reloading the BSL body.
#[test]
fn criterion_find_array_elements_follow_dirty_content_and_saved_close() {
    let body = "\u{feff}// Я😀\r\nProcedure Run()\r\n Rows = FilterCriteria.Linked.Find(\"key\");\r\n Value = Rows[0].Choice;\r\nEndProcedure\r\n";
    let (_directory, source, mut session) = project(body);
    let result = labels(&mut session, body, "Rows[0].").unwrap();
    assert!(result.contains("Choice"), "{result:?}");
    assert!(!result.contains("Other"));
    let criterion = owner(CRITERION);
    let saved = fs::read_to_string(source.join(criterion.as_path())).unwrap();
    let changed = saved.replace(
        "Catalog.Items.Attribute.Key",
        "Document.Orders.Attribute.Key",
    );
    assert_ne!(saved, changed);
    session
        .apply_overlay(
            &criterion,
            session.revision(),
            1,
            &changed,
            &InputRequest::default(),
        )
        .unwrap();
    let result = labels(&mut session, body, "Rows[0].").unwrap();
    assert!(
        result.contains("Other") && !result.contains("Choice"),
        "{result:?}"
    );
    assert!(
        session
            .invalidate_saved_inputs(std::slice::from_ref(&criterion), session.revision())
            .unwrap()
            .is_empty()
    );
    session
        .apply_overlay(
            &criterion,
            session.revision(),
            2,
            "broken: [",
            &InputRequest::default(),
        )
        .unwrap();
    assert!(labels(&mut session, body, "Rows[0].").is_err());
    session
        .apply_overlay(
            &criterion,
            session.revision(),
            3,
            &changed,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        labels(&mut session, body, "Rows[0].")
            .unwrap()
            .contains("Other")
    );
    session
        .close_overlay(&criterion, session.revision(), &InputRequest::default())
        .unwrap();
    assert!(
        labels(&mut session, body, "Rows[0].")
            .unwrap()
            .contains("Choice")
    );
    assert_eq!(
        fs::read_to_string(source.join(criterion.as_path())).unwrap(),
        saved
    );
    assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), body);
}

/// Native call checking consumes the configured search type and result-reference fields.
#[test]
fn criterion_find_search_type_reaches_native_diagnostics() {
    let body = "Procedure Run()\n Rows=FilterCriteria.Linked.Find(\"key\");\n Value=Rows[0].Absent;\nEndProcedure\n";
    let (_directory, source, mut session) = project(body);
    session
        .load_semantic_inputs(session.revision(), &InputRequest::default())
        .unwrap();
    let findings = |session: &AnalysisSession| {
        session
            .module_diagnostics(
                &owner(MODULE),
                session.revision(),
                &crate::DiagnosticsConfig::default(),
                &QueryCancellation::default(),
            )
            .unwrap()
            .value
    };
    let first = findings(&session);
    assert!(
        first
            .iter()
            .any(|finding| finding.code == crate::DiagnosticCode::UnresolvedField),
        "{first:?}"
    );
    assert!(
        !first
            .iter()
            .any(|finding| finding.code == crate::DiagnosticCode::TypeMismatch),
        "{first:?}"
    );
    let saved = fs::read_to_string(source.join(CRITERION)).unwrap();
    let changed = saved.replace("x:string", "x:boolean");
    session
        .apply_overlay(
            &owner(CRITERION),
            session.revision(),
            1,
            &changed,
            &InputRequest::default(),
        )
        .unwrap();
    let current = findings(&session);
    assert!(
        current
            .iter()
            .any(|finding| finding.code == crate::DiagnosticCode::TypeMismatch),
        "{current:?}"
    );
    session
        .close_overlay(
            &owner(CRITERION),
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        !findings(&session)
            .iter()
            .any(|finding| finding.code == crate::DiagnosticCode::TypeMismatch)
    );
}

/// Shadowing, comments and other procedure bodies cannot trigger criterion-family loading.
#[test]
fn criterion_discovery_respects_native_shadowing_and_enclosing_body() {
    for body in [
        "Procedure Run(FilterCriteria)\n Value=FilterCriteria.Linked;\nEndProcedure\n",
        "Procedure Run()\n Rows=New Structure;\n Value=Rows.Choice;\nEndProcedure\nProcedure Other()\n Rows=FilterCriteria.Linked.Find(\"key\");\nEndProcedure\n",
        "Procedure Run()\n // FilterCriteria.Linked.Find(\"key\")\n Rows=New Structure;\n Value=Rows.Choice;\nEndProcedure\n",
    ] {
        let (_directory, _source, mut session) = project(body);
        let needle = if body.contains("Value=FilterCriteria") {
            "FilterCriteria."
        } else {
            "Rows."
        };
        // Use the completion site in Run, not an incidental receiver in another procedure.
        let offset = u32::try_from(body.find(needle).unwrap() + needle.len()).unwrap();
        session
            .prepare_completion_metadata(
                &owner(MODULE),
                session.revision(),
                offset,
                &InputRequest::default(),
                &QueryCancellation::default(),
            )
            .unwrap();
        assert!(matches!(
            session.owner(&owner(CRITERION)).unwrap().state,
            ModelState::Pending
        ));
    }
}

/// The same native call binding serves signature help after cold family preparation.
#[test]
fn criterion_find_signature_and_hover_use_current_configured_call() {
    let body = "Procedure Run()\n Rows=FilterCriteria.Linked.Find(\"key\");\nEndProcedure\n";
    let (_directory, _source, mut session) = project(body);
    let offset = u32::try_from(body.find("\"key\"").unwrap()).unwrap();
    session
        .prepare_signature_help_metadata(
            &owner(MODULE),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    let signature = session
        .module_signature_help(
            &owner(MODULE),
            session.revision(),
            offset,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .expect("native Find signature");
    assert_eq!(signature.signatures[0].parameters.len(), 1);
    assert!(
        signature.signatures[0].label.contains("Найти"),
        "{signature:?}"
    );
    assert!(
        signature.signatures[0].parameters[0]
            .label
            .contains("Строка"),
        "{signature:?}"
    );
    assert!(
        !signature.signatures[0].label.contains("&lt;"),
        "{signature:?}"
    );
    let offset = u32::try_from(body.find("Find").unwrap()).unwrap();
    session
        .prepare_symbol_hover_metadata(
            &owner(MODULE),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    let card = session
        .module_symbol_hover(
            &owner(MODULE),
            session.revision(),
            offset,
            Locale::En,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .expect("native Find hover");
    assert_eq!(card.kind, crate::HoverKind::PlatformFunction);
    assert!(
        card.return_type
            .as_deref()
            .is_some_and(|value| value.contains("Array")),
        "{card:?}"
    );
}

/// Explicit source configuration admits only the selected CFE and its declared dependencies.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
fn composition(body: &str) -> (TestDirectory, PathBuf, PathBuf, AnalysisSession) {
    use eska::project_core::source_format::SourceReader;
    let directory = TestDirectory::new();
    let create = |name, extension, uuid, extra| {
        let directory = directory.0.join(name);
        let root = crate::test_project::project(&directory, body, false);
        crate::test_filter_criterion_project::add_criteria(
            &root,
            &directory.join("native"),
            extension,
            uuid,
            extra,
        );
        root
    };
    let base = create("base", false, "70000000-0000-4000-8000-000000000001", "");
    let selected = create(
        "selected",
        true,
        "70000000-0000-4000-8000-000000000002",
        "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>70000000-0000-4000-8000-000000000001</ExtendedConfigurationObject><Content><r:Item xsi:type='r:MDObjectRef'>Document.Orders.Attribute.Key</r:Item></Content>",
    );
    let hidden = create("hidden", true, "70000000-0000-4000-8000-000000000003", "");
    let settings_file = selected.join("bsl-analyzer.toml");
    fs::write(&settings_file, "[source]\nroot='../../base/project/src-with-criteria'\nextensions=[{name='Selected',path='src-with-criteria'},{name='Hidden',path='../../hidden/project/src-with-criteria'}]\n").unwrap();
    let settings = project_model::ProjectConfig::load_from_file(&settings_file).unwrap();
    let readers = [&base, &selected, &hidden]
        .map(|root| SourceReader::open(&root.join("src-with-criteria")).unwrap())
        .into_iter()
        .collect();
    let topology = crate::composition::admit(
        &selected,
        &settings,
        readers,
        &selected.join("src-with-criteria"),
    )
    .unwrap();
    let session =
        AnalysisSession::from_composition(topology, crate::SessionOptions::default()).unwrap();
    (directory, base, selected, session)
}

/// Invalid base input, a wrong original UUID or changed borrowed Type cannot expose stale or hidden criteria.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
#[test]
fn criterion_cfe_content_inherits_current_base_and_refuses_incompatible_adoption() {
    use crate::composition::SourceKey;
    let body = "Procedure Run()\n Value=FilterCriteria.Linked;\nEndProcedure\n";
    let (_directory, base, selected, mut session) = composition(body);
    session
        .load_module_saved(&owner(MODULE), session.revision(), &InputRequest::default())
        .unwrap();
    assert!(
        labels(&mut session, body, "FilterCriteria.")
            .unwrap()
            .contains("Linked")
    );
    let file = session.selected_root.modules[&owner(MODULE)].file;
    let current = session
        .database
        .resolve_filter_criterion_for_file(file, "Linked")
        .unwrap();
    assert_eq!(current.content().unwrap().len(), 2);
    assert!(matches!(
        current.search_type(),
        Some(bsl_metadata::AttributeType::String { .. })
    ));
    let hidden = session.root_ids().find(|root| root.index() == 2).unwrap();
    assert!(matches!(
        session.root_inputs(hidden).unwrap().owners[&owner(CRITERION)]
            .model
            .state,
        ModelState::Pending
    ));
    let base_id = session.root_ids().find(|root| root.index() == 0).unwrap();
    let key = SourceKey {
        root: base_id,
        source: owner(CRITERION),
    };
    session
        .apply_overlay_at(
            &key,
            session.revision(),
            1,
            "broken: [",
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        matches!(labels(&mut session, body, "FilterCriteria."), Err(QueryError::Request(SessionRequestError::CompositionInputIncomplete(failed))) if failed == key)
    );
    session
        .close_overlay_at(&key, session.revision(), &InputRequest::default())
        .unwrap();
    let saved = fs::read_to_string(selected.join("src-with-criteria").join(CRITERION)).unwrap();
    let wrong = saved.replace(
        "70000000-0000-4000-8000-000000000001",
        "70000000-0000-4000-8000-000000000999",
    );
    for (index, changed) in [
        wrong,
        saved.replace(
            "<UseStandardCommands>",
            "<Type><t:Type>x:boolean</t:Type></Type><UseStandardCommands>",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        session
            .apply_overlay(
                &owner(CRITERION),
                session.revision(),
                i64::try_from(index + 1).unwrap(),
                &changed,
                &InputRequest::default(),
            )
            .unwrap();
        assert!(labels(&mut session, body, "FilterCriteria.").is_err());
        require_semantic_refusal(&session);
    }
    session
        .close_overlay(
            &owner(CRITERION),
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        labels(&mut session, body, "FilterCriteria.")
            .unwrap()
            .contains("Linked")
    );
    assert!(
        fs::read_to_string(base.join("src-with-criteria").join(CRITERION))
            .unwrap()
            .contains("x:string")
    );
}

/// Semantic failure must leave the explicitly independent syntax route usable.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
fn require_semantic_refusal(session: &AnalysisSession) {
    assert!(
        session
            .module_diagnostics(
                &owner(MODULE),
                session.revision(),
                &crate::DiagnosticsConfig::default(),
                &QueryCancellation::default()
            )
            .is_err()
    );
    session
        .module_syntax_diagnostics(
            &owner(MODULE),
            session.revision(),
            &crate::DiagnosticsConfig::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
}
