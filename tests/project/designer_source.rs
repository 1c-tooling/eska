use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::support::TestDir;
use eska::project::{
    designer_source::{DesignerSource, SourceError, SourceRole, open_projects},
    metadata_model::{
        MetadataKind, MetadataObject, MetadataProjectError, ModuleRole, ObjectId, ProjectScope,
    },
};

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

/// Module locations cover every role and reverse ownership reads only affected descriptors.
#[test]
fn maps_all_module_roles_and_reuses_incremental_reverse_ownership() {
    let (_directory, source) = fixture(
        "extension",
        "Configuration.xml",
        "Configuration",
        "<ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>",
    );
    for (kind, folder, tag, role, file) in [
        (
            MetadataKind::Catalog,
            "Catalogs",
            "Catalog",
            ModuleRole::Object,
            "ObjectModule.bsl",
        ),
        (
            MetadataKind::Catalog,
            "Catalogs",
            "Catalog",
            ModuleRole::Manager,
            "ManagerModule.bsl",
        ),
        (
            MetadataKind::InformationRegister,
            "InformationRegisters",
            "InformationRegister",
            ModuleRole::RecordSet,
            "RecordSetModule.bsl",
        ),
        (
            MetadataKind::Constant,
            "Constants",
            "Constant",
            ModuleRole::ValueManager,
            "ValueManagerModule.bsl",
        ),
        (
            MetadataKind::CommonModule,
            "CommonModules",
            "CommonModule",
            ModuleRole::Module,
            "Module.bsl",
        ),
        (
            MetadataKind::CommonCommand,
            "CommonCommands",
            "CommonCommand",
            ModuleRole::Command,
            "CommandModule.bsl",
        ),
    ] {
        let object = id(kind, "Item", None);
        write(
            source.project().source(),
            &format!("{folder}/Item.xml"),
            xml(tag, "Item", ""),
        );
        let path = Path::new(folder).join("Item").join("Ext").join(file);
        write(source.project().source(), path.to_str().unwrap(), [0xff]);
        assert_eq!(source.module(&object, role).unwrap().unwrap().path, path);
        let affected = source.changed_owners(std::slice::from_ref(&path)).unwrap();
        assert!(affected.issues().is_empty());
        assert_eq!(affected.descriptors_read(), 1);
        assert_eq!(
            affected.model().objects_for_changed_path(&path)[0].id(),
            &object
        );
    }
    for (role, file) in [
        (
            ModuleRole::ManagedApplication,
            "ManagedApplicationModule.bsl",
        ),
        (
            ModuleRole::OrdinaryApplication,
            "OrdinaryApplicationModule.bsl",
        ),
        (ModuleRole::Session, "SessionModule.bsl"),
        (
            ModuleRole::ExternalConnection,
            "ExternalConnectionModule.bsl",
        ),
    ] {
        write(source.project().source(), &format!("Ext/{file}"), [0xff]);
        assert_eq!(
            source
                .module(source.root().id(), role)
                .unwrap()
                .unwrap()
                .path,
            Path::new("Ext").join(file)
        );
    }
}

/// Write one owned fixture file and its parent directories.
fn write(root: &Path, path: &str, content: impl AsRef<[u8]>) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Construct a minimal Designer descriptor without invoking any platform tool.
fn xml(tag: &str, name: &str, extra: &str) -> String {
    format!(
        "<MetaDataObject xmlns=\"{MD}\"><{tag} uuid=\"shared\"><Properties><Name>{name}</Name>{extra}</Properties></{tag}></MetaDataObject>"
    )
}

/// Open a tiny manifest-backed source tree with an intentionally nonstandard source directory.
fn fixture(
    project_type: &str,
    filename: &str,
    tag: &str,
    extra: &str,
) -> (TestDir, DesignerSource) {
    let directory = TestDir::new();
    write(
        &directory.0,
        "eska.toml",
        format!("[project]\ntype = '{project_type}'\nsource = 'design'\n"),
    );
    write(
        &directory.0,
        &format!("design/{filename}"),
        xml(tag, "Root", extra),
    );
    let mut projects = open_projects(&directory.0, &[], false).expect("open fixture");
    (directory, projects.remove(0))
}

/// Build identities through the public metadata model rather than inventing wire strings.
fn id(kind: MetadataKind, name: &str, parent: Option<&ObjectId>) -> ObjectId {
    MetadataObject::new(kind, name.into(), "shared".into(), parent.cloned())
        .unwrap()
        .id()
        .clone()
}

