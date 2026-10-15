//! Configured functional-option results are current storage types rather than universal Boolean values.

use super::*;
use crate::{
    Locale, QueryCancellation,
    session::tests::{owner, support::TestDirectory},
};
use std::{fs, path::PathBuf};

const MODULE: &str = "CommonModules/Tools/Module.bsl";
const OPTION: &str = "FunctionalOptions/FeatureЯ/object.yaml";
const CONSTANT: &str = "Constants/Storage/object.yaml";
const BODY: &str = "\u{feff}// Я😀\r\nProcedure Run()\r\n Var Value;\r\n Value=GetFunctionalOption(\"FeatureЯ\");\r\n Result=Value;\r\nEndProcedure\r\n";

/// The normal importer and current source reader keep every effect inside the owning test directory.
fn project(body: &str) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, body, false);
    crate::test_functional_option_project::add_options(&root, &directory.0.join("native"));
    let source = root.join("src-with-options");
    let mut session = AnalysisSession::open(&source).unwrap();
    session
        .load_module_saved(&owner(MODULE), session.revision(), &InputRequest::default())
        .unwrap();
    (directory, source, session)
}

/// Hover a local value after the public cold preparation path, retaining native UTF-8 coordinates.
fn value(session: &mut AnalysisSession, body: &str, locale: Locale) -> Option<String> {
    let offset = u32::try_from(body.rfind("Value").unwrap()).unwrap();
    session
        .prepare_symbol_hover_metadata(
            &owner(MODULE),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
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
        .expect("declared native local")
        .return_type
}

/// A type edit in the current storage changes the unchanged call and its native local value.
#[test]
fn functional_option_current_constant_type_reaches_native_hover() {
    let (_directory, source, mut session) = project(BODY);
    session
        .load_semantic_inputs(session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(
        value(&mut session, BODY, Locale::En).as_deref(),
        Some("String")
    );
    let path = owner(CONSTANT);
    let saved = fs::read_to_string(source.join(CONSTANT)).unwrap();
    session
        .apply_overlay(
            &path,
            session.revision(),
            1,
            &saved.replace("x:string", "x:boolean"),
            &InputRequest::default(),
        )
        .unwrap();
    assert_eq!(
        value(&mut session, BODY, Locale::Ru).as_deref(),
        Some("Булево")
    );
    session
        .close_overlay(&path, session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(
        value(&mut session, BODY, Locale::En).as_deref(),
        Some("String")
    );
    assert_eq!(fs::read_to_string(source.join(CONSTANT)).unwrap(), saved);
    assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), BODY);
}

/// Dirty failures revoke the previous type, ignore saved watch noise and recover only current bytes.
#[test]
fn functional_option_invalid_watch_repair_and_close_preserve_source_authority() {
    let (_directory, source, mut session) = project(BODY);
    assert_eq!(
        value(&mut session, BODY, Locale::En).as_deref(),
        Some("String")
    );
    let module_revision = session.module(&owner(MODULE)).unwrap().revision;
    let option = owner(OPTION);
    let saved = fs::read_to_string(source.join(OPTION)).unwrap();
    let changed = saved.replace("Constant.Storage", "Catalog.Items.Attribute.Stored");
    session
        .apply_overlay(
            &option,
            session.revision(),
            1,
            &changed,
            &InputRequest::default(),
        )
        .unwrap();
    assert_eq!(
        value(&mut session, BODY, Locale::En).as_deref(),
        Some("Boolean | NULL")
    );
    assert!(
        session
            .invalidate_saved_inputs(std::slice::from_ref(&option), session.revision())
            .unwrap()
            .is_empty()
    );
    session
        .apply_overlay(
            &option,
            session.revision(),
            2,
            "broken: [",
            &InputRequest::default(),
        )
        .unwrap();
    let offset = u32::try_from(BODY.rfind("Value").unwrap()).unwrap();
    assert!(
        session
            .prepare_symbol_hover_metadata(
                &owner(MODULE),
                session.revision(),
                offset,
                &InputRequest::default(),
                &QueryCancellation::default()
            )
            .is_err()
    );
    session
        .apply_overlay(
            &option,
            session.revision(),
            3,
            &changed,
            &InputRequest::default(),
        )
        .unwrap();
    assert_eq!(
        value(&mut session, BODY, Locale::En).as_deref(),
        Some("Boolean | NULL")
    );
    session
        .close_overlay(&option, session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(
        value(&mut session, BODY, Locale::En).as_deref(),
        Some("String")
    );
    let constant = owner(CONSTANT);
    session
        .apply_overlay(
            &constant,
            session.revision(),
            1,
            "broken: [",
            &InputRequest::default(),
        )
        .unwrap();
    assert_eq!(value(&mut session, BODY, Locale::En), None);
    session
        .close_overlay(&constant, session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(
        value(&mut session, BODY, Locale::En).as_deref(),
        Some("String")
    );
    assert_eq!(fs::read_to_string(source.join(OPTION)).unwrap(), saved);
    assert_eq!(
        session.module(&owner(MODULE)).unwrap().revision,
        module_revision
    );
}

/// Current Location selects constant, catalog-attribute or register-resource types with the same native BSL call.
#[test]
fn functional_option_storage_kinds_and_explicit_empty_are_distinct() {
    let (_directory, source, mut session) = project(BODY);
    session
        .load_semantic_inputs(session.revision(), &InputRequest::default())
        .unwrap();
    let path = owner(OPTION);
    let saved = fs::read_to_string(source.join(OPTION)).unwrap();
    for (index, (location, expected)) in [
        ("Catalog.Items.Attribute.Stored", Some("Boolean | NULL")),
        (
            "InformationRegister.Settings.Resource.Stored",
            Some("DateTime"),
        ),
        ("", None),
    ]
    .into_iter()
    .enumerate()
    {
        let text = saved.replace("Constant.Storage", location);
        session
            .apply_overlay(
                &path,
                session.revision(),
                i64::try_from(index + 1).unwrap(),
                &text,
                &InputRequest::default(),
            )
            .unwrap();
        assert_eq!(
            value(&mut session, BODY, Locale::En).as_deref(),
            expected,
            "{location}"
        );
    }
}

/// Cold literal discovery prepares one option and its exact current owner rather than every declaration.
#[test]
fn functional_option_cold_hover_prepares_only_the_named_storage() {
    let (_directory, _source, mut session) = project(BODY);
    assert_eq!(
        value(&mut session, BODY, Locale::En).as_deref(),
        Some("String")
    );
    assert!(matches!(
        session
            .owner(&owner("FunctionalOptions/Other/object.yaml"))
            .unwrap()
            .state,
        ModelState::Pending
    ));
    assert!(matches!(
        session
            .owner(&owner("Catalogs/Items/object.yaml"))
            .unwrap()
            .state,
        ModelState::Pending
    ));
}

/// The real platform global's call binding supplies current return types to both native readers.
#[test]
fn functional_option_global_hover_and_signature_share_current_native_candidates() {
    let (_directory, _source, mut session) = project(BODY);
    let offset = u32::try_from(BODY.find("GetFunctionalOption").unwrap()).unwrap();
    session
        .prepare_symbol_hover_metadata(
            &owner(MODULE),
            session.revision(),
            offset,
            &InputRequest::default(),
            &QueryCancellation::default(),
        )
        .unwrap();
    for (locale, expected) in [(Locale::Ru, "Строка"), (Locale::En, "String")] {
        let card = session
            .module_symbol_hover(
                &owner(MODULE),
                session.revision(),
                offset,
                locale,
                &QueryCancellation::default(),
            )
            .unwrap()
            .value
            .expect("native global card");
        assert_eq!(card.kind, crate::HoverKind::PlatformFunction);
        assert_eq!(card.return_type.as_deref(), Some(expected));
    }
    let offset = u32::try_from(BODY.find("\"FeatureЯ\"").unwrap()).unwrap();
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
        .expect("native global signature");
    assert!(
        signature.signatures[0].label.contains("Строка"),
        "{signature:?}"
    );
    assert!(
        !signature.signatures[0].label.contains("Произвольный"),
        "{signature:?}"
    );
}

/// User methods, dynamic names and another procedure's platform call cannot grant option payload reads.
#[test]
fn functional_option_discovery_rejects_shadowed_and_dynamic_calls() {
    for body in [
        "Function GetFunctionalOption(Name)\n Return 42;\nEndFunction\nProcedure Run()\n Var Value;\n Value=GetFunctionalOption(\"FeatureЯ\");\n Result=Value;\nEndProcedure\n",
        "Procedure Run(Name)\n Var Value;\n Value=GetFunctionalOption(Name);\n Result=Value;\nEndProcedure\n",
        "Procedure Run()\n Var Value;\n Value=42;\n Result=Value;\nEndProcedure\nProcedure Other()\n GetFunctionalOption(\"FeatureЯ\");\nEndProcedure\n",
    ] {
        let (_directory, _source, mut session) = project(body);
        let _ = value(&mut session, body, Locale::En);
        assert!(
            matches!(
                session.owner(&owner(OPTION)).unwrap().state,
                ModelState::Pending
            ),
            "{body}"
        );
    }
}

/// A separately imported extension contains only an exact borrowed option and its own caller body.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
fn extension(directory: &std::path::Path, uuid: &str, original: &str) -> PathBuf {
    let root = crate::test_project::project(directory, BODY, false);
    let native = directory.join("native");
    let path = native.join("Configuration.xml");
    let config = fs::read_to_string(&path).unwrap().replace("<Name>SyntaxFixture</Name>", "<Name>Extension</Name><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>").replace("10000000-0000-4000-8000-000000000001", &uuid.replacen("80000000", "10000000", 1)).replace("</ChildObjects>", "<FunctionalOption>FeatureЯ</FunctionalOption></ChildObjects>");
    fs::write(path, config).unwrap();
    fs::create_dir_all(native.join("FunctionalOptions")).unwrap();
    fs::write(native.join("FunctionalOptions/FeatureЯ.xml"), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><FunctionalOption uuid="{uuid}"><Properties><Name>FeatureЯ</Name><ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>{original}</ExtendedConfigurationObject></Properties></FunctionalOption></MetaDataObject>"#)).unwrap();
    eska::project_core::source_format::import_designer_queries(
        &native,
        &root.join("src-with-options"),
    )
    .unwrap();
    let config = fs::read_to_string(root.join("eska.toml"))
        .unwrap()
        .replace("type='configuration'", "type='extension'")
        .replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-with-options'",
        );
    fs::write(root.join("eska.toml"), config).unwrap();
    root
}

/// Native file-scoped type inference retains the selected CFE's current base and excludes an incompatible sibling.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
#[test]
fn functional_option_cfe_inherits_current_storage_and_refuses_unproven_original() {
    use crate::composition::SourceKey;
    let (_directory, base, selected, mut session) = composed();
    session
        .load_module_saved(&owner(MODULE), session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(
        value(&mut session, BODY, Locale::En).as_deref(),
        Some("String")
    );
    let hidden_id = session.root_ids().find(|root| root.index() == 2).unwrap();
    assert!(matches!(
        session.root_inputs(hidden_id).unwrap().owners[&owner(OPTION)]
            .model
            .state,
        ModelState::Pending
    ));
    let base_id = session.root_ids().find(|root| root.index() == 0).unwrap();
    let key = SourceKey {
        root: base_id,
        source: owner(CONSTANT),
    };
    let saved = fs::read_to_string(base.join("src-with-options").join(CONSTANT)).unwrap();
    session
        .apply_overlay_at(
            &key,
            session.revision(),
            1,
            &saved.replace("x:string", "x:boolean"),
            &InputRequest::default(),
        )
        .unwrap();
    assert_eq!(
        value(&mut session, BODY, Locale::Ru).as_deref(),
        Some("Булево")
    );
    let text = fs::read_to_string(selected.join("src-with-options").join(OPTION)).unwrap();
    let wrong = text.replace(
        "80000000-0000-4000-8000-000000000001",
        "80000000-0000-4000-8000-000000000999",
    );
    session
        .apply_overlay(
            &owner(OPTION),
            session.revision(),
            1,
            &wrong,
            &InputRequest::default(),
        )
        .unwrap();
    let offset = u32::try_from(BODY.rfind("Value").unwrap()).unwrap();
    assert!(
        session
            .prepare_symbol_hover_metadata(
                &owner(MODULE),
                session.revision(),
                offset,
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
        .close_overlay(&owner(OPTION), session.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(
        value(&mut session, BODY, Locale::En).as_deref(),
        Some("Boolean")
    );
    assert_eq!(
        fs::read_to_string(base.join("src-with-options").join(CONSTANT)).unwrap(),
        saved
    );
}

/// Enroll only independently imported roots from the explicit analyzer configuration.
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
fn composed() -> (TestDirectory, PathBuf, PathBuf, AnalysisSession) {
    use eska::project_core::source_format::SourceReader;
    let directory = TestDirectory::new();
    let base_directory = directory.0.join("base");
    let base = crate::test_project::project(&base_directory, BODY, false);
    crate::test_functional_option_project::add_options(&base, &base_directory.join("native"));
    let selected = extension(
        &directory.0.join("selected"),
        "80000000-0000-4000-8000-000000000002",
        "80000000-0000-4000-8000-000000000001",
    );
    let hidden = extension(
        &directory.0.join("hidden"),
        "80000000-0000-4000-8000-000000000003",
        "80000000-0000-4000-8000-000000000999",
    );
    let settings_file = selected.join("bsl-analyzer.toml");
    fs::write(&settings_file, "[source]\nroot='../../base/project/src-with-options'\nextensions=[{name='Selected',path='src-with-options'},{name='Hidden',path='../../hidden/project/src-with-options'}]\n").unwrap();
    let settings = project_model::ProjectConfig::load_from_file(&settings_file).unwrap();
    let readers = [&base, &selected, &hidden]
        .map(|root| SourceReader::open(&root.join("src-with-options")).unwrap())
        .into_iter()
        .collect();
    let topology = crate::composition::admit(
        &selected,
        &settings,
        readers,
        &selected.join("src-with-options"),
    )
    .unwrap();
    let session =
        AnalysisSession::from_composition(topology, crate::SessionOptions::default()).unwrap();
    (directory, base, selected, session)
}
