use std::{fs, path::Path};

use eska::project::{
    Project, ProjectConfiguration, ProjectType, SourceFormat,
    object_model::{ObjectModel, ObjectModelError, discover},
};

use crate::support::TestDir;

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

/// Designer fixtures for every supported project type produce stable root identities.
#[test]
fn discovers_configuration_extension_processing_and_report_fixtures() {
    for (project_type, file, tag, metadata_type, uuid, name) in [
        (
            ProjectType::Configuration,
            "Configuration.xml",
            "Configuration",
            "configuration",
            "00000000-0000-0000-0000-000000000001",
            "MainConfiguration",
        ),
        (
            ProjectType::Extension,
            "Configuration.xml",
            "Configuration",
            "configuration",
            "00000000-0000-0000-0000-000000000002",
            "MainExtension",
        ),
        (
            ProjectType::Processing,
            "ExternalDataProcessor.xml",
            "ExternalDataProcessor",
            "data-processor",
            "00000000-0000-0000-0000-000000000003",
            "ImportData",
        ),
        (
            ProjectType::Report,
            "ExternalReport.xml",
            "ExternalReport",
            "report",
            "00000000-0000-0000-0000-000000000004",
            "Sales",
        ),
    ] {
        let fixture = fixture(project_type);
        write_descriptor(&fixture.source, file, tag, uuid, name, "");

        let model = discover(&fixture.project).expect("discover Designer object model");
        let root = object(&model, uuid);

        assert_eq!(root.name(), name);
        assert_eq!(root.metadata_type(), metadata_type);
        assert_eq!(root.descriptor_path(), Path::new(file));
        assert!(root.parent().is_none());
    }
}

/// Object UUIDs survive names while paths map both directions to nearest owners.
#[test]
fn maps_descriptors_inline_children_modules_and_forms() {
    let fixture = fixture(ProjectType::Configuration);
    write_descriptor(
        &fixture.source,
        "Configuration.xml",
        "Configuration",
        "10000000-0000-0000-0000-000000000001",
        "Main",
        "",
    );
    write_descriptor(
        &fixture.source,
        "Catalogs/Partners.xml",
        "Catalog",
        "10000000-0000-0000-0000-000000000002",
        "Partners",
        r#"<ChildObjects><Attribute uuid="10000000-0000-0000-0000-000000000003"><Properties><Name>Code</Name></Properties></Attribute><Form uuid="10000000-0000-0000-0000-000000000004"><Properties><Name>Item</Name></Properties></Form></ChildObjects>"#,
    );
    write_file(
        &fixture.source,
        "Catalogs/Partners/Ext/ObjectModule.bsl",
        "// module",
    );
    write_descriptor(
        &fixture.source,
        "Catalogs/Partners/Forms/Item.xml",
        "Form",
        "10000000-0000-0000-0000-000000000004",
        "Item",
        "",
    );
    write_file(
        &fixture.source,
        "Catalogs/Partners/Forms/Item/Ext/Form/Module.bsl",
        "// form module",
    );

    let model = discover(&fixture.project).expect("discover Designer object model");
    let catalog = object(&model, "10000000-0000-0000-0000-000000000002");
    let attribute = object(&model, "10000000-0000-0000-0000-000000000003");
    let form = object(&model, "10000000-0000-0000-0000-000000000004");

    assert_eq!(model.objects().len(), 4);
    assert_eq!(catalog.id().as_str(), "catalog:Partners");
    assert_eq!(form.id().as_str(), "catalog:Partners/form:Item");
    assert_eq!(attribute.parent(), Some(catalog.id()));
    assert_eq!(form.parent(), Some(catalog.id()));
    assert_eq!(
        catalog.module_paths().collect::<Vec<_>>(),
        [Path::new("Catalogs/Partners/Ext/ObjectModule.bsl")]
    );
    assert_eq!(
        catalog.form_paths().collect::<Vec<_>>(),
        [Path::new("Catalogs/Partners/Forms/Item.xml")]
    );
    assert_eq!(
        form.module_paths().collect::<Vec<_>>(),
        [Path::new(
            "Catalogs/Partners/Forms/Item/Ext/Form/Module.bsl"
        )]
    );
    assert_eq!(
        ids(model.objects_for_changed_path(Path::new("Catalogs/Partners.xml"))),
        [
            "10000000-0000-0000-0000-000000000002",
            "10000000-0000-0000-0000-000000000003",
            "10000000-0000-0000-0000-000000000004",
        ]
    );
    assert_eq!(
        ids(model.objects_for_changed_path(Path::new("Catalogs/Partners/Ext/ManagerModule.bsl"))),
        ["10000000-0000-0000-0000-000000000002"]
    );
    assert_eq!(
        model.paths_for_object(form.id()).expect("form paths"),
        [
            Path::new("Catalogs/Partners/Forms/Item/Ext/Form/Module.bsl"),
            Path::new("Catalogs/Partners/Forms/Item.xml"),
            Path::new("Catalogs/Partners.xml"),
        ]
    );
}

