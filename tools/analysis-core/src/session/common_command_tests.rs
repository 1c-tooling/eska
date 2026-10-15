//! Command parameter inference consumes the exact current YAML and declared native body.

use super::*;
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
use crate::{DiagnosticsConfig, QueryError};
use crate::{
    Locale, QueryCancellation,
    session::tests::{owner, support::TestDirectory},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MODULE: &str = "CommonCommands/Run/CommandModule.bsl";
const COMMAND: &str = "CommonCommands/Run/command.yaml";
const BODY: &str = "\u{feff}// Я😀\r\n&AtClient\r\nProcedure CommandProcessing(Chosen, Context)\r\n Result = Chosen;\r\nEndProcedure\r\nProcedure Helper(Chosen, Context)\r\n Result = Chosen;\r\nEndProcedure\r\n";

/// Fresh native imports keep all filesystem effects inside this test's exclusive directory.
fn project(directory: &Path, extension: bool, uuid: &str, extra: &str) -> PathBuf {
    let root = crate::test_project::project(directory, "// independent common module\n", false);
    crate::test_command_project::add_commands(
        &root,
        &directory.join("native"),
        BODY,
        extension,
        uuid,
        extra,
    );
    root
}

/// Prepare the native owner cold, then ask the same public hover API used by IDE clients.
fn parameter(session: &mut AnalysisSession, locale: Locale, helper: bool) -> Option<String> {
    let offset = u32::try_from(if helper {
        BODY.rfind("Chosen").unwrap()
    } else {
        BODY.find("Result = Chosen").unwrap() + "Result = ".len()
    })
    .unwrap();
    session
        .prepare_symbol_hover_metadata(
            &owner(MODULE),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    let file = session.selected_root.modules[&owner(MODULE)].file;
    let current = session.database.common_command_for_file(file).unwrap();
    assert!(current.is_some(), "current owner {current:?}");
    let metadata = hir::DefDatabase::module_metadata(&session.database, hir::ModuleId::new(file));
    assert!(metadata.common_command.is_some(), "module {metadata:?}");
    session
        .module_symbol_hover(
            &owner(MODULE),
            session.revision(),
            offset,
            locale,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .unwrap()
        .return_type
}

/// Type/mode edits leave the BSL unchanged; watch cannot replace dirty authority and close reads saved bytes.
#[test]
fn current_common_command_parameter_types_modes_invalid_repair_and_close_reach_native_hover() {
    let directory = TestDirectory::new();
    let root = project(
        &directory.0,
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let source = root.join("src-with-common-commands");
    let mut session = AnalysisSession::open(&source).unwrap();
    let request = InputRequest::default();
    session
        .load_module_saved(&owner(MODULE), session.revision(), &request)
        .unwrap();
    let module_revision = session.module(&owner(MODULE)).unwrap().revision;
    assert!(matches!(
        session.owner(&owner(COMMAND)).unwrap().state,
        ModelState::Pending
    ));
    for (locale, expected) in [(Locale::En, "String"), (Locale::Ru, "Строка")] {
        assert_eq!(
            parameter(&mut session, locale, false).as_deref(),
            Some(expected)
        );
    }
    assert_eq!(parameter(&mut session, Locale::En, true), None);
    assert!(matches!(
        session
            .owner(&owner("CommonCommands/Other/command.yaml"))
            .unwrap()
            .state,
        ModelState::Pending
    ));
    let command = owner(COMMAND);
    let saved = fs::read_to_string(source.join(command.as_path())).unwrap();
    let bytes = fs::read(source.join(command.as_path())).unwrap();
    let changed = saved.replace("x:string", "x:boolean");
    session
        .apply_overlay(&command, session.revision(), 1, &changed, &request)
        .unwrap();
    for (locale, expected) in [(Locale::En, "Boolean"), (Locale::Ru, "Булево")] {
        assert_eq!(
            parameter(&mut session, locale, false).as_deref(),
            Some(expected)
        );
    }
    assert!(
        session
            .invalidate_saved_inputs(std::slice::from_ref(&command), session.revision())
            .unwrap()
            .is_empty()
    );
    let multiple = changed.replace(
        "<ParameterUseMode>Single</ParameterUseMode>",
        "<ParameterUseMode>Multiple</ParameterUseMode>",
    );
    assert_ne!(multiple, changed);
    session
        .apply_overlay(&command, session.revision(), 2, &multiple, &request)
        .unwrap();
    let ty = parameter(&mut session, Locale::En, false).unwrap();
    assert!(ty.contains("Array") && ty.contains("Boolean"), "{ty}");
    session
        .apply_overlay(&command, session.revision(), 3, "broken: [", &request)
        .unwrap();
    assert!(
        session
            .resolve(&command, session.revision())
            .unwrap()
            .is_none()
    );
    let file = session.selected_root.modules[&owner(MODULE)].file;
    assert!(session.database.common_command_for_file(file).is_err());
    session
        .apply_overlay(&command, session.revision(), 4, &changed, &request)
        .unwrap();
    assert_eq!(
        parameter(&mut session, Locale::En, false).as_deref(),
        Some("Boolean")
    );
    session
        .close_overlay(&command, session.revision(), &request)
        .unwrap();
    assert_eq!(
        parameter(&mut session, Locale::En, false).as_deref(),
        Some("String")
    );
    assert_eq!(fs::read(source.join(command.as_path())).unwrap(), bytes);
    assert_eq!(
        session.module(&owner(MODULE)).unwrap().revision,
        module_revision
    );
}

/// The platform procedure is recognized in both languages; helpers, functions and invalid arity stay untyped.
#[test]
fn command_parameter_inference_requires_the_predefined_procedure_and_nonempty_type() {
    let directory = TestDirectory::new();
    let root = project(
        &directory.0,
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let source = root.join("src-with-common-commands");
    let mut session = AnalysisSession::open(&source).unwrap();
    let request = InputRequest::default();
    for (index, (header, end, known)) in [
        (
            "Процедура ОбработкаКоманды(Chosen, Context)",
            "КонецПроцедуры",
            true,
        ),
        (
            "Procedure cOmMaNdPrOcEsSiNg(Chosen, Context)",
            "EndProcedure",
            true,
        ),
        ("Procedure Helper(Chosen, Context)", "EndProcedure", false),
        (
            "Function CommandProcessing(Chosen, Context)",
            "EndFunction",
            false,
        ),
        ("Procedure CommandProcessing(Chosen)", "EndProcedure", false),
    ]
    .iter()
    .enumerate()
    {
        let body = format!("{header}\n Result=Chosen;\n{end}\n");
        session
            .apply_module_overlay(
                &owner(MODULE),
                session.revision(),
                i64::try_from(index + 1).unwrap(),
                &body,
                &request,
            )
            .unwrap();
        let offset = u32::try_from(body.rfind("Chosen").unwrap()).unwrap();
        session
            .prepare_symbol_hover_metadata(
                &owner(MODULE),
                session.revision(),
                offset,
                &request,
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
            .unwrap();
        assert_eq!(
            card.return_type.as_deref(),
            if *known { Some("String") } else { None },
            "{body}"
        );
    }
    session
        .apply_module_overlay(&owner(MODULE), session.revision(), 6, BODY, &request)
        .unwrap();
    let saved = fs::read_to_string(source.join(COMMAND)).unwrap();
    let empty = saved.replace("<t:Type>x:string</t:Type>", "").replace(
        "<ParameterUseMode>Single</ParameterUseMode>",
        "<ParameterUseMode>Multiple</ParameterUseMode>",
    );
    assert_ne!(empty, saved);
    session
        .apply_overlay(&owner(COMMAND), session.revision(), 1, &empty, &request)
        .unwrap();
    assert_eq!(parameter(&mut session, Locale::En, false), None);
}

/// The same native receiver discovery prepares a cold reference target and preserves multiple-selection elements.
#[test]
fn current_command_reference_parameter_prepares_native_catalog_completion_and_array_elements() {
    let directory = TestDirectory::new();
    let root = project(
        &directory.0,
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let source = root.join("src-with-common-commands");
    let mut session = AnalysisSession::open(&source).unwrap();
    let request = InputRequest::default();
    let saved = fs::read_to_string(source.join(COMMAND)).unwrap();
    let reference = saved.replace("x:string", "cfg:CatalogRef.Items");
    assert_ne!(reference, saved);
    session
        .apply_overlay(&owner(COMMAND), session.revision(), 1, &reference, &request)
        .unwrap();
    assert!(matches!(
        session
            .owner(&owner("Catalogs/Items/object.yaml"))
            .unwrap()
            .state,
        ModelState::Pending
    ));
    for (index, receiver) in ["Chosen.", "Chosen[0]."].iter().enumerate() {
        if index == 1 {
            session
                .apply_overlay(
                    &owner(COMMAND),
                    session.revision(),
                    2,
                    &reference.replace(
                        "<ParameterUseMode>Single</ParameterUseMode>",
                        "<ParameterUseMode>Multiple</ParameterUseMode>",
                    ),
                    &request,
                )
                .unwrap();
        }
        let body =
            format!("Procedure CommandProcessing(Chosen,Context)\n {receiver}\nEndProcedure\n");
        let offset = u32::try_from(body.find(receiver).unwrap() + receiver.len()).unwrap();
        session
            .apply_module_overlay(
                &owner(MODULE),
                session.revision(),
                i64::try_from(index + 1).unwrap(),
                &body,
                &request,
            )
            .unwrap();
        session
            .prepare_completion_metadata(
                &owner(MODULE),
                session.revision(),
                offset,
                &request,
                &QueryCancellation::default(),
            )
            .unwrap();
        for locale in [Locale::En, Locale::Ru] {
            let completion = session
                .module_completions(
                    &owner(MODULE),
                    session.revision(),
                    offset,
                    locale,
                    &QueryCancellation::default(),
                )
                .unwrap();
            assert!(
                completion.value.iter().any(|item| item.label == "Choice"),
                "{receiver} {completion:?}"
            );
        }
    }
    assert!(matches!(
        session
            .owner(&owner("Catalogs/Items/object.yaml"))
            .unwrap()
            .state,
        ModelState::Ready(_)
    ));
}

/// Enroll three independently imported roots through the real configured source contract.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
fn composition() -> (TestDirectory, PathBuf, PathBuf, AnalysisSession) {
    use crate::composition::admit;
    use eska::project_core::source_format::SourceReader;
    let directory = TestDirectory::new();
    let base = project(
        &directory.0.join("base"),
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    let selected = project(
        &directory.0.join("selected"),
        true,
        "70000000-0000-4000-8000-000000000002",
        "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>70000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>",
    );
    let hidden = project(
        &directory.0.join("hidden"),
        true,
        "70000000-0000-4000-8000-000000000003",
        "",
    );
    let source = |root: &Path| root.join("src-with-common-commands");
    let settings_file = selected.join("bsl-analyzer.toml");
    fs::write(&settings_file,"[source]\nroot='../../base/project/src-with-common-commands'\nextensions=[{name='Selected',path='src-with-common-commands'},{name='Hidden',path='../../hidden/project/src-with-common-commands'}]\n").unwrap();
    let settings = project_model::ProjectConfig::load_from_file(&settings_file).unwrap();
    let readers = [&base, &selected, &hidden]
        .map(|root| SourceReader::open(&source(root)).unwrap())
        .into_iter()
        .collect();
    let topology = admit(&selected, &settings, readers, &source(&selected)).unwrap();
    let session =
        AnalysisSession::from_composition(topology, crate::SessionOptions::default()).unwrap();
    (directory, base, selected, session)
}

/// A configured CFE uses current base parameters; hidden namesakes and failed adoption cannot rescue them.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
#[test]
fn current_cfe_command_parameter_inherits_base_type_and_rejects_invalid_or_wrong_uuid_contributions()
 {
    use crate::composition::SourceKey;
    let (_directory, base, selected, mut session) = composition();
    let source = |root: &Path| root.join("src-with-common-commands");
    let request = InputRequest::default();
    session
        .load_module_saved(&owner(MODULE), session.revision(), &request)
        .unwrap();
    assert_eq!(
        parameter(&mut session, Locale::En, false).as_deref(),
        Some("String")
    );
    let base_id = session.root_ids().find(|id| id.index() == 0).unwrap();
    let hidden_id = session.root_ids().find(|id| id.index() == 2).unwrap();
    assert!(matches!(
        session.root_inputs(hidden_id).unwrap().owners[&owner(COMMAND)]
            .model
            .state,
        ModelState::Pending
    ));
    let key = SourceKey {
        root: base_id,
        source: owner(COMMAND),
    };
    let saved = fs::read_to_string(source(&base).join(COMMAND)).unwrap();
    session
        .apply_overlay_at(
            &key,
            session.revision(),
            1,
            &saved.replace("x:string", "x:boolean"),
            &request,
        )
        .unwrap();
    assert_eq!(
        parameter(&mut session, Locale::Ru, false).as_deref(),
        Some("Булево")
    );
    session
        .apply_overlay_at(&key, session.revision(), 2, "broken: [", &request)
        .unwrap();
    let offset = u32::try_from(BODY.find("Result = Chosen").unwrap() + "Result = ".len()).unwrap();
    assert!(
        matches!(session.prepare_symbol_hover_metadata(&owner(MODULE),session.revision(),offset,&request,&QueryCancellation::default()),Err(QueryError::Request(SessionRequestError::CompositionInputIncomplete(failed))) if failed==key)
    );
    session
        .apply_overlay_at(&key, session.revision(), 3, &saved, &request)
        .unwrap();
    assert_eq!(
        parameter(&mut session, Locale::En, false).as_deref(),
        Some("String")
    );
    let selected_text = fs::read_to_string(source(&selected).join(COMMAND)).unwrap();
    let wrong = selected_text.replace(
        "70000000-0000-4000-8000-000000000001",
        "70000000-0000-4000-8000-000000000999",
    );
    assert_ne!(wrong, selected_text);
    session
        .apply_overlay(&owner(COMMAND), session.revision(), 1, &wrong, &request)
        .unwrap();
    assert!(
        matches!(session.prepare_symbol_hover_metadata(&owner(MODULE),session.revision(),offset,&request,&QueryCancellation::default()),Err(QueryError::Request(SessionRequestError::CompositionInputIncomplete(failed))) if failed==SourceKey{root:session.selected_root_id(),source:owner(COMMAND)})
    );
    assert!(
        matches!(session.module_diagnostics(&owner(MODULE),session.revision(),&DiagnosticsConfig::default(),&QueryCancellation::default()),Err(QueryError::Request(SessionRequestError::CompositionInputIncomplete(failed))) if failed==SourceKey{root:session.selected_root_id(),source:owner(COMMAND)})
    );
    session
        .module_syntax_diagnostics(
            &owner(MODULE),
            session.revision(),
            &DiagnosticsConfig::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    session
        .close_overlay(&owner(COMMAND), session.revision(), &request)
        .unwrap();
    assert_eq!(
        parameter(&mut session, Locale::En, false).as_deref(),
        Some("String")
    );
    assert_eq!(
        fs::read_to_string(source(&base).join(COMMAND)).unwrap(),
        saved
    );
}
