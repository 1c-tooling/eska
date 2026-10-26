//! One extension declaration owns its path, deployment scope and dependency edges.

use super::*;
use crate::application::ExtensionScope;

/// One physical extension can be installed differently in two independent applications.
#[test]
fn selected_application_resolves_scope_without_changing_membership() {
    let fixture = TestDir::new();
    project(&fixture.0.join("base"), "configuration", Some("base"), None);
    project(&fixture.0.join("overlay"), "extension", Some("fixes"), None);
    let input = "[workspace.members.whole]\nconfiguration='base'\nextensions=[{path='overlay'}]\n[workspace.members.tenant]\nconfiguration='base'\nextensions=[{path='overlay',scope='DataSeparation'}]\n";
    fs::write(fixture.0.join("eska.toml"), input).unwrap();
    let config = WorkspaceConfig::from_toml(input).unwrap();
    assert_eq!(config.members().len(), 2);
    assert_eq!(
        WorkspaceConfig::from_toml(&config.to_toml().unwrap()).unwrap(),
        config
    );
    for (name, expected) in [
        ("whole", ExtensionScope::InfoBase),
        ("tenant", ExtensionScope::DataSeparation),
    ] {
        let context = resolve(&fixture.0, Some(name)).unwrap();
        assert_eq!(context.sources()[0].extension_scope(), None);
        assert_eq!(context.sources()[1].extension_scope(), Some(expected));
        assert_eq!(context.sources()[1].member().unwrap().as_str(), "fixes");
    }
    let first = resolve_snapshot(&fixture.0, Some("whole"))
        .unwrap()
        .fingerprint;
    fs::write(
        fixture.0.join("eska.toml"),
        input.replace(
            "{path='overlay'}",
            "{path='overlay',scope='DataSeparation'}",
        ),
    )
    .unwrap();
    assert_ne!(
        resolve_snapshot(&fixture.0, Some("whole"))
            .unwrap()
            .fingerprint,
        first
    );
}

/// Only structured extension records are accepted; obsolete parallel maps cannot lose settings.
#[test]
fn scope_requires_an_extension_in_the_selected_application() {
    for declaration in [
        "extensions=['ext']",
        "extensions=[{path='ext',scope='Other'}]",
        "extensions=[{path='ext',unknown=true}]",
        "extensions=[{scope='InfoBase'}]",
        "extensions=[{path='ext',depends_on='other'}]",
        "extensions=[{path='ext'}]\nextension_scopes={ext='InfoBase'}",
        "extensions=[{path='ext'}]\nextension_dependencies={ext=[]}",
    ] {
        assert!(
            WorkspaceConfig::from_toml(&format!("[workspace.members.app]\n{declaration}")).is_err(),
            "{declaration}"
        );
    }
    assert!(matches!(
        WorkspaceConfig::from_toml("[workspace]\n[analysis]\nextension_scope='InfoBase'"),
        Err(WorkspaceConfigError::InvalidAnalysis(error)) if matches!(*error, SettingsError::ExtensionScopeLocation)
    ));
}

/// Standalone extensions share the `InfoBase` default and can explicitly select `DataSeparation`.
#[test]
fn standalone_extension_scope_defaults_and_is_type_checked() {
    let fixture = TestDir::new();
    for kind in ["extension", "configuration", "processing", "report"] {
        let root = fixture.0.join(kind);
        project(&root, kind, None, None);
        assert_eq!(
            resolve(&root, None).unwrap().sources()[0].extension_scope(),
            (kind == "extension").then_some(ExtensionScope::InfoBase)
        );
        let manifest = root.join("eska.toml");
        let text = fs::read_to_string(&manifest).unwrap()
            + "\n[analysis]\nextension_scope='DataSeparation'\n";
        fs::write(&manifest, &text).unwrap();
        let config = ProjectConfig::from_toml(&text).unwrap();
        assert_eq!(
            ProjectConfig::from_toml(&config.to_toml().unwrap()).unwrap(),
            config
        );
        match resolve(&root, None) {
            Ok(context) => {
                assert_eq!(kind, "extension");
                assert_eq!(
                    context.sources()[0].extension_scope(),
                    Some(ExtensionScope::DataSeparation)
                );
            }
            Err(ContextError::Settings(error)) => {
                assert!(matches!(*error, SettingsError::ExtensionScopeLocation));
            }
            other => panic!("unexpected context: {other:?}"),
        }
    }
}
