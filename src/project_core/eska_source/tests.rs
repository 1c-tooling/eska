use super::*;
use crate::{
    project_core::{
        ProjectType,
        config::ProjectConfig,
        discovery::{discover, discover_context},
        selection::{SelectionIntent, select_projects},
        source_format::{SourcePath, accept_source, import_designer},
    },
    test_support::TestDir,
};
use std::fs;

/// Create a complete, small configuration and its byte-preserved accepted base in owned space.
fn project(root: &Path, name: &str) -> (Project, AcceptedRef) {
    fs::create_dir_all(root.join("native")).expect("native directory");
    fs::write(root.join("native/Configuration.xml"),
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name><Comment>Accepted</Comment><Version>1.2.3.4</Version></Properties></Configuration></MetaDataObject>"
    ).expect("root descriptor");
    import_designer(&root.join("native"), &root.join("src")).expect("source import");
    fs::create_dir(root.join(".eska")).expect("accepted parent");
    let reference = accept_source(&root.join("src"), &root.join(".eska/accepted"), &[])
        .expect("accepted bytes");
    fs::write(root.join("eska.toml"), format!(
        "[project]\nname = '{name}'\ntype = 'configuration'\nsource_format = 'eska'\n[eska]\naccepted_base = '.eska/accepted'\n"
    )).expect("project manifest");
    (discover(root).expect("discovered project"), reference)
}

#[test]
/// Reading is lazy and unprivileged; materialization always checks the caller's independent pin.
fn selected_source_reads_without_a_base_and_never_falls_back_to_leftover_xml() {
    let fixture = TestDir::new();
    let (project, reference) = project(&fixture.0, "sample");
    let source = EskaSource::open(project.clone(), ProjectScope::Standalone).expect("YAML source");
    assert_eq!(
        source.reader().configuration().declarations[0].name,
        "Sample"
    );
    assert_eq!(source.scope(), &ProjectScope::Standalone);
    fs::remove_dir_all(project.root().join(".eska/accepted")).expect("remove own base");
    assert_eq!(
        source
            .prepare(&reference, &fixture.0.join("generations"), true)
            .expect_err("no trust fallback")
            .error
            .code,
        "source_baseline_missing"
    );
    fs::copy(
        project.root().join("native/Configuration.xml"),
        project.source().join("Configuration.xml"),
    )
    .expect("leftover XML");
    fs::write(project.source().join("configuration.yaml"), b"broken: [").expect("invalid YAML");
    assert_eq!(
        EskaSource::open(project, ProjectScope::Standalone)
            .err()
            .expect("YAML error")
            .code,
        "source_yaml_invalid"
    );
    assert!(!fixture.0.join("generations").exists());
}

#[test]
/// Cache deletion and a mismatched pin cannot change which accepted input supplies an export.
fn preparation_uses_member_relative_accepted_bytes_and_recreates_the_same_generation() {
    let fixture = TestDir::new();
    let (project, reference) = project(&fixture.0.join("member"), "sample");
    let source = EskaSource::open(project, ProjectScope::Standalone).expect("YAML source");
    let generations = fixture.0.join("generations");
    let untrusted = AcceptedRef {
        sha256: "0".repeat(64),
    };
    assert_eq!(
        source
            .prepare(&untrusted, &generations, true)
            .expect_err("independent pin")
            .error
            .code,
        "source_baseline_untrusted"
    );
    assert!(!generations.exists());
    let first = source
        .prepare(&reference, &generations, true)
        .expect("prepared source");
    let checksum = first.source_map().checksum().expect("source map");
    fs::remove_dir_all(&generations).expect("delete disposable generations");
    let second = source
        .prepare(&reference, &generations, true)
        .expect("recreated source");
    assert_eq!(first.generation(), second.generation());
    assert_eq!(
        second.source_map().checksum().expect("source map"),
        checksum
    );
    assert_eq!(second.baseline(), reference.sha256);
    second.validate().expect("native bytes");
}