/// External projects map direct `Ext` and `Forms` folders to their single root object.
#[test]
fn maps_external_project_modules_and_forms_without_a_wrapper_directory() {
    let fixture = fixture(ProjectType::Processing);
    write_descriptor(
        &fixture.source,
        "ExternalDataProcessor.xml",
        "ExternalDataProcessor",
        "20000000-0000-0000-0000-000000000001",
        "ImportData",
        "",
    );
    write_file(&fixture.source, "Ext/ObjectModule.bsl", "// module");
    write_descriptor(
        &fixture.source,
        "Forms/Main.xml",
        "Form",
        "20000000-0000-0000-0000-000000000002",
        "Main",
        "",
    );

    let model = discover(&fixture.project).expect("discover external object model");
    let root = object(&model, "20000000-0000-0000-0000-000000000001");
    let form = object(&model, "20000000-0000-0000-0000-000000000002");

    assert_eq!(form.parent(), Some(root.id()));
    assert_eq!(
        root.module_paths().collect::<Vec<_>>(),
        [Path::new("Ext/ObjectModule.bsl")]
    );
    assert_eq!(
        root.form_paths().collect::<Vec<_>>(),
        [Path::new("Forms/Main.xml")]
    );
}

/// Descriptor candidates fail explicitly while unrelated XML payload remains an owner path.
#[test]
fn rejects_malformed_descriptors_without_parsing_owned_payload_xml() {
    let fixture = fixture(ProjectType::Configuration);
    write_descriptor(
        &fixture.source,
        "Configuration.xml",
        "Configuration",
        "30000000-0000-0000-0000-000000000001",
        "Main",
        "",
    );
    write_descriptor(
        &fixture.source,
        "Catalogs/Partners.xml",
        "Catalog",
        "30000000-0000-0000-0000-000000000002",
        "Partners",
        "",
    );
    write_file(
        &fixture.source,
        "Catalogs/Partners/Ext/Help.xml",
        "<not-designer-payload>",
    );

    let model = discover(&fixture.project).expect("payload XML is not a descriptor");
    assert_eq!(
        ids(model.objects_for_changed_path(Path::new("Catalogs/Partners/Ext/Help.xml"))),
        ["30000000-0000-0000-0000-000000000002"]
    );

    write_file(&fixture.source, "Catalogs/Broken.xml", "<broken>");
    assert!(matches!(
        discover(&fixture.project),
        Err(ObjectModelError::InvalidXml { path, .. }) if path == Path::new("Catalogs/Broken.xml")
    ));
}

/// Reused Designer UUIDs do not collapse distinct logical metadata objects.
#[test]
fn keeps_object_ids_unique_when_designer_uuids_are_reused() {
    let fixture = fixture(ProjectType::Configuration);
    for (path, name) in [
        ("Catalogs/First.xml", "First"),
        ("Catalogs/Second.xml", "Second"),
    ] {
        write_descriptor(
            &fixture.source,
            path,
            "Catalog",
            "40000000-0000-0000-0000-000000000001",
            name,
            "",
        );
    }

    let model = discover(&fixture.project).expect("discover reused UUIDs");
    let ids = model
        .objects()
        .map(|object| object.id().as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, ["catalog:First", "catalog:Second"]);
}

struct Fixture {
    _directory: TestDir,
    source: std::path::PathBuf,
    project: Project,
}

/// Create one isolated project with an empty Designer source directory.
fn fixture(project_type: ProjectType) -> Fixture {
    let directory = TestDir::new();
    let source = directory.0.join("src");
    fs::create_dir(&source).expect("create source directory");
    let project = Project::new(
        directory.0.clone(),
        source.clone(),
        ProjectConfiguration::new(project_type, SourceFormat::DesignerXml),
    )
    .expect("valid project");
    Fixture {
        _directory: directory,
        source,
        project,
    }
}

/// Write a minimal namespace-aware Designer metadata descriptor.
fn write_descriptor(source: &Path, path: &str, tag: &str, uuid: &str, name: &str, children: &str) {
    write_file(
        source,
        path,
        &format!(
            r#"<MetaDataObject xmlns="{MD}"><{tag} uuid="{uuid}"><Properties><Name>{name}</Name></Properties>{children}</{tag}></MetaDataObject>"#
        ),
    );
}

/// Write one fixture file after creating its owned parent directories.
fn write_file(source: &Path, path: &str, contents: &str) {
    let path = source.join(path);
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture parent");
    fs::write(path, contents).expect("write fixture file");
}

/// Find one fixture object by its known stable UUID.
fn object<'a>(model: &'a ObjectModel, id: &str) -> &'a eska::project::object_model::LogicalObject {
    model
        .objects()
        .find(|object| object.uuid() == id)
        .expect("fixture object")
}

/// Convert mapped objects to deterministic stable identifiers for assertions.
fn ids<const N: usize>(objects: Vec<&eska::project::object_model::LogicalObject>) -> [&str; N] {
    objects
        .into_iter()
        .map(eska::project::object_model::LogicalObject::uuid)
        .collect::<Vec<_>>()
        .try_into()
        .expect("expected object count")
}