/// All four project types require a manifest and agree with their root descriptor.
#[test]
fn opens_manifest_backed_project_types_without_build_platform() {
    for (kind, file, tag, extra) in [
        ("configuration", "Configuration.xml", "Configuration", ""),
        (
            "extension",
            "Configuration.xml",
            "Configuration",
            "<ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>",
        ),
        ("report", "NamedExport.xml", "ExternalReport", ""),
        ("processing", "NamedExport.xml", "ExternalDataProcessor", ""),
    ] {
        let (directory, source) = fixture(kind, file, tag, extra);
        assert_eq!(
            source.project().configuration().project_type().as_str(),
            kind
        );
        assert_eq!(source.descriptor(), Path::new(file));
        assert_eq!(source.scope(), &ProjectScope::Standalone);
        let manifest = fs::read(directory.0.join("eska.toml")).unwrap();
        write(&directory.0, "design/Unrelated/Broken.xml", "<broken>");
        open_projects(&directory.0, &[], false).expect("no recursive parse");
        assert_eq!(fs::read(directory.0.join("eska.toml")).unwrap(), manifest);
        fs::remove_file(directory.0.join("eska.toml")).unwrap();
        assert!(matches!(
            open_projects(&directory.0, &[], false),
            Err(SourceError::Discovery(_))
        ));
    }
    let (directory, _) = fixture("configuration", "Configuration.xml", "Configuration", "");
    write(
        &directory.0,
        "eska.toml",
        "[project]\ntype='extension'\nsource='design'\n",
    );
    assert!(matches!(
        open_projects(&directory.0, &[], false),
        Err(SourceError::InvalidRoot {
            source: MetadataProjectError::TypeMismatch { .. },
            ..
        })
    ));
}

/// Every registered top-level folder resolves without reading its object descriptor.
#[test]
fn maps_registered_types_inline_elements_and_missing_candidates() {
    let (_directory, source) = fixture("configuration", "Configuration.xml", "Configuration", "");
    for &kind in MetadataKind::ALL {
        let Some(folder) = kind.collection_folder() else {
            continue;
        };
        let object = id(kind, "Пример", None);
        assert!(source.sources(&object).unwrap().is_empty());
        let path = format!("{folder}/Пример.xml");
        write(
            source.project().source(),
            &path,
            "descriptor content belongs to parser, not resolver",
        );
        let files = source.sources(&object).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, Path::new(&path));
    }
    let catalog = id(MetadataKind::Catalog, "Пример", None);
    let section = id(MetadataKind::TabularSection, "Контакты", Some(&catalog));
    let attribute = id(MetadataKind::Attribute, "Телефон", Some(&section));
    let files = source.sources(&attribute).unwrap();
    assert_eq!(files[0].path, Path::new("Catalogs/Пример.xml"));
    assert_eq!(
        files[0]
            .inline
            .iter()
            .map(|part| part.name.as_str())
            .collect::<Vec<_>>(),
        ["Контакты", "Телефон"]
    );
    assert!(
        source
            .sources(&id(MetadataKind::Catalog, "../outside", None))
            .is_err()
    );
    assert!(
        source
            .sources(&id(MetadataKind::Catalog, "..\\outside", None))
            .is_err()
    );
}

/// Binary-only modules disappear, while existing BSL paths are resolved without decoding BSL bytes.
#[test]
fn maps_modules_forms_commands_templates_and_subsystems() {
    let (_directory, source) = fixture("configuration", "Configuration.xml", "Configuration", "");
    let root = source.project().source();
    write(
        root,
        "Catalogs/Partners.xml",
        xml("Catalog", "Partners", ""),
    );
    let catalog = id(MetadataKind::Catalog, "Partners", None);
    write(root, "Catalogs/Partners/Ext/ObjectModule.bin", [0xff, 0xff]);
    assert_eq!(source.module(&catalog, ModuleRole::Object).unwrap(), None);
    write(root, "Catalogs/Partners/Ext/ObjectModule.bsl", [0xff, 0xff]);
    write(
        root,
        "Catalogs/Partners/Ext/ManagerModule.bsl",
        [0xff, 0xff],
    );
    let files = source.sources(&catalog).unwrap();
    assert_eq!(files.len(), 3);
    assert!(
        files
            .iter()
            .all(|file| file.path.extension().unwrap() != "bin")
    );
    assert_eq!(
        source
            .module(&catalog, ModuleRole::Object)
            .unwrap()
            .unwrap()
            .path,
        Path::new("Catalogs/Partners/Ext/ObjectModule.bsl")
    );

    for (kind, folder, payload) in [
        (MetadataKind::Form, "Forms", "Form.xml"),
        (MetadataKind::Template, "Templates", "Template.mxl"),
        (MetadataKind::Command, "Commands", "CommandModule.bsl"),
    ] {
        let path = format!("Catalogs/Partners/{folder}/Item");
        write(root, &format!("{path}.xml"), "descriptor");
        write(root, &format!("{path}/Ext/{payload}"), [0xff]);
        if kind == MetadataKind::Form {
            write(root, &format!("{path}/Ext/Form/Module.bsl"), [0xff]);
        }
        let object = id(kind, "Item", Some(&catalog));
        let files = source.sources(&object).unwrap();
        assert_eq!(files[0].path, PathBuf::from(format!("{path}.xml")));
        assert!(
            files
                .iter()
                .any(|file| file.path == Path::new(&format!("{path}/Ext/{payload}")))
        );
        if kind == MetadataKind::Form {
            assert!(
                files
                    .iter()
                    .any(|file| file.role == SourceRole::Module(ModuleRole::Module))
            );
        }
    }
    let subsystem = id(MetadataKind::Subsystem, "First", None);
    let nested = id(MetadataKind::Subsystem, "Second", Some(&subsystem));
    write(root, "Subsystems/First/Subsystems/Second.xml", "descriptor");
    assert_eq!(
        source.sources(&nested).unwrap()[0].path,
        Path::new("Subsystems/First/Subsystems/Second.xml")
    );
}