#[test]
/// Discovery retains each member's source settings and reads only the explicitly selected YAML.
fn mixed_workspace_preserves_inheritance_selection_and_member_scope() {
    let fixture = TestDir::new();
    let (first, _) = project(&fixture.0.join("first"), "first");
    let (other, _) = project(&fixture.0.join("other"), "other");
    fs::write(
        other.root().join("eska.toml"),
        "[project]\nname='other'\ntype='configuration'\nsource='native'\n",
    )
    .expect("Designer member");
    fs::write(fixture.0.join("eska.toml"),
        "[workspace]\nmembers=['first','other']\n[build]\nplatform_version='8.3.27.2325'\n[vcs.workflow]\npreset='trunk'\n")
        .expect("workspace");
    let context = discover_context(&fixture.0).expect("mixed workspace");
    let selected = select_projects(
        &context,
        &["first".to_owned()],
        false,
        SelectionIntent::ReadOnly,
    )
    .expect("one member");
    let selected = selected.projects()[0];
    assert_eq!(
        selected.project().configuration().source_format(),
        first.configuration().source_format()
    );
    assert_eq!(
        selected
            .project()
            .configuration()
            .build_settings()
            .platform_version()
            .expect("inherited platform")
            .as_str(),
        "8.3.27.2325"
    );
    assert_eq!(
        selected.project().configuration().workflow(),
        Some(super::super::workflow::WorkflowPreset::Trunk)
    );
    let scope = ProjectScope::Member(selected.name().expect("member name").clone());
    let source =
        EskaSource::open(selected.project().clone(), scope.clone()).expect("selected YAML");
    assert_eq!(source.scope(), &scope);
    fs::write(first.source().join("configuration.yaml"), b"broken: [")
        .expect("unselected bad YAML");
    let selected = select_projects(
        &context,
        &["other".to_owned()],
        false,
        SelectionIntent::ReadOnly,
    )
    .expect("Designer member");
    assert_eq!(
        selected.projects()[0]
            .project()
            .configuration()
            .source_format(),
        &SourceFormat::DesignerXml
    );
    let reloaded = discover_context(first.root()).expect("member context");
    let selected =
        select_projects(&reloaded, &[], false, SelectionIntent::ReadOnly).expect("current member");
    assert_eq!(
        selected.projects()[0].name().expect("name").as_str(),
        "first"
    );
}

#[test]
/// Builders keep existing workflow/build settings and serialize source settings for a member too.
fn config_round_trip_retains_source_settings_and_existing_policy() {
    let native = ProjectConfig::from_toml(
        "[project]\nname='sample'\ntype='configuration'\nsource='sources'\n[build]\nplatform_version='8.3.27.2325'\n[vcs.workflow]\npreset='trunk'\n"
    ).expect("native config");
    let source_format = SourceFormat::Eska {
        accepted_base: SourcePath::parse("policy/accepted").expect("path"),
    };
    let config = native
        .clone()
        .with_source_format(source_format)
        .expect("ESKA config");
    assert_eq!(
        config.configuration().workflow(),
        native.configuration().workflow()
    );
    assert_eq!(
        config.configuration().build_settings(),
        native.configuration().build_settings()
    );
    assert_eq!(config.source(), native.source());
    assert_eq!(config.name(), native.name());
    let text = config.to_toml().expect("serialize");
    assert!(text.contains("source_format = \"eska\""));
    assert!(text.contains("accepted_base = \"policy/accepted\""));
    assert_eq!(ProjectConfig::from_toml(&text).expect("reload"), config);
    let member = config.to_workspace_member_toml().expect("member manifest");
    assert!(member.contains("[eska]"));
    assert!(!member.contains("[vcs]"));
    assert_eq!(
        ProjectConfig::from_toml(&member)
            .expect("member")
            .configuration()
            .source_format(),
        config.configuration().source_format()
    );
}

