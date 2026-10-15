//! Companion admission requires exact package/protocol fields and typed operation capabilities.

use super::*;
use serde_json::json;

/// Checker package versions differ from the linked project core; protocol and core remain exact.
#[test]
fn checker_admission_checks_core_protocol_and_requested_profile() {
    let original = json!({"schema_version":1,"tool":"eska-check","command":"capabilities","status":"success",
        "data":{"version":"0.0.0","source_core_version":env!("CARGO_PKG_VERSION"),
        "protocol_version":1,"commands":["check"],"profiles":["syntax","semantic"],
        "source_formats":["eska"],"report_schema_version":1}});
    for profile in ["syntax", "semantic"] {
        assert_eq!(
            validate(&original, Kind::Check, Capability::Check(profile)),
            Ok(())
        );
    }
    for (pointer, value) in [
        ("/data/version", json!("")),
        ("/data/source_core_version", json!("incompatible")),
        ("/data/protocol_version", json!(2)),
        ("/data/profiles", json!(["semantic"])),
        ("/data/commands", json!(["check-preview"])),
        ("/data/source_formats", json!(["designer-xml"])),
        ("/data/report_schema_version", json!(2)),
    ] {
        let mut report = original.clone();
        *report.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            validate(&report, Kind::Check, Capability::Check("syntax")),
            Err(Error::Incompatible),
            "{pointer}"
        );
    }
}

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

#[test]
/// Import admission verifies the selected native type and independent preview/write/recovery contract.
fn source_import_requires_each_specific_capability() {
    let original = json!({"schema_version":1,"tool":"eska-build","command":"capabilities","status":"success",
        "data":{"version":env!("CARGO_PKG_VERSION"),"protocol_version":1,"commands":["import"],
        "eska_artifact_types":["configuration","extension","processing","report"],"source_import_formats":["eska"],
        "source_import_mode":"preview_then_explicit_write","source_import_ancestor":"independently_confirmed_sha256",
        "source_import_recovery":"eska-project recover-import"}});
    for kind in [
        ProjectType::Configuration,
        ProjectType::Extension,
        ProjectType::Processing,
        ProjectType::Report,
    ] {
        assert_eq!(
            validate(&original, Kind::Build, Capability::SourceImport(kind)),
            Ok(())
        );
    }
    for (key, value) in [
        ("commands", json!(["build"])),
        ("eska_artifact_types", json!(["configuration"])),
        ("source_import_formats", json!(["designer-xml"])),
        ("source_import_mode", json!("implicit_write")),
        ("source_import_ancestor", json!("local_state")),
        ("source_import_recovery", json!("none")),
    ] {
        let mut report = original.clone();
        report["data"][key] = value;
        assert_eq!(
            validate(
                &report,
                Kind::Build,
                Capability::SourceImport(ProjectType::Report)
            ),
            Err(Error::Incompatible),
            "{key}"
        );
    }
}
