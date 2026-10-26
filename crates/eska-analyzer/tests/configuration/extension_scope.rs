//! Deployment scope survives generation, policy refresh and explicit foreign-config migration.

use super::*;

/// Keep the selected extension identity independent of its XML name.
fn workspace(root: &Path, scope: Option<&str>) {
    project(&root.join("base"), ProjectType::Configuration);
    project(&root.join("fixes"), ProjectType::Extension);
    for name in ["base", "fixes"] {
        let path = root.join(name).join("eska.toml");
        let config = ProjectConfig::load(&path)
            .unwrap()
            .with_name(eska_project_core::project::ProjectName::parse(name.into()).unwrap());
        fs::write(path, config.to_toml().unwrap()).unwrap();
    }
    fs::write(root.join("eska.toml"), manifest(scope)).unwrap();
}

/// Omitted scope selects `InfoBase`; an override lives beside the extension path.
fn manifest(scope: Option<&str>) -> String {
    let scopes = scope.map_or_else(String::new, |scope| format!(",scope={scope:?}"));
    format!(
        "[workspace.members.app]\nconfiguration='base'\nextensions=[{{path='fixes'{scopes}}}]\n[tools.bsl-analyzer.diagnostics.parameters]\nLineLength=false\n"
    )
}

/// Changing deployment updates the derived file while retaining diagnostic policy.
#[test]
#[cfg(unix)]
fn scope_changes_regenerate_the_owned_configuration() {
    let fixture = TestDir::new();
    workspace(&fixture.0, None);
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: fake_analyzer(&fixture.0),
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let first = configuration::prepare(
        &configuration::inspect(&fixture.0, None).unwrap(),
        &analyzer,
        false,
    )
    .unwrap();
    for scope in [Some("DataSeparation"), None] {
        fs::write(fixture.0.join("eska.toml"), manifest(scope)).unwrap();
        let stale = configuration::inspect(&fixture.0, None).unwrap();
        assert_eq!(stale.status, Status::Stale);
        let current = configuration::prepare(&stale, &analyzer, false).unwrap();
        let generated: toml::Value =
            toml::from_str(std::str::from_utf8(&current.contents).unwrap()).unwrap();
        assert_eq!(
            generated["source"]["extensions"][0]["scope"].as_str(),
            Some(scope.unwrap_or("InfoBase"))
        );
        assert_eq!(
            generated["diagnostics"]["parameters"]["LineLength"].as_bool(),
            Some(false)
        );
        assert_eq!(current.status, Status::Current);
    }
    assert_eq!(
        configuration::inspect(&fixture.0, None).unwrap().contents,
        first.contents
    );
}

/// Migration must neither discard a foreign scope nor report a conflict when it agrees with ESKA.
#[test]
fn migration_compares_explicit_extension_scopes() {
    for scope in [None, Some("InfoBase"), Some("DataSeparation")] {
        let fixture = TestDir::new();
        workspace(&fixture.0, scope);
        let original = "[source]\nroot='base/src'\nextensions=[{name='fixes',path='fixes/src',scope='InfoBase'}]\n";
        fs::write(fixture.0.join("bsl-analyzer.toml"), original).unwrap();
        let preview = migration::preview(&fixture.0, None).unwrap();
        assert_eq!(
            preview.conflicts.is_empty(),
            scope != Some("DataSeparation")
        );
        assert_eq!(
            fs::read_to_string(fixture.0.join("bsl-analyzer.toml")).unwrap(),
            original
        );
    }
}

/// The published executable must accept both scopes and omit its unknown-deployment warning.
#[test]
#[ignore = "requires ESKA_BSL_ANALYZER pointing to published bsl-analyzer-app 0.4.0"]
fn published_analyzer_accepts_explicit_scopes_without_legacy_warning() {
    let fixture = TestDir::new();
    workspace(&fixture.0, None);
    let binary = PathBuf::from(std::env::var_os("ESKA_BSL_ANALYZER").expect("published analyzer"));
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: binary.clone(),
        timeout: Duration::from_secs(60),
        cancelled: &cancelled,
        connection_closed: None,
    };
    for scope in [None, Some("DataSeparation")] {
        fs::write(fixture.0.join("eska.toml"), manifest(scope)).unwrap();
        let current = configuration::prepare(
            &configuration::inspect(&fixture.0, None).unwrap(),
            &analyzer,
            false,
        )
        .unwrap();
        let output = std::process::Command::new(&binary)
            .current_dir(&fixture.0)
            .arg("check-config")
            .arg("--config")
            .arg(current.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let logs = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !logs.contains("Extensions without an installation scope"),
            "{logs}"
        );
    }
}
