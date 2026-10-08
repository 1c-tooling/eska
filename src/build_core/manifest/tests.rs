use super::*;
use crate::{
    project_core::source_format::{
        AcceptedBase, PreparedSource, accept_source, import_designer, prepare_protected,
    },
    project_core::{ProjectConfiguration, ProjectType, SourceFormat},
    test_support::TestDir,
};

#[test]
/// Keep supplied and unavailable Git facts distinct in the existing wire schema.
fn serializes_caller_provenance_without_inventing_repository_facts() {
    for (provenance, expected) in [
        (
            GitProvenance::Available {
                commit: Some("0123456789abcdef".to_owned()),
                dirty: true,
            },
            serde_json::json!({"status": "available", "commit": "0123456789abcdef", "dirty": true}),
        ),
        (
            GitProvenance::Available {
                commit: None,
                dirty: false,
            },
            serde_json::json!({"status": "available", "commit": null, "dirty": false}),
        ),
        (
            GitProvenance::Absent,
            serde_json::json!({"status": "absent", "commit": null, "dirty": null}),
        ),
        (
            GitProvenance::Unavailable,
            serde_json::json!({"status": "unavailable", "commit": null, "dirty": null}),
        ),
    ] {
        assert_eq!(
            serde_json::to_value(GitDocument::from(provenance)).expect("Git JSON"),
            expected,
        );
    }
}

/// Keep native fixtures and a Git-named opaque payload under this test's owned directory.
fn prepared_project(root: &Path) -> (Project, BuildPlan, PreparedSource) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    let input = root.join("input");
    copy_directory(&fixture, &input, &fixture, &[], &mut Sha256::new(), true)
        .expect("fixture copy");
    // Synthetic CF bytes cover preservation; platform validity has its separate C1 gate.
    fs::create_dir(input.join("Ext/ParentConfigurations")).expect("supplier directory");
    fs::write(
        input.join("Ext/ParentConfigurations/SampleConfiguration.cf"),
        b"synthetic opaque CF sentinel",
    )
    .expect("supplier payload");
    fs::write(input.join(".git"), b"opaque native payload\r\n").expect("Git-named input payload");
    let source = root.join("source");
    import_designer(&input, &source).expect("source import");
    let base_path = root.join("base");
    let reference = accept_source(&source, &base_path, &[]).expect("base acceptance");
    let base = AcceptedBase::open(&base_path, &reference).expect("verified base");
    let prepared = prepare_protected(&source, &base, &root.join("generations"), true)
        .expect("prepared generation");
    let project = Project::new(
        root.to_owned(),
        prepared.xml().to_owned(),
        ProjectConfiguration::new(ProjectType::Configuration, SourceFormat::DesignerXml),
    )
    .expect("native project");
    let plan = BuildPlan::with_platform_version(
        &project,
        None,
        Some(super::super::PlatformVersion::parse("8.3.27.2325").expect("platform version")),
    )
    .expect("plan");
    (project, plan, prepared)
}

#[test]
/// Bind complete prepared bytes and policy context in schema 2 without dropping opaque payloads.
fn prepared_snapshot_preserves_complete_tree_and_records_authorized_context() {
    let fixture = TestDir::new();
    let (project, plan, prepared) = prepared_project(&fixture.0);
    let stage = fixture.0.join("stage");
    fs::create_dir(&stage).expect("stage");
    let mut snapshot = capture_source(&project, &plan, &stage, true).expect("snapshot");
    prepared
        .validate_snapshot(snapshot.source())
        .expect("complete prepared snapshot");
    assert_eq!(
        fs::read(snapshot.source().join(".git")).expect("opaque payload"),
        b"opaque native payload\r\n"
    );
    snapshot.bind_prepared(&prepared).expect("prepared map");
    let artifact = stage.join("artifact.cf");
    fs::write(&artifact, b"prepared artifact").expect("artifact");
    let output = stage.join("manifest.json");
    write(
        &output,
        &artifact,
        ArtifactType::Configuration,
        "8.3.27.2325",
        BuildProvenance::new(None, GitProvenance::Unavailable),
        &snapshot,
        None,
    )
    .expect("manifest");
    let document: serde_json::Value =
        serde_json::from_slice(&fs::read(output).expect("manifest bytes")).expect("manifest JSON");
    assert_eq!(document["schema_version"], 2);
    assert_eq!(
        document["source"]["protected"],
        serde_json::json!({
            "source_schema_version": SCHEMA_VERSION,
            "rules_version": RULES_VERSION,
            "profile": PROFILE,
            "generation": prepared.generation(),
            "input_revision": prepared.input_revision(),
            "baseline": prepared.baseline(),
            "native_revision": prepared.native_revision(),
            "source_map_checksum": {"algorithm": "sha256", "value": prepared.source_map().checksum().expect("map checksum")},
        })
    );
    let map: crate::project_core::source_format::SourceMap =
        serde_json::from_value(document["source"]["source_map"].clone())
            .expect("stored source map");
    assert_eq!(map, *prepared.source_map());
    assert_eq!(
        map.checksum().expect("stored map checksum"),
        document["source"]["protected"]["source_map_checksum"]["value"]
    );
    let other_stage = fixture.0.join("ordinary-stage");
    fs::create_dir(&other_stage).expect("ordinary stage");
    let ordinary = capture_source(&project, &plan, &other_stage, false).expect("ordinary snapshot");
    assert_eq!(
        prepared
            .validate_snapshot(ordinary.source())
            .expect_err("discarded opaque payload")
            .code,
        "source_snapshot_changed"
    );
}

