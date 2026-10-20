//! Fixtures exercise manifest composition without a platform, Git or analyzer process.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::{ProjectConfig, WorkspaceConfig, WorkspaceConfigError},
    project::{
        ProjectName, ProjectType, designer_source::SourceError, discovery::ContextDiscoveryError,
    },
    test_support::TestDir,
};

use super::{ContextError, SettingsError, SourceRole, TargetPlatformVersion, resolve};

/// Create one declared project with a nonstandard source and a real Designer root descriptor.
fn project(root: &Path, kind: &str, name: Option<&str>, version: Option<&str>) -> PathBuf {
    let source = root.join("исходники XML");
    fs::create_dir_all(&source).unwrap();
    let name = name.map_or(String::new(), |name| format!("name = {name:?}\n"));
    let version = version.map_or(String::new(), |version| {
        format!("[build]\nplatform_version = {version:?}\n")
    });
    fs::write(
        root.join("eska.toml"),
        format!("[project]\n{name}type = {kind:?}\nsource = 'исходники XML'\n{version}"),
    )
    .unwrap();
    let (tag, purpose) = match kind {
        "configuration" => ("Configuration", ""),
        "extension" => (
            "Configuration",
            "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>",
        ),
        "processing" => ("ExternalDataProcessor", ""),
        "report" => ("ExternalReport", ""),
        _ => panic!("unsupported fixture type"),
    };
    let field = if kind == "extension" {
        "ConfigurationExtensionCompatibilityMode"
    } else {
        "CompatibilityMode"
    };
    let file = if matches!(kind, "configuration" | "extension") {
        "Configuration.xml"
    } else {
        "Файл.СовсемДругоеИмя.xml"
    };
    let descriptor = source.join(file);
    fs::write(&descriptor, format!("\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.17\"><{tag} uuid=\"11111111-1111-1111-1111-111111111111\"><Properties><Name>ИмяИзXML</Name>{purpose}<{field}>Version8_3_20</{field}></Properties><ChildObjects/></{tag}></MetaDataObject>\r\n")).unwrap();
    fs::write(
        source.join("NeverRead.bsl"),
        "This is intentionally not valid BSL\r\n",
    )
    .unwrap();
    descriptor
}

/// Root settings always name members explicitly; the resolver may not infer additional inputs.
fn workspace(root: &Path, members: &[&str], application: &str) {
    fs::write(root.join("eska.toml"), format!("[workspace]\nmembers = {members:?}\n[build]\nplatform_version = '8.3.27.2325'\n{application}")).unwrap();
}

/// Compare exact files before/after read-only resolution, including BOM, CRLF and opaque bytes.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.insert(path.clone(), fs::read(path).unwrap());
            }
        }
    }
    files
}

