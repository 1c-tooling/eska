//! Generated analyzer style follows the same manifest layers and snapshots as the standalone fmt.

use super::*;

/// Inspect actual generated TOML without publishing or requiring the external analyzer.
fn generated(root: &Path) -> toml::Value {
    let plan = configuration::inspect(root, None).unwrap();
    toml::from_str(std::str::from_utf8(&plan.contents).unwrap()).unwrap()
}

/// New structural gaps participate in the analyzer's maximum without leaking formatter-only keys.
#[test]
fn readability_gaps_raise_the_analyzers_allowed_empty_line_count() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let path = root.join("eska.toml");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap()
            + "\n[format]\nprofile='eska'\n[format.bsl]\nblank_lines_around_multiline_statements=2\nblank_lines_between_object_groups=3\n",
    ).unwrap();
    let value = generated(&root);
    assert_eq!(
        value["diagnostics"]["parameters"]["ConsecutiveEmptyLines"]["allowedEmptyLinesCount"]
            .as_integer(),
        Some(3)
    );
    assert!(
        value["formatting"]
            .get("blank_lines_between_object_groups")
            .is_none()
    );
}

/// Selecting ESKA changes the analyzer threshold while preserving explicit unrelated rule parameters.
#[test]
fn preset_controls_supported_analyzer_style_parameters() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let path = root.join("eska.toml");
    let before = fs::read_to_string(&path).unwrap()
        + "\n[format]\nprofile='eska'\n[tools.bsl-analyzer.diagnostics.parameters.LineLength]\nmaxLineLength=120\ncheckMethodDescription=false\n";
    fs::write(&path, &before).unwrap();
    let value = generated(&root);
    assert_eq!(
        value["diagnostics"]["parameters"]["LineLength"]["maxLineLength"].as_integer(),
        Some(150)
    );
    assert_eq!(
        value["diagnostics"]["parameters"]["LineLength"]["checkMethodDescription"].as_bool(),
        Some(false)
    );
    assert_eq!(
        value["diagnostics"]["parameters"]["ConsecutiveEmptyLines"]["allowedEmptyLinesCount"]
            .as_integer(),
        Some(1)
    );
    assert_eq!(value["formatting"]["use_tabs"].as_bool(), Some(true));
    assert_eq!(value["formatting"]["indent_size"].as_integer(), Some(1));
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
    assert!(!root.join("bsl-analyzer.toml").exists());
}

/// Custom profiles and explicit formatting overrides win over the preset's defaults.
#[test]
fn overrides_and_structural_empty_line_minima_reach_analyzer() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let path = root.join("eska.toml");
    fs::write(&path, fs::read_to_string(&path).unwrap() + "\n[format]\nprofile='team'\n[format.profiles.team]\nbased_on='eska'\n[format.profiles.team.common]\ncolumn_limit=132\nindent_style='space'\nindent_width=2\n[format.bsl]\nblank_lines_before_return=3\n").unwrap();
    let value = generated(&root);
    assert_eq!(
        value["diagnostics"]["parameters"]["LineLength"]["maxLineLength"].as_integer(),
        Some(132)
    );
    assert_eq!(
        value["diagnostics"]["parameters"]["ConsecutiveEmptyLines"]["allowedEmptyLinesCount"]
            .as_integer(),
        Some(3)
    );
    assert_eq!(value["formatting"]["use_tabs"].as_bool(), Some(false));
    assert_eq!(value["formatting"]["indent_size"].as_integer(), Some(2));
}

/// A disabled diagnostic stays disabled; a disabled formatter width disables only `LineLength`.
#[test]
fn explicit_rule_disables_and_unlimited_columns_are_preserved() {
    for policy in [
        "[format]\nprofile='eska'\n[tools.bsl-analyzer.diagnostics.parameters]\nLineLength=false\nConsecutiveEmptyLines=false\n",
        "[format.common]\ncolumn_limit=0\n[tools.bsl-analyzer.diagnostics.parameters]\nConsecutiveEmptyLines=false\n",
    ] {
        let fixture = TestDir::new();
        let root = fixture.0.join("project");
        project(&root, ProjectType::Configuration);
        let path = root.join("eska.toml");
        fs::write(&path, fs::read_to_string(&path).unwrap() + "\n" + policy).unwrap();
        let value = generated(&root);
        assert_eq!(
            value["diagnostics"]["parameters"]["LineLength"].as_bool(),
            Some(false)
        );
        assert_eq!(
            value["diagnostics"]["parameters"]["ConsecutiveEmptyLines"].as_bool(),
            Some(false)
        );
    }
}

