//! Regression coverage for the single membership schema and reference-only base configurations.

use super::*;

/// Paths, member identities and XML names remain independent; composition is declared once.
#[test]
fn role_paths_supply_members_and_context_without_a_second_table() {
    let fixture = TestDir::new();
    project(
        &fixture.0.join("основная база"),
        "configuration",
        Some("base"),
        None,
    );
    project(
        &fixture.0.join("исправления"),
        "extension",
        Some("fixes"),
        None,
    );
    fs::write(fixture.0.join("eska.toml"), "[workspace.members.accounting]\nconfiguration='основная база'\nextensions=['исправления']\n[build]\nplatform_version='8.3.27.2325'\n[tools.bsl-analyzer.diagnostics.parameters]\nLineLength=false\n").unwrap();
    let context = resolve(&fixture.0, None).unwrap();
    assert_eq!(
        context
            .sources()
            .iter()
            .map(|source| source.member().unwrap().as_str())
            .collect::<Vec<_>>(),
        ["base", "fixes"]
    );
    assert_eq!(context.sources()[1].role(), SourceRole::Extension);
    assert!(
        context
            .analyzer_policy()
            .as_toml()
            .contains("LineLength = false")
    );
    let crate::project::discovery::DiscoveryContext::Workspace { workspace, .. } =
        crate::project::discovery::discover_context(&fixture.0).unwrap()
    else {
        panic!("workspace");
    };
    assert_eq!(workspace.members().len(), 2);
}

/// A standalone EPF can use raw BSP XML elsewhere on disk without enrolling or modifying that base.
#[test]
fn standalone_processing_attaches_reference_xml_without_a_manifest() {
    let fixture = TestDir::new();
    let reference = TestDir::new();
    let descriptor = project(&reference.0, "configuration", None, None);
    fs::remove_file(reference.0.join("eska.toml")).unwrap();
    project(&fixture.0, "processing", None, Some("8.3.27.2325"));
    let manifest = fixture.0.join("eska.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest).unwrap()
            + &format!(
                "\n[analysis.configuration]\nsource={:?}\n",
                descriptor.parent().unwrap()
            ),
    )
    .unwrap();
    let before = snapshot(&reference.0);
    let first = resolve_snapshot(&fixture.0, None).unwrap();
    assert_eq!(first.context.sources().len(), 2);
    assert!(first.context.sources()[0].is_reference());
    assert!(!first.context.limited_context());
    assert_eq!(snapshot(&reference.0), before);
    fs::write(
        &descriptor,
        fs::read_to_string(&descriptor).unwrap() + "<!-- changed reference -->",
    )
    .unwrap();
    assert_ne!(
        resolve_snapshot(&fixture.0, None).unwrap().fingerprint,
        first.fingerprint
    );
}

/// Relative references are resolved from the root manifest and never become build/VCS members.
#[test]
fn workspace_reference_is_not_a_member_and_missing_reference_fails() {
    let fixture = TestDir::new();
    let root = fixture.0.join("work");
    project(&root.join("report"), "report", Some("my-report"), None);
    let descriptor = project(&fixture.0.join("база БСП"), "configuration", None, None);
    fs::remove_file(fixture.0.join("база БСП/eska.toml")).unwrap();
    fs::write(root.join("eska.toml"), "[workspace.members.tools]\nconfiguration={source='../база БСП/исходники XML'}\nexternal_objects=['report']\n").unwrap();
    let context = resolve(&root, None).unwrap();
    assert!(context.sources()[0].is_reference());
    let crate::project::discovery::DiscoveryContext::Workspace { workspace, .. } =
        crate::project::discovery::discover_context(&root).unwrap()
    else {
        panic!("workspace");
    };
    assert_eq!(workspace.members().len(), 1);
    assert_eq!(workspace.members()[0].name().as_str(), "my-report");
    fs::remove_file(descriptor).unwrap();
    assert!(matches!(resolve(&root, None), Err(ContextError::Source(_))));
}

/// Obsolete declarations are rejected rather than interpreted as another source of membership.
#[test]
fn old_and_mixed_workspace_schemas_are_rejected() {
    for input in [
        "[workspace]\nmembers=['base']\n",
        "[workspace]\n[applications.main]\nconfiguration='base'\n",
        "[workspace.members.main]\nconfiguration='base'\n[applications.main]\nconfiguration='base'\n",
    ] {
        assert!(WorkspaceConfig::from_toml(input).is_err());
    }
}

/// An empty workspace follows the same root-only policy as one that already has applications.
#[test]
fn empty_workspace_rejects_a_standalone_target_override() {
    assert!(matches!(
        WorkspaceConfig::from_toml(
            "[workspace]\n[analysis]\ntarget_platform_version='8.3.27'\n"
        ),
        Err(crate::config::WorkspaceConfigError::InvalidAnalysis(error))
            if matches!(*error, SettingsError::TargetPlatformScope)
    ));
}
