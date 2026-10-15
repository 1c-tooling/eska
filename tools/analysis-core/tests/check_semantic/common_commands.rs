//! Saved checker rules use the same command parameter inference as the resident SDK.

use super::*;

const MODULE: &str = "CommonCommands/Run/CommandModule.bsl";
const COMMAND: &str = "CommonCommands/Run/command.yaml";

/// A real reference parameter and its multiple-selection element both expose the exact native missing-field rule.
#[test]
fn saved_command_parameter_types_reach_checker_diagnostics_in_both_locales() {
    let (directory, root) = support::project("// Independent module.\n", false);
    crate::command_project::add_commands(
        &root,
        &directory.0.join("native"),
        "Procedure CommandProcessing(Chosen,Context)\n Result=Chosen.Absent;\nEndProcedure\n",
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    settings(&root, &[DiagnosticCode::UnresolvedField]);
    let source = root.join("src-with-common-commands");
    let saved = fs::read_to_string(source.join(COMMAND)).unwrap();
    let reference = saved.replace("x:string", "cfg:CatalogRef.Items");
    assert_ne!(reference, saved);
    for (mode, receiver) in [
        ("Single", "Chosen.Absent"),
        ("Multiple", "Chosen[0].Absent"),
    ] {
        let yaml = reference.replace(
            "<ParameterUseMode>Single</ParameterUseMode>",
            &format!("<ParameterUseMode>{mode}</ParameterUseMode>"),
        );
        fs::write(source.join(COMMAND), &yaml).unwrap();
        let body = format!(
            "Procedure CommandProcessing(Chosen,Context)\n Result={receiver};\nEndProcedure\n"
        );
        fs::write(source.join(MODULE), &body).unwrap();
        let mut variants = Vec::new();
        for locale in ["ru-RU", "en-US"] {
            let output = run(
                &root,
                locale,
                &["--profile", "semantic", "--format", "json"],
            );
            assert_eq!(
                output.status.code(),
                Some(1),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            let value = report(&output, 1);
            let batch = &value["data"]["report"];
            assert_eq!(
                batch["coverage"]["semantic_profile_complete"], true,
                "{value}"
            );
            let file = batch["files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|file| file["source"] == MODULE)
                .unwrap();
            assert_eq!(file["findings"].as_array().unwrap().len(), 1, "{file}");
            assert_eq!(file["findings"][0]["code"], "UnresolvedField");
            variants.push(value);
        }
        assert_eq!(variants[0], variants[1]);
        assert_eq!(fs::read_to_string(source.join(COMMAND)).unwrap(), yaml);
        assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), body);
        fs::write(
            source.join(COMMAND),
            yaml.replace("<t:Type>cfg:CatalogRef.Items</t:Type>", ""),
        )
        .unwrap();
        for locale in ["ru-RU", "en-US"] {
            report(
                &run(
                    &root,
                    locale,
                    &["--profile", "semantic", "--format", "json"],
                ),
                0,
            );
        }
    }
}