#[test]
/// Only a supported format/profile can carry a portable baseline locator; no policy pin is parsed.
fn config_refuses_missing_unexpected_unsafe_and_unsupported_eska_settings() {
    for input in [
        "[project]\ntype='configuration'\nsource_format='eska'\n",
        "[project]\ntype='configuration'\n[eska]\naccepted_base='base'\n",
        "[project]\ntype='configuration'\nsource_format='eska'\n[eska]\naccepted_base='base'\nsha256='anything'\n",
    ] {
        assert!(ProjectConfig::from_toml(input).is_err(), "{input}");
    }
    for path in [
        "",
        "/outside",
        "../outside",
        "./base",
        "a//b",
        "a/../base",
        "NUL",
        "a\\b",
    ] {
        let input = format!(
            "[project]\ntype='configuration'\nsource_format='eska'\n[eska]\naccepted_base='{path}'\n"
        );
        assert!(ProjectConfig::from_toml(&input).is_err(), "{path}");
    }
    let config = ProjectConfig::new(ProjectType::Processing);
    assert!(
        config
            .with_source_format(SourceFormat::Eska {
                accepted_base: SourcePath::parse("base").expect("path"),
            })
            .is_ok()
    );
}

#[cfg(feature = "legacy")]
#[test]
/// Old XML readers and writers refuse the format before touching leftover descriptors or Git.
fn legacy_xml_operations_cannot_read_or_mutate_leftover_xml() {
    let fixture = TestDir::new();
    let (project, _) = project(&fixture.0, "sample");
    let xml = project.source().join("Configuration.xml");
    let bytes = fs::read(project.root().join("native/Configuration.xml")).expect("XML");
    fs::write(&xml, &bytes).expect("leftover XML");
    assert!(matches!(
        super::super::native_version::inspect(&project),
        Err(super::super::native_version::VersionError::SourceFormatUnsupported)
    ));
    assert!(matches!(
        super::super::native_version::bump(&project, super::super::native_version::BumpKind::Patch),
        Err(super::super::native_version::VersionError::SourceFormatUnsupported)
    ));
    assert!(matches!(
        crate::project::artifact::ImportPlan::inspect(&project),
        Err(crate::project::artifact::ArtifactError::SourceFormatUnsupported)
    ));
    assert!(matches!(
        crate::project::designer_source::open_projects(project.root(), &[], false),
        Err(crate::project::designer_source::SourceError::SourceFormatUnsupported)
    ));
    assert!(matches!(
        crate::project::object_model::discover(&project),
        Err(crate::project::object_model::ObjectModelError::SourceFormatUnsupported)
    ));
    assert_eq!(
        crate::project::patch::plan(&project, None)
            .expect_err("unsupported patch")
            .code,
        "source-format"
    );
    assert_eq!(fs::read(xml).expect("unchanged XML"), bytes);
}

#[test]
/// Accepted bytes cannot become part of the editable tree through parsing, builders or direct models.
fn accepted_base_must_not_overlap_sources_in_either_direction() {
    use crate::project_core::{ProjectConfiguration, config::ProjectConfigError};

    for (source, base) in [
        (".", "accepted"),
        ("src", "src"),
        ("src", "src/accepted"),
        ("accepted/src", "accepted"),
        ("./src", "src/accepted"),
    ] {
        let input = format!(
            "[project]\ntype='configuration'\nsource_format='eska'\nsource='{source}'\n[eska]\naccepted_base='{base}'\n"
        );
        assert!(matches!(
            ProjectConfig::from_toml(&input),
            Err(ProjectConfigError::EskaBaseOverlapsSource)
        ));
    }
    let format = SourceFormat::Eska {
        accepted_base: SourcePath::parse("accepted").expect("base"),
    };
    let config = ProjectConfig::new(ProjectType::Configuration)
        .with_source_format(format.clone())
        .expect("separate base");
    assert!(matches!(
        config.with_source("accepted/inside".into()),
        Err(ProjectConfigError::EskaBaseOverlapsSource)
    ));
    let config = ProjectConfig::new(ProjectType::Configuration)
        .with_source(".".into())
        .expect("native root sources");
    assert!(matches!(
        config.with_source_format(format.clone()),
        Err(ProjectConfigError::EskaBaseOverlapsSource)
    ));
    let fixture = TestDir::new();
    let project = Project::new(
        fixture.0.clone(),
        fixture.0.clone(),
        ProjectConfiguration::new(ProjectType::Configuration, format),
    )
    .expect("pure path model");
    assert_eq!(
        EskaSource::open(project, ProjectScope::Standalone)
            .err()
            .expect("overlap refusal")
            .code,
        "source_baseline_overlap"
    );
}
