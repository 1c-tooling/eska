//! Small imported native projects keep declaration proofs independent of semantic model construction.

use std::{fs, path::Path};

use crate::{AnalysisScope, AnalysisSession, SessionOptions};
use eska::{
    analysis_core::InputState,
    project_core::{
        Project, ProjectConfiguration, ProjectName, ProjectType, SourceFormat,
        eska_source::EskaSource,
        metadata_model::{ModuleRole, ProjectScope},
        source_format::{SourcePath, import_designer_queries},
    },
};

use super::navigation::{Navigation, module_role};
use serde_json::json;

#[path = "../../../../bsl-adapter/tests/support/mod.rs"]
mod directory;
use directory::TestDirectory;

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

/// Write only the test's native descriptor, preserving the actual class and explicit native UUID.
fn descriptor(native: &Path, path: &str, kind: &str, uuid: u32, fields: &str, children: &str) {
    let path = native.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("<MetaDataObject xmlns=\"{MD}\" version=\"2.20\"><{kind} uuid=\"10000000-0000-4000-8000-{uuid:012}\"><Properties>{fields}</Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>")).unwrap();
}

/// Real all-four imports include separately declared forms, binary BSL and extracted plain queries.
fn fixture(kind: ProjectType, name: &str, scope: ProjectScope) -> (TestDirectory, EskaSource) {
    let directory = TestDirectory::new();
    let native = directory.0.join("native");
    fs::create_dir(&native).unwrap();
    let external = matches!(kind, ProjectType::Processing | ProjectType::Report);
    let base = if external {
        "Sample"
    } else {
        "Catalogs/Catalog"
    };
    let (tag, root_path) = match kind {
        ProjectType::Configuration | ProjectType::Extension => {
            ("Configuration", "Configuration.xml")
        }
        ProjectType::Processing => ("ExternalDataProcessor", "Sample.xml"),
        ProjectType::Report => ("ExternalReport", "Sample.xml"),
    };
    let extension = if kind == ProjectType::Extension {
        "<ObjectBelonging>Adopted</ObjectBelonging><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>"
    } else {
        ""
    };
    let children = if external {
        "<Form>Main</Form><Template>Schema</Template>"
    } else {
        "<CommonModule>Tools</CommonModule><Catalog>Catalog</Catalog>"
    };
    descriptor(
        &native,
        root_path,
        tag,
        1,
        &format!("<Name>{name}</Name>{extension}"),
        children,
    );
    if !external {
        descriptor(
            &native,
            "CommonModules/Tools.xml",
            "CommonModule",
            2,
            "<Name>Tools</Name><Server>true</Server>",
            "",
        );
        descriptor(
            &native,
            "Catalogs/Catalog.xml",
            "Catalog",
            3,
            "<Name>Catalog</Name>",
            "<Form>Main</Form><Template>Schema</Template>",
        );
    }
    descriptor(
        &native,
        &format!("{base}/Forms/Main.xml"),
        "Form",
        4,
        "<Name>Main</Name><FormType>Managed</FormType>",
        "",
    );
    descriptor(
        &native,
        &format!("{base}/Templates/Schema.xml"),
        "Template",
        5,
        "<Name>Schema</Name><TemplateType>DataCompositionSchema</TemplateType>",
        "",
    );
    for (path, bytes) in [
        (format!("{base}/Forms/Main/Ext/Form.xml"), b"<Form xmlns=\"http://v8.1c.ru/8.3/xcf/logform\" version=\"2.20\"><AutoCommandBar name=\"\" id=\"-1\"/><Attributes/></Form>".as_slice()),
        (format!("{base}/Forms/Main/Ext/Form/Module.bsl"), "\u{feff}// Я😀\r\nПроцедура Проба()\r\nКонецПроцедуры\r\n".as_bytes()),
        (format!("{base}/Templates/Schema/Ext/Template.xml"), b"<DataCompositionSchema xmlns=\"http://v8.1c.ru/8.1/data-composition-system/schema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><dataSet xsi:type=\"DataSetQuery\"><name>Main</name><query>SELECT 1 AS Value</query></dataSet><settingsVariant/></DataCompositionSchema>".as_slice()),
        (if external { "Sample/Ext/ObjectModule.bin".into() } else { "CommonModules/Tools/Ext/Module.bin".into() }, [0, 255, 1].as_slice()),
    ] {
        let path = native.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let root = directory.0.join("project");
    fs::create_dir(&root).unwrap();
    let source = root.join("src");
    import_designer_queries(&native, &source).unwrap();
    let project = Project::new(
        root,
        source,
        ProjectConfiguration::new(
            kind,
            SourceFormat::Eska {
                accepted_base: SourcePath::parse(".eska/accepted").unwrap(),
            },
        ),
    )
    .unwrap();
    (directory, EskaSource::open(project, scope).unwrap())
}

/// Handshake facts and root classes belong to the selected artifact, including true EPF/ERF profiles.
#[test]
fn all_four_project_roots_retain_actual_type_kind_source_and_directory_uris() {
    for kind in [
        ProjectType::Configuration,
        ProjectType::Extension,
        ProjectType::Processing,
        ProjectType::Report,
    ] {
        let (_directory, source) = fixture(kind, "Sample", ProjectScope::Standalone);
        let root = source.reader().configuration();
        let navigation = Navigation::selected(&source).unwrap();
        assert_eq!(
            navigation.project(),
            json!({"project_type":kind.as_str(),
            "scope":{"kind":"standalone"},
            "root_uri":url::Url::from_directory_path(source.project().root()).unwrap().as_str(),
            "source_uri":url::Url::from_directory_path(source.reader().root()).unwrap().as_str(),
            "root_source":root.path})
        );
        let expected_kind = match kind {
            ProjectType::Configuration | ProjectType::Extension => "configuration",
            ProjectType::Processing => "data-processor",
            ProjectType::Report => "report",
        };
        let owner = navigation.owner(&root.path).unwrap();
        assert_eq!(
            owner["declaration"],
            json!({"id":format!("{expected_kind}:Sample"),"kind":expected_kind,
            "name":"Sample","uuid":"10000000-0000-4000-8000-000000000001","parent":null})
        );
        if matches!(kind, ProjectType::Processing | ProjectType::Report) {
            let expected_profile = if kind == ProjectType::Processing {
                eska::project_core::source_format::PROCESSING_PROFILE
            } else {
                eska::project_core::source_format::REPORT_PROFILE
            };
            assert_eq!(root.source.document.descriptor.profile, expected_profile);
        }
    }
}

/// Common-module declarations are navigable before the shared session loads their YAML or body.
#[test]
fn pending_owners_have_identity_without_claiming_current_readiness() {
    let (_directory, source) = fixture(
        ProjectType::Configuration,
        "Sample",
        ProjectScope::Standalone,
    );
    let owner = source
        .reader()
        .owner_paths()
        .find(|path| {
            source.reader().declared_owner(path).unwrap().kind
                == eska::project_core::MetadataKind::CommonModule
        })
        .unwrap()
        .clone();
    let navigation = Navigation::selected(&source).unwrap();
    let session = AnalysisSession::from_reader_with_scope(
        source.into_reader(),
        SessionOptions::default(),
        AnalysisScope::LocalSyntax,
    )
    .unwrap();
    assert!(matches!(
        session.metadata(&owner, session.revision()).unwrap().input,
        InputState::Pending
    ));
    let declaration = navigation.owner(&owner).unwrap();
    assert_eq!(declaration["declaration"]["id"], "common-module:Tools");
    assert_eq!(declaration["declaration"]["name"], "Tools");
    assert!(declaration.get("state").is_none());
    assert!(declaration.get("permissions").is_none());
    assert!(declaration.get("model").is_none());
}

/// Logical names use the same trim and escaping rules as parsed metadata; member scope stays separate.
#[test]
fn escaped_root_names_and_member_scope_do_not_change_native_or_physical_identity() {
    let scope = ProjectScope::Member(ProjectName::parse("navigation-member".into()).unwrap());
    let (_directory, source) = fixture(ProjectType::Configuration, "  Root%/:Я😀  ", scope);
    let navigation = Navigation::selected(&source).unwrap();
    let owner = navigation
        .owner(&source.reader().configuration().path)
        .unwrap();
    assert_eq!(owner["declaration"]["id"], "configuration:Root%25%2F%3AЯ😀");
    assert_eq!(owner["declaration"]["name"], "Root%/:Я😀");
    assert_eq!(
        navigation.project()["scope"],
        json!({"kind":"member","name":"navigation-member"})
    );
    assert_eq!(navigation.project()["root_source"], "configuration.yaml");
    assert_eq!(
        source
            .reader()
            .configuration()
            .source
            .document
            .path
            .as_str(),
        "Configuration.xml"
    );
}

/// Forms retain their real parent, while mapped source bodies and queries retain native descriptor bindings.
#[test]
fn nested_owner_ids_and_module_query_bindings_follow_actual_declarations() {
    for kind in [
        ProjectType::Configuration,
        ProjectType::Processing,
        ProjectType::Report,
    ] {
        let (_directory, source) = fixture(kind, "Sample", ProjectScope::Standalone);
        let reader = source.reader();
        let navigation = Navigation::selected(&source).unwrap();
        let external = kind != ProjectType::Configuration;
        let parent = match kind {
            ProjectType::Configuration => "catalog:Catalog",
            ProjectType::Processing => "data-processor:Sample",
            ProjectType::Report => "report:Sample",
            ProjectType::Extension => unreachable!(),
        };
        let base = if external {
            "Sample"
        } else {
            "Catalogs/Catalog"
        };
        let form_native = SourcePath::parse(&format!("{base}/Forms/Main.xml")).unwrap();
        let form = reader.owner_for_native_main(&form_native).unwrap();
        let value = navigation.owner(form).unwrap();
        assert_eq!(value["declaration"]["id"], format!("{parent}/form:Main"));
        assert_eq!(value["declaration"]["parent"], parent);
        let module_source = reader
            .module_paths()
            .find(|path| reader.declared_module(path).unwrap().owner == form)
            .unwrap();
        assert_eq!(
            value["modules"],
            json!([{"source":module_source,
            "native_path":format!("{base}/Forms/Main/Ext/Form/Module.bsl"),
            "native_descriptor":form_native,"role":"module","binary":false}])
        );
        let query_source = reader.query_paths().next().unwrap();
        let query = reader.declared_query(query_source).unwrap();
        let value = navigation.owner(query.owner).unwrap();
        assert_eq!(
            value["declaration"]["id"],
            format!("{parent}/template:Schema")
        );
        assert_eq!(
            value["queries"],
            json!([{"source":query_source,
            "native_resource":format!("{base}/Templates/Schema/Ext/Template.xml")}])
        );
        assert_eq!(
            query_source.as_path().extension(),
            Some(std::ffi::OsStr::new("sdbl"))
        );
        let binary_source = reader
            .module_paths()
            .find(|path| reader.declared_module(path).unwrap().binary)
            .unwrap();
        let binary = reader.declared_module(binary_source).unwrap();
        assert_eq!(
            navigation.owner(binary.owner).unwrap()["modules"],
            json!([{"source":binary_source,
            "native_path":binary.native_path,"native_descriptor":binary.native_descriptor,
            "role":if external { "object" } else { "module" },"binary":true}])
        );
    }
}

/// A deleted own project proves capture and subsequent navigation never reopen files or scan directories.
#[test]
fn navigation_remains_declaration_only_after_all_selected_files_are_removed() {
    let (_directory, source) = fixture(
        ProjectType::Configuration,
        "Sample",
        ProjectScope::Standalone,
    );
    let paths = source.reader().owner_paths().cloned().collect::<Vec<_>>();
    fs::remove_dir_all(source.project().root()).unwrap();
    let navigation = Navigation::selected(&source).unwrap();
    assert_eq!(navigation.project()["project_type"], "configuration");
    for owner in paths {
        assert!(navigation.owner(&owner).is_some());
    }
    assert!(
        navigation
            .owner(&SourcePath::parse("private/object.yaml").unwrap())
            .is_none()
    );
    assert!(!source.project().root().exists());
}

/// All existing role keys recognize both binary and text basenames without inferring new metadata classes.
#[test]
fn native_module_roles_cover_existing_names_and_keep_unknown_names_generic() {
    for (stem, role) in [
        ("Module", ModuleRole::Module),
        ("ObjectModule", ModuleRole::Object),
        ("ManagerModule", ModuleRole::Manager),
        ("RecordSetModule", ModuleRole::RecordSet),
        ("ValueManagerModule", ModuleRole::ValueManager),
        ("ManagedApplicationModule", ModuleRole::ManagedApplication),
        ("OrdinaryApplicationModule", ModuleRole::OrdinaryApplication),
        ("SessionModule", ModuleRole::Session),
        ("ExternalConnectionModule", ModuleRole::ExternalConnection),
        ("CommandModule", ModuleRole::Command),
    ] {
        for extension in ["bsl", "bin"] {
            for spelling in [
                stem.to_owned(),
                stem.to_ascii_lowercase(),
                stem.to_ascii_uppercase(),
            ] {
                assert_eq!(
                    module_role(
                        &SourcePath::parse(&format!("Ext/{spelling}.{extension}")).unwrap()
                    ),
                    role
                );
            }
        }
    }
    assert_eq!(
        module_role(&SourcePath::parse("Ext/FutureModule.bsl").unwrap()),
        ModuleRole::Module
    );
}

/// Native role classification tolerates mixed case without changing the supplied declaration path.
#[test]
fn mixed_case_native_path_keeps_its_role_and_original_spelling() {
    let filename = "Sample/Ext/oBjEcTmOdUlE.bin";
    let path = SourcePath::parse(filename).unwrap();
    assert_eq!(module_role(&path), ModuleRole::Object);
    assert_eq!(path.as_str(), filename);
}
