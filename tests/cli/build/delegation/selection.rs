//! Selection and unsupported option errors precede companion/native process creation.

use super::fixture::{self, Fixture};
use std::fs;

#[test]
/// XML projects retain their local route and reject ESKA authority options before platform discovery.
fn source_options_do_not_change_designer_xml_execution() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.0.join("eska.toml"),
        "[project]\ntype='configuration'\n[build]\nplatform_version='8.3.27.2325'\n",
    )
    .unwrap();
    for options in [
        vec!["--accepted-base-sha256".to_owned(), "a".repeat(64)],
        vec!["--generations".to_owned(), "derived".to_owned()],
        vec!["--allow-exclusions".to_owned()],
    ] {
        let mut values = Vec::new();
        for locale in ["ru", "en"] {
            let output = fixture
                .command(locale)
                .args(["build", "--format", "json"])
                .args(&options)
                .output()
                .unwrap();
            values.push(fixture::failure(
                &output,
                "build-source-options-incompatible",
            ));
        }
        assert_eq!(values[0], values[1]);
    }
    assert!(!fixture.0.0.join("probe.bin").exists());
    assert!(!fixture.0.0.join("build").exists());
}

#[test]
/// Workspace/multiple and mixed-format selections never partially build an ESKA member.
fn refuses_aggregate_and_mixed_selection_before_tool_discovery() {
    for second_format in ["eska", "designer-xml"] {
        let fixture = Fixture::new();
        let config = fs::read_to_string(fixture.0.0.join("eska.toml")).unwrap();
        for name in ["first", "second"] {
            fs::create_dir_all(fixture.0.0.join(name).join("src")).unwrap();
            let input = if name == "second" && second_format == "designer-xml" {
                "[project]\nname='second'\ntype='configuration'\n".to_owned()
            } else {
                config.replace("facade-demo", name)
            };
            fs::write(fixture.0.0.join(name).join("eska.toml"), input).unwrap();
        }
        fs::write(
            fixture.0.0.join("eska.toml"),
            "[workspace]\nmembers=['first','second']\n[build]\nplatform_version='8.3.27.2325'\n",
        )
        .unwrap();
        for options in [
            &["--workspace"][..],
            &["-p", "first", "-p", "second"][..],
            &[][..],
        ] {
            for locale in ["ru", "en"] {
                let output = fixture
                    .command(locale)
                    .args(["build", "--format", "json"])
                    .args(options)
                    .output()
                    .unwrap();
                fixture::failure(&output, "build-eska-scope-unsupported");
            }
        }
        assert!(!fixture.0.0.join("probe.bin").exists());
        assert!(!fixture.0.0.join("build").exists());
    }
}

#[test]
/// Native output-type and interactive-platform restrictions survive delegation before any probe.
fn shell_preflight_precedes_capabilities() {
    let fixture = Fixture::new();
    fixture.sibling();
    for locale in ["ru", "en"] {
        let output = fixture
            .command(locale)
            .args(["build", "--format", "json", "--output", "wrong.cfe"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(!fixture.0.0.join("probe.bin").exists());
        let output = fixture
            .command(locale)
            .args(["build", "--format", "json", "--select-platform"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(!fixture.0.0.join("probe.bin").exists());
    }
}
