//! Current parameter Use drives the native open structure rather than evaluating interface state.

#[path = "../../tests/support/functional_option_parameter_project.rs"]
mod fixture;
use super::*;
use crate::{
    Locale, QueryCancellation,
    session::tests::{owner, support::TestDirectory},
};
use std::{fs, path::PathBuf};

const MODULE: &str = "CommonModules/Tools/Module.bsl";
const PARAMETER: &str = "FunctionalOptionsParameters/SettingЯ/object.yaml";
const REGISTER: &str = "InformationRegisters/Settings/object.yaml";
const BODY: &str = "\u{feff}// Я😀\r\n&AtClient\r\nProcedure Run()\r\n Params=GetInterfaceFunctionalOptionParameters();\r\n Value=Params.SettingЯ;\r\n Result=Value;\r\nEndProcedure\r\n";

/// A cold ordinary importer/session owns every source and preserves source bytes.
fn project(body: &str) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    crate::test_functional_option_project::add_options(&root, &directory.0.join("native"));
    fixture::add_parameters(&root, &directory.0.join("native"));
    let source = root.join("src-with-option-parameters");
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_module_saved(&owner(MODULE), session.revision(), &InputRequest::default())
        .unwrap();
    (directory, source, session)
}

/// The native structure policy exposes leaf types as completion hints without hardening mutable values.
fn field_detail(session: &mut AnalysisSession, body: &str, locale: Locale) -> Option<String> {
    let at = u32::try_from(body.find("SettingЯ").unwrap()).unwrap();
    session
        .prepare_completion_metadata(
            &owner(MODULE),
            session.revision(),
            at,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    session
        .module_completions(
            &owner(MODULE),
            session.revision(),
            at,
            locale,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .into_iter()
        .find(|item| item.label == "SettingЯ")
        .and_then(|item| item.detail)
}

/// The real zero-argument getter prepares only parameter declarations and their Use owners.
#[test]
fn functional_option_parameter_cold_native_fields_and_storage_types_are_current() {
    let (_directory, source, mut session) = project(BODY);
    assert!(
        field_detail(&mut session, BODY, Locale::En)
            .unwrap()
            .contains("String")
    );
    assert!(matches!(
        session
            .owner(&owner("FunctionalOptions/FeatureЯ/object.yaml"))
            .unwrap()
            .state,
        ModelState::Pending
    ));
    assert!(matches!(
        session
            .owner(&owner("Constants/Storage/object.yaml"))
            .unwrap()
            .state,
        ModelState::Pending
    ));
    let at = u32::try_from(BODY.find("SettingЯ").unwrap()).unwrap();
    session
        .prepare_completion_metadata(
            &owner(MODULE),
            session.revision(),
            at,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    for locale in [Locale::Ru, Locale::En] {
        let items = session
            .module_completions(
                &owner(MODULE),
                session.revision(),
                at,
                locale,
                &QueryCancellation::default(),
            )
            .unwrap()
            .value;
        for name in ["SettingЯ", "OrganizationЯ", "UnusedЯ"] {
            assert!(
                items.iter().any(|item| item.label == name),
                "{name}: {items:?}"
            );
        }
    }
    let saved = fs::read_to_string(source.join(REGISTER)).unwrap();
    let changed = saved.replace("x:string", "x:boolean");
    assert_ne!(changed, saved);
    let revision = session.module(&owner(MODULE)).unwrap().revision;
    session
        .apply_overlay(
            &owner(REGISTER),
            session.revision(),
            1,
            &changed,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        field_detail(&mut session, BODY, Locale::Ru)
            .unwrap()
            .contains("Булево")
    );
    session
        .close_overlay(
            &owner(REGISTER),
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        field_detail(&mut session, BODY, Locale::En)
            .unwrap()
            .contains("String")
    );
    assert_eq!(session.module(&owner(MODULE)).unwrap().revision, revision);
    assert_eq!(fs::read_to_string(source.join(REGISTER)).unwrap(), saved);
}

/// Current invalid targets revoke their former type, and malformed parameters refuse semantic preparation.
#[test]
fn functional_option_parameter_invalid_watch_repair_and_close_do_not_use_saved_models() {
    let (_directory, source, mut session) = project(BODY);
    assert!(
        field_detail(&mut session, BODY, Locale::En)
            .unwrap()
            .contains("String")
    );
    let saved = fs::read_to_string(source.join(REGISTER)).unwrap();
    session
        .apply_overlay(
            &owner(REGISTER),
            session.revision(),
            1,
            "broken: [",
            &InputRequest::default(),
        )
        .unwrap();
    let unknown = field_detail(&mut session, BODY, Locale::En).unwrap_or_default();
    assert!(!unknown.contains("String") && !unknown.contains("Boolean"));
    session
        .close_overlay(
            &owner(REGISTER),
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        field_detail(&mut session, BODY, Locale::En)
            .unwrap()
            .contains("String")
    );
    let saved_parameter = fs::read_to_string(source.join(PARAMETER)).unwrap();
    session
        .apply_overlay(
            &owner(PARAMETER),
            session.revision(),
            1,
            "broken: [",
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        session
            .invalidate_saved_inputs(&[owner(PARAMETER)], session.revision())
            .unwrap()
            .is_empty()
    );
    let at = u32::try_from(BODY.rfind("Value").unwrap()).unwrap();
    assert!(
        session
            .prepare_symbol_hover_metadata(
                &owner(MODULE),
                session.revision(),
                at,
                &InputRequest::default(),
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
    session
        .apply_overlay(
            &owner(PARAMETER),
            session.revision(),
            2,
            &saved_parameter,
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        field_detail(&mut session, BODY, Locale::En)
            .unwrap()
            .contains("String")
    );
    session
        .close_overlay(
            &owner(PARAMETER),
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    assert_eq!(fs::read_to_string(source.join(REGISTER)).unwrap(), saved);
    assert_eq!(
        fs::read_to_string(source.join(PARAMETER)).unwrap(),
        saved_parameter
    );
}

/// Native platform IDs, environments and the zero-argument signature survive the configured result shape.
#[test]
fn functional_option_parameter_getter_has_native_hover_and_empty_arity() {
    let (_directory, _source, mut session) = project(BODY);
    let at = u32::try_from(BODY.find("GetInterfaceFunctionalOptionParameters").unwrap()).unwrap();
    session
        .prepare_symbol_hover_metadata(
            &owner(MODULE),
            session.revision(),
            at,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    for (locale, expected) in [(Locale::En, "Structure"), (Locale::Ru, "Структура")] {
        let card = session
            .module_symbol_hover(
                &owner(MODULE),
                session.revision(),
                at,
                locale,
                &QueryCancellation::default(),
            )
            .unwrap()
            .value
            .expect("native global");
        assert_eq!(card.kind, crate::HoverKind::PlatformFunction);
        assert!(card.return_type.unwrap().contains(expected));
    }
    let at = u32::try_from(BODY.find("();").unwrap() + 1).unwrap();
    session
        .prepare_signature_help_metadata(
            &owner(MODULE),
            session.revision(),
            at,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    let signature = session
        .module_signature_help(
            &owner(MODULE),
            session.revision(),
            at,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .expect("native zero-argument signature");
    assert!(signature.signatures[0].parameters.is_empty());
    assert!(signature.signatures[0].label.contains("Структура"));
}

/// Another body and a same-spelled user function cannot authorize parameter-owner reads.
#[test]
fn functional_option_parameter_discovery_keeps_native_call_ownership_and_body_scope() {
    for body in [
        "Function GetInterfaceFunctionalOptionParameters()\n Return New Structure();\nEndFunction\nProcedure Run()\n Params=GetInterfaceFunctionalOptionParameters();\n Value=Params.SettingЯ;\nEndProcedure\n",
        "Procedure Other()\n Params=GetInterfaceFunctionalOptionParameters();\nEndProcedure\nProcedure Run()\n Params=New Structure();\n Value=Params.SettingЯ;\nEndProcedure\n",
    ] {
        let (_directory, _source, mut session) = project(body);
        let _ = field_detail(&mut session, body, Locale::En);
        for path in [PARAMETER, REGISTER] {
            assert!(
                matches!(
                    session.owner(&owner(path)).unwrap().state,
                    ModelState::Pending
                ),
                "{path}: {body}"
            );
        }
    }
}

/// A selected extension owns only its caller and the borrowed parameter declaration, never a copy of base Use.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
fn extension(directory: &std::path::Path, suffix: &str, original: &str) -> PathBuf {
    let root = crate::test_project::project(directory, BODY, false);
    let native = directory.join("native");
    let config = native.join("Configuration.xml");
    let text = fs::read_to_string(&config).unwrap().replace("<Name>SyntaxFixture</Name>", "<Name>ParameterExtension</Name><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>").replace("10000000-0000-4000-8000-000000000001", &format!("10000000-0000-4000-8000-{suffix}")).replace("</ChildObjects>", "<FunctionalOptionsParameter>SettingЯ</FunctionalOptionsParameter></ChildObjects>");
    fs::write(config, text).unwrap();
    let module = native.join("CommonModules/Tools.xml");
    let text = fs::read_to_string(&module).unwrap().replace(
        "<Server>true</Server>",
        "<Server>false</Server><ClientManagedApplication>true</ClientManagedApplication>",
    );
    fs::write(module, text).unwrap();
    fs::create_dir_all(native.join("FunctionalOptionsParameters")).unwrap();
    fs::write(native.join("FunctionalOptionsParameters/SettingЯ.xml"), format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'><FunctionalOptionsParameter uuid='c0000000-0000-4000-8000-{suffix}'><Properties><Name>SettingЯ</Name><ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>{original}</ExtendedConfigurationObject></Properties></FunctionalOptionsParameter></MetaDataObject>")).unwrap();
    eska::project_core::source_format::import_designer_queries(
        &native,
        &root.join("src-with-option-parameters"),
    )
    .unwrap();
    let config = fs::read_to_string(root.join("eska.toml"))
        .unwrap()
        .replace("type='configuration'", "type='extension'")
        .replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-option-parameters'",
        );
    fs::write(root.join("eska.toml"), config).unwrap();
    root
}

/// Explicit settings admit only the named selected/dependency chain, preserving a hidden bad sibling.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
fn composed() -> (TestDirectory, PathBuf, PathBuf, AnalysisSession) {
    use eska::project_core::source_format::SourceReader;
    let directory = TestDirectory::new();
    let base_directory = directory.0.join("base");
    let base = crate::test_project::project(&base_directory, BODY, false);
    crate::test_functional_option_project::add_options(&base, &base_directory.join("native"));
    fixture::add_parameters(&base, &base_directory.join("native"));
    let selected = extension(
        &directory.0.join("selected"),
        "000000000002",
        "a0000000-0000-4000-8000-000000000002",
    );
    let hidden = extension(
        &directory.0.join("hidden"),
        "000000000003",
        "a0000000-0000-4000-8000-000000000999",
    );
    let settings_file = selected.join("bsl-analyzer.toml");
    fs::write(&settings_file, "[source]\nroot='../../base/project/src-with-option-parameters'\nextensions=[{name='Selected',path='src-with-option-parameters'},{name='Hidden',path='../../hidden/project/src-with-option-parameters'}]\n").unwrap();
    let settings = project_model::ProjectConfig::load_from_file(&settings_file).unwrap();
    let readers = [&base, &selected, &hidden]
        .map(|root| SourceReader::open(&root.join("src-with-option-parameters")).unwrap())
        .into_iter()
        .collect();
    let topology = crate::composition::admit(
        &selected,
        &settings,
        readers,
        &selected.join("src-with-option-parameters"),
    )
    .unwrap();
    let session =
        AnalysisSession::from_composition(topology, crate::SessionOptions::default()).unwrap();
    (directory, base, selected, session)
}

/// UUID-proven inherited Use reads the foreign current dimension, and a failed selected contribution revokes the getter.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
#[test]
fn functional_option_parameter_cfe_uses_current_base_and_refuses_a_wrong_original() {
    use crate::composition::SourceKey;
    let (_directory, base, selected, mut session) = composed();
    session
        .load_module_saved(&owner(MODULE), session.revision(), &InputRequest::default())
        .unwrap();
    assert!(
        field_detail(&mut session, BODY, Locale::En)
            .unwrap()
            .contains("String")
    );
    let hidden = session.root_ids().find(|root| root.index() == 2).unwrap();
    assert!(matches!(
        session.root_inputs(hidden).unwrap().owners[&owner(PARAMETER)]
            .model
            .state,
        ModelState::Pending
    ));
    let base_id = session.root_ids().find(|root| root.index() == 0).unwrap();
    let key = SourceKey {
        root: base_id,
        source: owner(REGISTER),
    };
    let saved = fs::read_to_string(base.join("src-with-option-parameters").join(REGISTER)).unwrap();
    session
        .apply_overlay_at(
            &key,
            session.revision(),
            1,
            &saved.replace("x:string", "x:boolean"),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        field_detail(&mut session, BODY, Locale::Ru)
            .unwrap()
            .contains("Булево")
    );
    let parameter =
        fs::read_to_string(selected.join("src-with-option-parameters").join(PARAMETER)).unwrap();
    let wrong = parameter.replace(
        "a0000000-0000-4000-8000-000000000002",
        "a0000000-0000-4000-8000-000000000999",
    );
    assert_ne!(wrong, parameter);
    session
        .apply_overlay(
            &owner(PARAMETER),
            session.revision(),
            1,
            &wrong,
            &InputRequest::default(),
        )
        .unwrap();
    let at = u32::try_from(BODY.find("SettingЯ").unwrap()).unwrap();
    assert!(
        session
            .prepare_completion_metadata(
                &owner(MODULE),
                session.revision(),
                at,
                &InputRequest::default(),
                &QueryCancellation::default()
            )
            .is_err()
    );
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
    session
        .close_overlay(
            &owner(PARAMETER),
            session.revision(),
            &InputRequest::default(),
        )
        .unwrap();
    assert!(
        field_detail(&mut session, BODY, Locale::En)
            .unwrap()
            .contains("Boolean")
    );
    assert_eq!(
        fs::read_to_string(base.join("src-with-option-parameters").join(REGISTER)).unwrap(),
        saved
    );
}