/// Legacy analyzer-only projects keep their original formatting until a modern style is selected.
#[test]
fn absent_formatting_policy_does_not_replace_legacy_analyzer_configuration() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let path = root.join("eska.toml");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap()
            + "\n[tools.bsl-analyzer.formatting]\nuse_tabs=true\nindent_size=2\n",
    )
    .unwrap();
    let value = generated(&root);
    assert_eq!(value["formatting"]["indent_size"].as_integer(), Some(2));
    assert!(value.get("diagnostics").is_none());
}

/// The selected main member can inherit a workspace profile and override its line budget.
#[test]
fn workspace_and_main_member_profiles_use_the_same_inheritance_as_fmt() {
    let fixture = TestDir::new();
    let base = fixture.0.join("base");
    project(&base, ProjectType::Configuration);
    fs::write(
        base.join("eska.toml"),
        "[project]\nname='base'\ntype='configuration'\n[format.common]\ncolumn_limit=144\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("eska.toml"),
        "[workspace.members.app]\nconfiguration='base'\n[format]\nprofile='eska'\n",
    )
    .unwrap();
    let value = generated(&fixture.0);
    assert_eq!(
        value["diagnostics"]["parameters"]["LineLength"]["maxLineLength"].as_integer(),
        Some(144)
    );
    assert_eq!(value["formatting"]["indent_size"].as_integer(), Some(1));
}

/// A reference configuration supplies types, while the external object's manifest supplies its style.
#[test]
fn external_reference_does_not_replace_the_external_objects_style() {
    let fixture = TestDir::new();
    let root = fixture.0.join("report");
    project(&root, ProjectType::Report);
    project(&fixture.0.join("base"), ProjectType::Configuration);
    let path = root.join("eska.toml");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap()
            + "\n[analysis.configuration]\nsource='../base/src'\n[format]\nprofile='eska'\n",
    )
    .unwrap();
    assert_eq!(
        generated(&root)["diagnostics"]["parameters"]["LineLength"]["maxLineLength"].as_integer(),
        Some(150)
    );
}

/// A path-specific formatting exception cannot become a global analyzer limit for every module.
#[test]
fn path_overrides_are_not_applied_to_the_global_analyzer_policy() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let path = root.join("eska.toml");
    fs::write(&path, fs::read_to_string(&path).unwrap() + "\n[format]\nprofile='eska'\n[[format.overrides]]\ninclude=['**']\n[format.overrides.common]\ncolumn_limit=99\n").unwrap();
    let value = generated(&root);
    assert_eq!(
        value["diagnostics"]["parameters"]["LineLength"]["maxLineLength"].as_integer(),
        Some(150)
    );
}

/// A changed preset makes the owned config stale and invalidates an already reviewed plan.
#[test]
#[cfg(unix)]
fn changing_the_preset_regenerates_owned_analyzer_configuration() {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    project(&root, ProjectType::Configuration);
    let path = root.join("eska.toml");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, original.clone() + "\n[format]\nprofile='1c'\n").unwrap();
    let cancelled = AtomicBool::new(false);
    let analyzer = Analyzer {
        binary: fake_analyzer(&fixture.0),
        timeout: Duration::from_secs(5),
        cancelled: &cancelled,
        connection_closed: None,
    };
    let current = configuration::prepare(
        &configuration::inspect(&root, None).unwrap(),
        &analyzer,
        false,
    )
    .unwrap();
    fs::write(&path, original + "\n[format]\nprofile='eska'\n").unwrap();
    assert!(matches!(
        configuration::lease(&current),
        Err(Error::StalePreview)
    ));
    let stale = configuration::inspect(&root, None).unwrap();
    assert_eq!(stale.status, Status::Stale);
    let current = configuration::prepare(&stale, &analyzer, false).unwrap();
    assert_eq!(current.status, Status::Current);
    assert_eq!(
        generated(&root)["diagnostics"]["parameters"]["LineLength"]["maxLineLength"].as_integer(),
        Some(150)
    );
    assert_eq!(
        configuration::inspect(&root, None).unwrap().contents,
        current.contents
    );
}