#[test]
/// Compare copied bytes with the private proof when native output changes after preparation.
fn copied_changed_generation_is_rejected_before_platform_execution() {
    let fixture = TestDir::new();
    let (project, plan, prepared) = prepared_project(&fixture.0);
    fs::write(
        prepared.xml().join("Configuration.xml"),
        b"changed after preparation",
    )
    .expect("changed generation");
    let stage = fixture.0.join("stage");
    fs::create_dir(&stage).expect("stage");
    let snapshot = capture_source(&project, &plan, &stage, true).expect("copy changed bytes");
    assert_eq!(
        prepared
            .validate_snapshot(snapshot.source())
            .expect_err("changed snapshot")
            .code,
        "source_snapshot_changed"
    );
}

#[test]
/// Record exact snapshot bytes and version without inspecting an unusable repository.
fn manifest_describes_snapshot_after_the_working_source_changes() {
    let fixture = TestDir::new();
    let source = fixture.0.join("src");
    fs::create_dir(&source).expect("source directory");
    fs::write(fixture.0.join(".git"), b"gitdir: inaccessible\n").expect("unusable Git marker");
    let original = b"\xef\xbb\xbf<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration><Properties><Version>01.2.3.04</Version></Properties></Configuration></MetaDataObject>\r\n";
    fs::write(source.join("Configuration.xml"), original).expect("root descriptor");
    let resource = b"\xef\xbb\xbfProcedure Sample()\r\nEndProcedure\r\n";
    fs::write(source.join("Module.bsl"), resource).expect("module bytes");
    let configuration =
        ProjectConfiguration::new(ProjectType::Configuration, SourceFormat::DesignerXml);
    let project = Project::new(fixture.0.clone(), source.clone(), configuration).expect("project");
    let plan = BuildPlan::with_platform_version(
        &project,
        None,
        Some(super::super::PlatformVersion::parse("8.3.27.2325").expect("platform version")),
    )
    .expect("plan");
    let workspace = fixture.0.join("stage");
    fs::create_dir(&workspace).expect("workspace");
    let snapshot = capture_source(&project, &plan, &workspace, false).expect("snapshot");
    assert_eq!(
        fs::read(snapshot.source().join("Configuration.xml")).expect("snapshot descriptor"),
        original,
    );
    assert_eq!(
        fs::read(snapshot.source().join("Module.bsl")).expect("snapshot module"),
        resource,
    );
    fs::write(source.join("Configuration.xml"), b"broken working XML")
        .expect("changed working XML");
    fs::write(source.join("Module.bsl"), b"changed module").expect("changed working module");
    let artifact = workspace.join("artifact.cf");
    fs::write(&artifact, b"captured artifact").expect("artifact");
    let output = workspace.join("artifact.manifest.json");
    write(
        &output,
        &artifact,
        ArtifactType::Configuration,
        "8.3.27.2325",
        BuildProvenance::new(Some("sample"), GitProvenance::Unavailable),
        &snapshot,
        None,
    )
    .expect("manifest");
    let document: serde_json::Value =
        serde_json::from_slice(&fs::read(output).expect("manifest bytes")).expect("manifest JSON");
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["project"]["name"], "sample");
    assert_eq!(document["project"]["version"]["value"], "01.2.3.04");
    assert_eq!(document["source"]["snapshot_id"], snapshot.id);
    assert_eq!(document["source"]["git"]["status"], "unavailable");
    assert_eq!(
        document["artifact"]["checksum"]["value"],
        hex(&Sha256::digest(b"captured artifact")),
    );
    assert_eq!(
        fs::read(snapshot.source().join("Module.bsl")).expect("unchanged snapshot"),
        resource,
    );
}
