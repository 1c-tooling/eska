//! Checker consumes the same current visible CF/CFE inputs and project-root-anchored settings.

use std::fs;

use super::{report, run, support};
use support::composition_project as fixture;

/// Exact source/input bytes remain owned by this fixture while checker reports honest union coverage.
#[test]
fn composition_checker_has_current_visible_inputs_and_rejects_missing_adopted_source() {
    let directory = support::TestDirectory::new();
    let project = fixture::project(&directory.0);
    assert_eq!(project.selected_accepted_sha256.len(), 64);
    fixture::parse_only(&project);
    fs::write(
        project.roots["b"]
            .join("src")
            .join(project.module.as_path()),
        fixture::CALLER.replace("    Value.\n", ""),
    )
    .unwrap();
    fs::remove_file(
        project.roots["sibling"]
            .join("src")
            .join(project.module.as_path()),
    )
    .unwrap();
    let mut values = Vec::new();
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(
                &project.roots["b"],
                lang,
                &["--profile", "semantic", "--format", "json"],
            ),
            0,
        );
        let composition = &value["data"]["report"]["composition"];
        assert_eq!(composition["current_complete"], true, "{value}");
        let sibling = composition["roots"]
            .as_array()
            .unwrap()
            .iter()
            .find(|root| root["root"] == 3)
            .unwrap();
        assert_eq!(sibling["visible"], false);
        assert_eq!(sibling["retained_source_bytes"], 0);
        assert_eq!(sibling["module_inputs_complete"], false);
        values.push(value);
    }
    assert_eq!(values[0], values[1]);
    fs::remove_file(
        project.roots["a"]
            .join("src")
            .join(project.catalog.as_path()),
    )
    .unwrap();
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(
                &project.roots["b"],
                lang,
                &["--profile", "semantic", "--format", "json"],
            ),
            2,
        );
        assert_eq!(
            value["data"]["report"]["composition"]["current_complete"],
            false
        );
        assert_eq!(
            value["data"]["report"]["coverage"]["semantic_profile_complete"],
            false
        );
        assert_eq!(value["data"]["report"]["coverage"]["unchecked_modules"], 1);
    }
}

/// An explicit config in a subdirectory keeps source roots anchored to the selected project root.
#[test]
fn composition_checker_config_subdirectory_preserves_project_root_anchor() {
    let directory = support::TestDirectory::new();
    let project = fixture::project(&directory.0);
    fixture::parse_only(&project);
    fs::write(
        project.roots["b"]
            .join("src")
            .join(project.module.as_path()),
        fixture::CALLER.replace("    Value.\n", ""),
    )
    .unwrap();
    let config = project.roots["b"].join("settings/check.toml");
    fs::create_dir(config.parent().unwrap()).unwrap();
    fs::copy(&project.settings, &config).unwrap();
    fs::remove_file(&project.settings).unwrap();
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(
                &project.roots["b"],
                lang,
                &[
                    "--profile",
                    "semantic",
                    "--config",
                    "settings/check.toml",
                    "--format",
                    "json",
                ],
            ),
            0,
        );
        assert_eq!(
            value["data"]["report"]["composition"]["current_complete"],
            true
        );
        assert_eq!(value["data"]["settings_path"], config.to_str().unwrap());
    }
}
