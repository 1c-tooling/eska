//! Extension roots retain native identities, borrowed metadata and independently accepted rights.

use super::{MetadataKind, ObjectSource, TestDir, export_designer, fixture, fs};
use eska::project_core::{
    ProjectType,
    config::ProjectConfig,
    eska_source::EskaSource,
    metadata_model::ProjectScope,
    source_format::{
        AcceptedBase, SourceReader, accept_source, export_protected, import_designer_queries,
    },
};

/// Mark only the owned fixture as an extension; borrowed evidence remains native data.
fn extension(root: &std::path::Path) -> std::path::PathBuf {
    let native = supported_native(root);
    let path = native.join("Configuration.xml");
    let text = fs::read_to_string(&path).unwrap().replace(
        "<Properties>",
        "<Properties><ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>",
    );
    fs::write(path, text).unwrap();
    let path = native.join("Catalogs/Sample.xml");
    let text = fs::read_to_string(&path).unwrap().replace(
        "<Properties>",
        "<Properties><ObjectBelonging>Adopted</ObjectBelonging>",
    );
    fs::write(path, text).unwrap();
    native
}

/// The family fixture's opaque placeholder is not a managed dialog or a supplier configuration.
fn supported_native(root: &std::path::Path) -> std::path::PathBuf {
    let native = fixture(root);
    fs::write(native.join("Catalogs/Sample/Forms/Main/Ext/Form.xml"),
        "<Form xmlns=\"http://v8.1c.ru/8.3/xcf/logform\" version=\"2.20\"><AutoCommandBar name=\"\" id=\"-1\"/><ChildItems/></Form>"
    ).unwrap();
    fs::remove_file(native.join("Ext/ParentConfigurations.bin")).unwrap();
    native
}

#[test]
/// Reading and complete export share the native CFE profile without reinterpreting borrowed bytes.
fn extension_import_preserves_composition_modules_and_native_profile() {
    let test = TestDir::new();
    let native = extension(&test.0);
    let source = test.0.join("source");
    let manifest = import_designer_queries(&native, &source).unwrap();
    assert_eq!(manifest.schema_version, 5);
    let reader = SourceReader::open(&source).unwrap();
    assert_eq!(reader.project_type().unwrap(), ProjectType::Extension);
    assert_eq!(
        reader.configuration().source.document.path.as_str(),
        "Configuration.xml"
    );
    assert_eq!(
        reader.configuration().declarations[0].kind,
        MetadataKind::Configuration
    );
    let borrowed = reader
        .read_owner(
            &eska::project::source_format::SourcePath::parse("Catalogs/Sample/object.yaml")
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        borrowed.source.document.descriptor.object.uuid.as_str(),
        "20000000-0000-4000-8000-000000000001"
    );
    let output = test.0.join("exported");
    export_designer(&source, &output).unwrap();
    assert_eq!(
        eska::project_core::native::project_type(
            &fs::read_to_string(output.join("Configuration.xml")).unwrap()
        )
        .unwrap(),
        Some(ProjectType::Extension)
    );
    assert!(
        fs::read_to_string(output.join("Catalogs/Sample.xml"))
            .unwrap()
            .contains("<ObjectBelonging>Adopted</ObjectBelonging>")
    );
    assert_eq!(
        fs::read(native.join("CommonModules/Tools/Ext/Module.bsl")).unwrap(),
        fs::read(output.join("CommonModules/Tools/Ext/Module.bsl")).unwrap()
    );
    assert_eq!(
        fs::read(native.join("CommonModules/Tools/Ext/Module.bsl")).unwrap(),
        fs::read(source.join("CommonModules/Tools/Module.bsl")).unwrap()
    );
}

#[test]
/// A CFE cannot escape an accepted module freeze through its own editable YAML.
fn extension_protected_export_keeps_the_accepted_module() {
    let test = TestDir::new();
    let native = extension(&test.0);
    let source = test.0.join("source");
    import_designer_queries(&native, &source).unwrap();
    let owner = ObjectSource::from_yaml(
        &fs::read_to_string(source.join("CommonModules/Tools/object.yaml")).unwrap(),
    )
    .unwrap();
    let reference = accept_source(
        &source,
        &test.0.join("base"),
        &[owner.document.descriptor.object.uuid],
    )
    .unwrap();
    let base = AcceptedBase::open(&test.0.join("base"), &reference).unwrap();
    let path = source.join("CommonModules/Tools/Module.bsl");
    let mut edited = fs::read(&path).unwrap();
    edited.extend_from_slice(b"\r\n// changed owned module\r\n");
    fs::write(&path, &edited).unwrap();
    assert_eq!(
        export_protected(&source, &base, &test.0.join("strict"), true)
            .unwrap_err()
            .error
            .code,
        "source_changes_excluded"
    );
    let output = export_protected(&source, &base, &test.0.join("generations"), false).unwrap();
    assert_eq!(output.diagnostics.len(), 1);
    assert_eq!(
        fs::read(output.xml.join("CommonModules/Tools/Ext/Module.bsl")).unwrap(),
        fs::read(native.join("CommonModules/Tools/Ext/Module.bsl")).unwrap()
    );
    assert_eq!(fs::read(path).unwrap(), edited);
}

#[test]
/// The manifest's type must agree with the native profile; no baseline read or XML fallback fixes it.
fn selected_project_refuses_a_configuration_extension_mismatch() {
    for declared in ["extension", "configuration"] {
        let test = TestDir::new();
        let native = extension(&test.0);
        import_designer_queries(&native, &test.0.join("src")).unwrap();
        let config = ProjectConfig::from_toml(&format!(
            "[project]\ntype='{declared}'\nsource_format='eska'\n[eska]\naccepted_base='missing-base'\n"
        )).unwrap();
        let project = config.into_project(test.0.clone()).unwrap();
        let opened = EskaSource::open(project, ProjectScope::Standalone);
        if declared == "extension" {
            assert_eq!(
                opened.unwrap().reader().project_type().unwrap(),
                ProjectType::Extension
            );
        } else {
            assert_eq!(opened.err().unwrap().code, "source_profile_unsupported");
        }
    }
}

#[test]
/// A foreign or nested marker cannot turn an ordinary configuration into an extension.
fn root_profile_checks_the_native_property_namespace_and_slot() {
    let test = TestDir::new();
    let native = supported_native(&test.0);
    let path = native.join("Configuration.xml");
    let contents = fs::read_to_string(&path).unwrap()
        .replace("<MetaDataObject ", "<MetaDataObject xmlns:f=\"urn:foreign\" ")
        .replace("<Properties>", "<Properties><f:ConfigurationExtensionPurpose>Patch</f:ConfigurationExtensionPurpose>")
        .replace("<ChildObjects>", "<InternalInfo><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose></InternalInfo><ChildObjects>");
    fs::write(path, contents).unwrap();
    import_designer_queries(&native, &test.0.join("source")).unwrap();
    assert_eq!(
        SourceReader::open(&test.0.join("source"))
            .unwrap()
            .project_type()
            .unwrap(),
        ProjectType::Configuration
    );
}