/// Expose semantic schema errors separately from TOML parse failures.
fn settings_error(extra: &str) -> SettingsError {
    let document = format!("[workspace]\nmembers = []\n{extra}");
    match WorkspaceConfig::from_toml(&document).unwrap_err() {
        WorkspaceConfigError::InvalidAnalysis(error) => *error,
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
/// All four standalone project types retain real filenames, identity and limited-context status.
fn standalone_projects_use_real_sources_and_keep_compatibility_separate_from_target() {
    for kind in ["configuration", "extension", "processing", "report"] {
        let fixture = TestDir::new();
        let path = project(&fixture.0, kind, None, Some("8.3.27.2325"));
        fs::write(path.parent().unwrap().join("opaque.bin"), [0, 255, 128]).unwrap();
        let before = snapshot(&fixture.0);
        let context = resolve(&fixture.0, None).unwrap();
        assert_eq!(context.root(), fixture.0);
        assert_eq!(context.sources().len(), 1);
        let source = &context.sources()[0];
        assert_eq!(source.project().source(), path.parent().unwrap());
        assert_eq!(source.descriptor(), path.file_name().unwrap());
        assert_eq!(source.metadata_name(), "ИмяИзXML");
        assert_eq!(source.uuid(), "11111111-1111-1111-1111-111111111111");
        assert_eq!(source.compatibility_mode(), Some("Version8_3_20"));
        assert_eq!(
            context.target_platform_version().unwrap().as_str(),
            "8.3.27.2325"
        );
        assert_eq!(context.limited_context(), kind != "configuration");
        assert_eq!(snapshot(&fixture.0), before);
    }
}

#[test]
/// Explicit three-component targets survive canonical serialization without changing build settings.
fn standalone_target_override_is_round_tripped_without_inventing_a_default() {
    let fixture = TestDir::new();
    project(&fixture.0, "processing", None, None);
    assert!(
        resolve(&fixture.0, None)
            .unwrap()
            .target_platform_version()
            .is_none()
    );
    let manifest = fixture.0.join("eska.toml");
    let input = fs::read_to_string(&manifest).unwrap()
        + "\n[analysis]\ntarget_platform_version = '8.5.1'\n";
    fs::write(&manifest, &input).unwrap();
    let config = ProjectConfig::from_toml(&input).unwrap();
    assert_eq!(
        ProjectConfig::from_toml(&config.to_toml().unwrap()).unwrap(),
        config
    );
    let context = resolve(&fixture.0, None).unwrap();
    assert_eq!(context.target_platform_version().unwrap().as_str(), "8.5.1");
    assert!(
        context.sources()[0]
            .project()
            .configuration()
            .build_settings()
            .platform_version()
            .is_none()
    );
    for invalid in [
        "8.3",
        "8.3.x",
        "8.3.27.1.2",
        "8.3.-1",
        " 8.3.27",
        "",
        "8.3.4294967296",
    ] {
        assert!(TargetPlatformVersion::parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
/// An application orders five extensions by declared edges and excludes unrelated members.
fn workspace_resolves_one_base_five_extensions_and_explicit_external_objects() {
    let fixture = TestDir::new();
    let names = [
        "main",
        "ext-1",
        "ext-2",
        "ext-3",
        "ext-4",
        "ext-5",
        "processing",
        "report",
        "unrelated",
    ];
    for name in names {
        let kind = if name.starts_with("ext-") {
            "extension"
        } else if name == "processing" {
            "processing"
        } else if name == "report" {
            "report"
        } else {
            "configuration"
        };
        let path = project(&fixture.0.join(name), kind, Some(name), None);
        if name == "unrelated" {
            fs::write(path, "not XML").unwrap();
        }
    }
    let declaration = "[analysis]\ndefault_application = 'accounting'\n[applications.accounting]\nconfiguration = 'main'\nextensions = ['ext-5', 'ext-4', 'ext-3', 'ext-2', 'ext-1']\nexternal_objects = ['processing', 'report']\nextension_dependencies = { ext-5 = ['ext-4'], ext-4 = ['ext-3'], ext-3 = ['ext-2'], ext-2 = ['ext-1'] }\n";
    workspace(&fixture.0, &names, declaration);
    let before = snapshot(&fixture.0);
    let context = resolve(&fixture.0, None).unwrap();
    assert_eq!(context.name().unwrap().as_str(), "accounting");
    assert_eq!(
        context
            .sources()
            .iter()
            .map(|source| source.member().unwrap().as_str())
            .collect::<Vec<_>>(),
        [
            "main",
            "ext-1",
            "ext-2",
            "ext-3",
            "ext-4",
            "ext-5",
            "processing",
            "report"
        ]
    );
    assert_eq!(context.sources()[0].role(), SourceRole::Configuration);
    assert_eq!(context.sources()[6].role(), SourceRole::ExternalObject);
    assert_eq!(
        context.target_platform_version().unwrap().as_str(),
        "8.3.27.2325"
    );
    assert!(!context.limited_context());
    let input = fs::read_to_string(fixture.0.join("eska.toml")).unwrap();
    let config = WorkspaceConfig::from_toml(&input).unwrap();
    assert_eq!(
        WorkspaceConfig::from_toml(&config.to_toml().unwrap()).unwrap(),
        config
    );
    assert_eq!(snapshot(&fixture.0), before);
}

#[test]
/// Independent applications cannot be merged or selected merely by directory proximity.
fn multiple_applications_require_a_default_or_explicit_selection() {
    let fixture = TestDir::new();
    for name in ["first", "second"] {
        project(&fixture.0.join(name), "configuration", Some(name), None);
    }
    let applications = "[applications.first]\nconfiguration = 'first'\n[applications.second]\nconfiguration = 'second'\ntarget_platform_version = '8.5.1'\n";
    workspace(&fixture.0, &["first", "second"], applications);
    assert!(matches!(
        resolve(&fixture.0.join("first"), None),
        Err(ContextError::AmbiguousApplication { .. })
    ));
    let selected = resolve(&fixture.0, Some("second")).unwrap();
    assert_eq!(selected.sources().len(), 1);
    assert_eq!(selected.sources()[0].member().unwrap().as_str(), "second");
    assert_eq!(
        selected.target_platform_version().unwrap().as_str(),
        "8.5.1"
    );
    assert!(matches!(
        resolve(&fixture.0, Some("../first")),
        Err(ContextError::UnknownApplication { .. })
    ));
    workspace(
        &fixture.0,
        &["first", "second"],
        &format!("[analysis]\ndefault_application = 'first'\n{applications}"),
    );
    assert_eq!(
        resolve(&fixture.0, None).unwrap().name().unwrap().as_str(),
        "first"
    );
}

#[test]
/// An uncomposed workspace remains usable for metadata but cannot silently drop analysis members.
fn implicit_context_requires_exactly_one_member() {
    let fixture = TestDir::new();
    project(&fixture.0.join("only"), "report", Some("only"), None);
    workspace(&fixture.0, &["only"], "");
    let context = resolve(&fixture.0, Some("only")).unwrap();
    assert!(context.limited_context());
    assert_eq!(context.name().unwrap().as_str(), "only");
    workspace(&fixture.0, &[], "");
    assert!(matches!(
        resolve(&fixture.0, None),
        Err(ContextError::EmptyWorkspace)
    ));
    project(
        &fixture.0.join("other"),
        "configuration",
        Some("other"),
        None,
    );
    workspace(&fixture.0, &["only", "other"], "");
    assert!(matches!(
        resolve(&fixture.0, None),
        Err(ContextError::ApplicationDeclarationsMissing { .. })
    ));
}

#[test]
/// Invalid graph edges, duplicates and cycles fail at schema loading before any BSL access.
fn graph_and_root_selection_are_strict() {
    assert!(matches!(
        settings_error("[analysis]\ndefault_application = 'missing'"),
        SettingsError::UnknownDefaultApplication { .. }
    ));
    assert!(matches!(
        settings_error("[applications.Main]\nconfiguration = 'main'"),
        SettingsError::InvalidName { .. }
    ));
    for extra in ["extensions = ['one', 'one']", "extensions = ['main']"] {
        assert!(matches!(
            settings_error(&format!(
                "[applications.app]\nconfiguration = 'main'\n{extra}"
            )),
            SettingsError::DuplicateMember { .. }
        ));
    }
    for edges in ["{ one = ['two'] }", "{ two = ['one'] }"] {
        assert!(matches!(
            settings_error(&format!(
                "[applications.app]\nconfiguration = 'main'\nextensions = ['one']\nextension_dependencies = {edges}"
            )),
            SettingsError::DependencyOutsideApplication { .. }
        ));
    }
    for edges in ["{ one = ['one'] }", "{ one = ['two'], two = ['one'] }"] {
        assert!(matches!(
            settings_error(&format!(
                "[applications.app]\nconfiguration = 'main'\nextensions = ['one', 'two']\nextension_dependencies = {edges}"
            )),
            SettingsError::DependencyCycle { .. }
        ));
    }
    assert!(matches!(
        settings_error(
            "[analysis]\ntarget_platform_version = '8.3.27'\n[applications.app]\nconfiguration = 'main'"
        ),
        SettingsError::TargetPlatformScope
    ));
    assert!(
        WorkspaceConfig::from_toml("[workspace]\nmembers = []\n[analysis]\nunknown = true")
            .is_err()
    );
    assert!(
        ProjectConfig::from_toml(
            "[project]\ntype = 'report'\n[applications.app]\nconfiguration = 'main'"
        )
        .is_err()
    );
}

#[test]
/// Member references are checked against parsed types, including members of unused applications.
fn missing_members_wrong_roles_and_member_settings_are_rejected() {
    let fixture = TestDir::new();
    project(&fixture.0.join("main"), "configuration", Some("main"), None);
    project(&fixture.0.join("ext"), "extension", Some("ext"), None);
    for (declaration, reason) in [
        ("configuration = 'absent'", "unknown"),
        ("configuration = 'ext'", "type"),
        ("configuration = 'main'\nexternal_objects = ['ext']", "type"),
    ] {
        workspace(
            &fixture.0,
            &["main", "ext"],
            &format!("[applications.app]\n{declaration}"),
        );
        let ContextError::Discovery(error) = resolve(&fixture.0, None).unwrap_err() else {
            panic!("discovery error")
        };
        let ContextDiscoveryError::Manifest {
            source:
                crate::config::ManifestConfigError::Workspace(WorkspaceConfigError::InvalidAnalysis(
                    error,
                )),
            ..
        } = *error
        else {
            panic!("configuration error")
        };
        assert!(matches!(
            (&*error, reason),
            (SettingsError::UnknownMember { .. }, "unknown")
                | (SettingsError::WrongMemberType { .. }, "type")
        ));
    }
    let manifest = fixture.0.join("ext/eska.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest).unwrap()
            + "\n[analysis]\ntarget_platform_version = '8.3.27'\n",
    )
    .unwrap();
    workspace(
        &fixture.0,
        &["main", "ext"],
        "[applications.app]\nconfiguration = 'main'\nextensions = ['ext']",
    );
    assert!(resolve(&fixture.0, None).is_err());
}

#[test]
/// Compatibility uses only the exact platform namespace and refuses ambiguous shapes.
fn compatibility_does_not_accept_foreign_or_duplicate_fields() {
    let fixture = TestDir::new();
    let path = project(&fixture.0, "configuration", None, None);
    let original = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        original.replace(
            "<CompatibilityMode>Version8_3_20</CompatibilityMode>",
            "<f:CompatibilityMode xmlns:f=\"urn:foreign\">Version8_5_1</f:CompatibilityMode>",
        ),
    )
    .unwrap();
    assert_eq!(
        resolve(&fixture.0, None).unwrap().sources()[0].compatibility_mode(),
        None
    );
    fs::write(
        &path,
        original.replace(
            "</CompatibilityMode>",
            "</CompatibilityMode><CompatibilityMode>Version8_5_1</CompatibilityMode>",
        ),
    )
    .unwrap();
    assert!(matches!(
        resolve(&fixture.0, None),
        Err(ContextError::Source(_))
    ));
    fs::write(path, "malformed XML").unwrap();
    assert!(matches!(
        resolve(&fixture.0, None),
        Err(ContextError::Source(_))
    ));
}

#[test]
/// The existing root classifier rejects a declared configuration containing an extension export.
fn xml_root_type_is_verified_after_manifest_selection() {
    let fixture = TestDir::new();
    project(&fixture.0, "extension", None, None);
    let manifest = fixture.0.join("eska.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest)
            .unwrap()
            .replace("\"extension\"", "\"configuration\""),
    )
    .unwrap();
    assert!(
        matches!(resolve(&fixture.0, None), Err(ContextError::Source(error)) if matches!(*error, SourceError::InvalidRoot { .. }))
    );
}

#[cfg(unix)]
#[test]
/// Canonical source containment rejects a symlink to an adjacent directory before reading XML.
fn source_symlink_cannot_escape_its_declared_project() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    let outside = fixture.0.join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'configuration'\nsource = 'src'",
    )
    .unwrap();
    std::os::unix::fs::symlink(outside, root.join("src")).unwrap();
    assert!(matches!(
        resolve(&root, None),
        Err(ContextError::Discovery(_))
    ));
}

#[test]
/// Portable context identifiers remain distinct from XML names and target strings.
fn typed_values_expose_only_validated_machine_values() {
    let name = ProjectName::parse("accounting".into()).unwrap();
    assert_eq!(serde_json::to_string(&name).unwrap(), "\"accounting\"");
    assert_eq!(
        SourceRole::ExternalObject.project_types(),
        &[ProjectType::Processing, ProjectType::Report]
    );
}

#[test]
/// A main member's effective version wins over workspace defaults and extension build overrides.
fn target_platform_follows_the_main_configuration_until_explicitly_overridden() {
    let fixture = TestDir::new();
    project(
        &fixture.0.join("main"),
        "configuration",
        Some("main"),
        Some("8.3.28.1000"),
    );
    project(
        &fixture.0.join("ext"),
        "extension",
        Some("ext"),
        Some("8.5.1.1234"),
    );
    let declaration = "[applications.app]\nconfiguration = 'main'\nextensions = ['ext']\n";
    workspace(&fixture.0, &["main", "ext"], declaration);
    let context = resolve(&fixture.0, None).unwrap();
    assert_eq!(
        context.target_platform_version().unwrap().as_str(),
        "8.3.28.1000"
    );
    workspace(
        &fixture.0,
        &["main", "ext"],
        &(declaration.to_owned() + "target_platform_version = '8.5.1'\n"),
    );
    let context = resolve(&fixture.0, None).unwrap();
    assert_eq!(context.target_platform_version().unwrap().as_str(), "8.5.1");
    assert_eq!(
        context.sources()[0].compatibility_mode(),
        Some("Version8_3_20")
    );
}

#[test]
/// Ready extensions retain their declaration order even when dependencies occur later in the list.
fn dependency_order_is_deterministic_for_unrelated_extensions() {
    let config = WorkspaceConfig::from_toml("[workspace]\nmembers = []\n[applications.app]\nconfiguration = 'main'\nextensions = ['third', 'first', 'second']\nextension_dependencies = {third = ['first']}\n").unwrap();
    let (name, application) = config
        .analysis_settings()
        .applications()
        .first_key_value()
        .unwrap();
    let ordered = super::graph::ordered_extensions(name, application).unwrap();
    assert_eq!(
        ordered.iter().map(ProjectName::as_str).collect::<Vec<_>>(),
        ["first", "third", "second"]
    );
}
