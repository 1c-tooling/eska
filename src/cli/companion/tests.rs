//! Companion admission requires exact package/protocol fields and typed operation capabilities.

use super::*;
use serde_json::json;

/// A package-compatible report advertises only current source-only creation requirements.
fn project() -> Value {
    json!({"schema_version":1,"tool":"eska-project","command":"capabilities","status":"success",
        "data":{"version":env!("CARGO_PKG_VERSION"),"protocol_version":1,"commands":["new","import"],
        "new_project_types":["configuration","extension","processing","report"],"new_source_layout":"queries",
        "initial_ancestor_layouts":["queries"],"import_layouts":["queries"],
        "import_layout_project_types":{"queries":["configuration","extension","processing","report"]}}})
}

#[test]
/// Every consumed New profile and Queries import share exact version/protocol admission.
fn admits_complete_source_creation_and_refuses_independent_contract_mismatches() {
    for kind in [
        ProjectType::Configuration,
        ProjectType::Extension,
        ProjectType::Processing,
        ProjectType::Report,
    ] {
        assert_eq!(
            validate(&project(), Kind::Project, Capability::New(kind)),
            Ok(())
        );
    }
    assert_eq!(
        validate(&project(), Kind::Project, Capability::ImportQueries),
        Ok(())
    );
    for (pointer, value) in [
        ("/schema_version", json!(2)),
        ("/tool", json!("eska-build")),
        ("/command", json!("build")),
        ("/status", json!("error")),
        ("/data/version", json!("0.0.0")),
        ("/data/protocol_version", json!(2)),
        ("/data/commands", json!(["new-project"])),
        ("/data/new_project_types", json!("configuration")),
        ("/data/new_source_layout", json!("family")),
        ("/data/initial_ancestor_layouts", json!(["family"])),
    ] {
        let mut report = project();
        *report.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            validate(
                &report,
                Kind::Project,
                Capability::New(ProjectType::Configuration)
            ),
            Err(Error::Incompatible),
            "{pointer}"
        );
    }
    let mut report = project();
    report["data"]["import_layout_project_types"]["queries"] =
        json!(["configuration", "extension", "processing"]);
    assert_eq!(
        validate(&report, Kind::Project, Capability::ImportQueries),
        Err(Error::Incompatible)
    );
}