/// External exports support both real layouts and reject duplicate candidate files.
#[test]
fn maps_external_layouts_and_reports_ambiguity() {
    for prefix in ["", "NamedExport/"] {
        let (_directory, source) = fixture("report", "NamedExport.xml", "ExternalReport", "");
        let root = source.project().source();
        write(root, &format!("{prefix}Ext/ObjectModule.bsl"), "module");
        write(root, &format!("{prefix}Forms/Main.xml"), "form descriptor");
        let form = id(MetadataKind::Form, "Main", Some(source.root().id()));
        assert_eq!(
            source.sources(&form).unwrap()[0].path,
            PathBuf::from(format!("{prefix}Forms/Main.xml"))
        );
        assert_eq!(
            source
                .module(source.root().id(), ModuleRole::Object)
                .unwrap()
                .unwrap()
                .path,
            PathBuf::from(format!("{prefix}Ext/ObjectModule.bsl"))
        );
        let duplicate = if prefix.is_empty() {
            "NamedExport/Ext/ObjectModule.bsl"
        } else {
            "Ext/ObjectModule.bsl"
        };
        write(root, duplicate, "other module");
        assert!(matches!(
            source.module(source.root().id(), ModuleRole::Object),
            Err(SourceError::AmbiguousLocation { .. })
        ));
        write(root, "Second.xml", xml("ExternalReport", "Other", ""));
        assert!(matches!(
            open_projects(source.project().root(), &[], false),
            Err(SourceError::AmbiguousRoot { .. })
        ));
    }
}

/// Selecting one member does not parse an unrelated member's broken root XML.
#[test]
fn opens_only_selected_workspace_root_descriptors() {
    let directory = TestDir::new();
    write(
        &directory.0,
        "eska.toml",
        "[workspace]\nmembers=['first','second']\n",
    );
    for name in ["first", "second"] {
        write(
            &directory.0,
            &format!("{name}/eska.toml"),
            format!("[project]\nname='{name}'\ntype='configuration'\n"),
        );
        write(
            &directory.0,
            &format!("{name}/src/Configuration.xml"),
            if name == "first" {
                xml("Configuration", "Root", "")
            } else {
                "<broken>".into()
            },
        );
    }
    let source = open_projects(&directory.0, &["first".into()], false)
        .unwrap()
        .remove(0);
    assert!(matches!(source.scope(), ProjectScope::Member(name) if name.as_str() == "first"));
    assert!(open_projects(&directory.0, &[], true).is_err());
}

/// A source-relative symlink may not escape into another project.
#[cfg(unix)]
#[test]
fn rejects_symlink_escapes_for_descriptors_and_modules() {
    use std::os::unix::fs::symlink;
    let (_directory, source) = fixture("configuration", "Configuration.xml", "Configuration", "");
    let outside = TestDir::new();
    write(&outside.0, "Partners.xml", "outside descriptor");
    fs::create_dir(source.project().source().join("Catalogs")).unwrap();
    symlink(
        outside.0.join("Partners.xml"),
        source.project().source().join("Catalogs/Partners.xml"),
    )
    .unwrap();
    let object = id(MetadataKind::Catalog, "Partners", None);
    assert!(matches!(
        source.sources(&object),
        Err(SourceError::OutsideSource { .. })
    ));
    fs::remove_file(source.project().source().join("Catalogs/Partners.xml")).unwrap();
    write(
        source.project().source(),
        "Catalogs/Partners.xml",
        "descriptor",
    );
    fs::create_dir(source.project().source().join("Catalogs/Partners")).unwrap();
    write(&outside.0, "ObjectModule.bsl", "outside module");
    symlink(
        &outside.0,
        source.project().source().join("Catalogs/Partners/Ext"),
    )
    .unwrap();
    assert!(matches!(
        source.module(&object, ModuleRole::Object),
        Err(SourceError::OutsideSource { .. })
    ));
}
