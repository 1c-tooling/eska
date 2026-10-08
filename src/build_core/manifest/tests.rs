use super::*;
use crate::{
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
    let snapshot = capture_source(&project, &plan, &workspace).expect("snapshot");
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
