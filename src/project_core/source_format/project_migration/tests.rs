use super::super::files::Inventory;
use super::*;
use crate::{
    project_core::source_format::{
        NativeUuid, Operation, PermissionReason, PropertyName, PropertyValue, export_designer,
        prepare_property_edit,
    },
    project_core::{ProjectName, creation, discovery::discover, eska_source::EskaSource},
    test_support::TestDir,
};

/// This fixture copies public input only into the test's own playground directory.
fn fixture(test: &TestDir, frozen: &[NativeUuid]) -> creation::ImportedProject {
    let native = test.0.join("native");
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    let mut pending = vec![PathBuf::new()];
    while let Some(path) = pending.pop() {
        fs::create_dir_all(native.join(&path)).unwrap();
        for entry in fs::read_dir(input.join(&path)).unwrap() {
            let entry = entry.unwrap();
            let path = path.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
            } else {
                fs::copy(entry.path(), native.join(path)).unwrap();
            }
        }
    }
    fs::remove_file(native.join("Ext/ParentConfigurations.bin")).unwrap();
    creation::import_project(
        &native,
        &test.0.join("old"),
        ProjectName::parse("demo".into()).unwrap(),
        None,
        frozen,
    )
    .unwrap()
}

/// Working values, old authority, support, annotations and byte-safe project settings stay distinct.
#[test]
fn project_copy_preserves_working_edits_and_separately_migrates_authority() {
    let test = TestDir::new();
    let frozen = NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap();
    let imported = fixture(&test, std::slice::from_ref(&frozen));
    let project = &imported.project;
    let old_base =
        AcceptedBase::open(&project.root().join(".eska/accepted"), &imported.accepted).unwrap();
    let uuid = NativeUuid::parse("10000000-0000-4000-8000-000000000001").unwrap();
    prepare_property_edit(
        project.source(),
        &old_base,
        &uuid,
        PropertyName::Comment,
        PropertyValue::Text("Working edit".into()),
    )
    .unwrap()
    .publish(&old_base)
    .unwrap();
    let config = project.root().join("eska.toml");
    fs::write(
        &config,
        format!(
            "# Preserve user settings\r\n{}",
            fs::read_to_string(&config).unwrap()
        ),
    )
    .unwrap();
    fs::write(
        project.root().join("notes.md"),
        "User-owned extra project file",
    )
    .unwrap();
    let before = Inventory::read(project.root()).unwrap();
    let plan = prepare_readable_project(
        project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &test.0.join("new"),
    )
    .unwrap();
    assert!(!plan.destination().exists());
    assert!(plan.layout_changed());
    assert_ne!(plan.accepted_reference(), &imported.accepted);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
    assert_eq!(Inventory::read(project.root()).unwrap(), before);
    let copied = discover(plan.destination()).unwrap();
    assert_eq!(copied.configuration(), project.configuration());
    assert_eq!(
        fs::read(copied.root().join("eska.toml")).unwrap(),
        fs::read(config).unwrap()
    );
    assert!(!copied.root().join("notes.md").exists());
    assert!(!copied.root().join(".git").exists());
    let source = EskaSource::open(copied.clone(), ProjectScope::Standalone).unwrap();
    assert_eq!(source.reader().manifest().schema_version, 3);
    let base = AcceptedBase::open(
        &copied.root().join(".eska/accepted"),
        plan.accepted_reference(),
    )
    .unwrap();
    assert_eq!(
        base.permission(&frozen, Operation::Property(PropertyName::Comment))
            .reason,
        PermissionReason::EskaFrozen
    );
    let restored = test.0.join("restored");
    base.restore(&restored).unwrap();
    assert!(
        !fs::read_to_string(restored.join("configuration.yaml"))
            .unwrap()
            .contains("Working edit")
    );
    assert!(
        fs::read_to_string(copied.source().join("configuration.yaml"))
            .unwrap()
            .contains("Working edit")
    );
    let old_xml = test.0.join("old-xml");
    let new_xml = test.0.join("new-xml");
    export_designer(project.source(), &old_xml).unwrap();
    export_designer(copied.source(), &new_xml).unwrap();
    assert_eq!(
        Inventory::read(&old_xml).unwrap(),
        Inventory::read(&new_xml).unwrap()
    );
    let protected = source
        .prepare(
            plan.accepted_reference(),
            &copied.root().join(".eska/generations"),
            true,
        )
        .unwrap();
    assert!(protected.diagnostics().is_empty());
    assert_eq!(
        Inventory::read(protected.xml()).unwrap(),
        Inventory::read(&old_xml).unwrap()
    );
}

/// Readable inputs retain exact authored bytes and authority rather than creating a fresh baseline.
#[test]
fn readable_project_copy_is_exact_noop_and_retains_independent_pin() {
    let test = TestDir::new();
    let imported = fixture(&test, &[]);
    let plan = prepare_readable_project(
        &imported.project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &test.0.join("new"),
    )
    .unwrap();
    plan.publish().unwrap();
    let copied = discover(plan.destination()).unwrap();
    let next = prepare_readable_project(
        &copied,
        &ProjectScope::Standalone,
        plan.accepted_reference(),
        &test.0.join("again"),
    )
    .unwrap();
    assert!(!next.layout_changed());
    assert_eq!(next.accepted_reference(), plan.accepted_reference());
    next.publish().unwrap();
    assert_eq!(
        Inventory::read(&copied.root().join("src")).unwrap(),
        Inventory::read(&next.destination().join("src")).unwrap()
    );
}

/// A process stop between source, base and project markers resumes from immutable original inputs.
#[test]
fn interrupted_project_publication_resumes_and_blocks_project_consumers() {
    let test = TestDir::new();
    let imported = fixture(&test, &[]);
    let plan = prepare_readable_project(
        &imported.project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &test.0.join("new"),
    )
    .unwrap();
    fs::create_dir(plan.destination()).unwrap();
    publish_new(
        plan.destination(),
        PROJECT_LAYOUT_RECEIPT,
        &serde_json::to_string(&plan.receipt).unwrap(),
    )
    .unwrap();
    fs::create_dir(plan.destination().join(".eska")).unwrap();
    plan.source.publish(&plan.source_destination()).unwrap();
    publication::interrupt(plan.base.publication_view(), &plan.base_destination());
    assert!(!plan.destination().join("eska.toml").exists());
    let resumed = prepare_readable_project(
        &imported.project,
        &ProjectScope::Standalone,
        &imported.accepted,
        plan.destination(),
    )
    .unwrap();
    assert_eq!(resumed.publish().unwrap(), LayoutPublication::Resumed);
    assert!(!plan.destination().join(PROJECT_LAYOUT_RECEIPT).exists());
    let copied = discover(plan.destination()).unwrap();
    EskaSource::open(copied.clone(), ProjectScope::Standalone).unwrap();
    publish_new(
        plan.destination(),
        PROJECT_LAYOUT_RECEIPT,
        &serde_json::to_string(&plan.receipt).unwrap(),
    )
    .unwrap();
    assert_eq!(
        EskaSource::open(copied, ProjectScope::Standalone)
            .err()
            .unwrap()
            .code,
        "source_migration_incomplete"
    );
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Resumed);
}

/// A foreign nested base blocks publication before the planned working source is created.
#[test]
fn foreign_or_edited_outputs_never_get_overwritten_or_partially_accepted() {
    let test = TestDir::new();
    let imported = fixture(&test, &[]);
    let plan = prepare_readable_project(
        &imported.project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &test.0.join("new"),
    )
    .unwrap();
    fs::create_dir(plan.destination()).unwrap();
    publish_new(
        plan.destination(),
        PROJECT_LAYOUT_RECEIPT,
        &serde_json::to_string(&plan.receipt).unwrap(),
    )
    .unwrap();
    fs::create_dir_all(plan.base_destination()).unwrap();
    fs::write(plan.base_destination().join("user"), "Do not replace").unwrap();
    assert!(plan.publish().is_err());
    assert!(!plan.source_destination().exists());
    assert_eq!(
        fs::read_to_string(plan.base_destination().join("user")).unwrap(),
        "Do not replace"
    );
    fs::remove_file(plan.base_destination().join("user")).unwrap();
    fs::remove_dir(plan.base_destination()).unwrap();
    plan.publish().unwrap();
    let yaml = plan.source_destination().join("configuration.yaml");
    fs::write(&yaml, "edited output").unwrap();
    assert!(plan.publish().is_err());
    assert_eq!(fs::read_to_string(yaml).unwrap(), "edited output");
}

/// Every selected input, including optional-file absence, remains part of publication authority.
#[test]
fn stale_sources_authority_and_settings_refuse_before_destination_creation() {
    for changed in [
        "src/configuration.yaml",
        ".eska/accepted/accepted.json",
        "eska.toml",
        ".gitignore",
    ] {
        let test = TestDir::new();
        let imported = fixture(&test, &[]);
        let plan = prepare_readable_project(
            &imported.project,
            &ProjectScope::Standalone,
            &imported.accepted,
            &test.0.join("new"),
        )
        .unwrap();
        let path = imported.project.root().join(changed);
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend_from_slice(b"\n");
        fs::write(path, bytes).unwrap();
        assert_eq!(plan.publish().unwrap_err().code, "source_input_changed");
        assert!(!plan.destination().exists());
    }
    let test = TestDir::new();
    let imported = fixture(&test, &[]);
    fs::remove_file(imported.project.root().join(".gitignore")).unwrap();
    let plan = prepare_readable_project(
        &imported.project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &test.0.join("new"),
    )
    .unwrap();
    fs::write(imported.project.root().join(".gitignore"), "new content").unwrap();
    assert_eq!(plan.publish().unwrap_err().code, "source_input_changed");
}

/// Explicit scope and trust failures are read-only, including overlapping or unowned roots.
#[test]
fn invalid_scope_pin_and_destination_refuse_without_modifying_inputs() {
    assert_eq!(
        validate_paths(
            &SourcePath::parse("src/data").unwrap(),
            &SourcePath::parse("SRC/base").unwrap()
        )
        .unwrap_err()
        .code,
        "source_path_collision"
    );
    let test = TestDir::new();
    let imported = fixture(&test, &[]);
    let before = Inventory::read(imported.project.root()).unwrap();
    assert_eq!(
        prepare_readable_project(
            &imported.project,
            &ProjectScope::Member(ProjectName::parse("member".into()).unwrap()),
            &imported.accepted,
            &test.0.join("new")
        )
        .unwrap_err()
        .code,
        "source_migration_scope_unsupported"
    );
    let bad_pin = AcceptedRef {
        sha256: "0".repeat(64),
    };
    assert_eq!(
        prepare_readable_project(
            &imported.project,
            &ProjectScope::Standalone,
            &bad_pin,
            &test.0.join("new")
        )
        .unwrap_err()
        .code,
        "source_baseline_untrusted"
    );
    assert_eq!(
        prepare_readable_project(
            &imported.project,
            &ProjectScope::Standalone,
            &imported.accepted,
            &imported.project.root().join("nested")
        )
        .unwrap_err()
        .code,
        "source_tree_overlap"
    );
    fs::create_dir(test.0.join("foreign")).unwrap();
    fs::write(test.0.join("foreign/user"), "user").unwrap();
    assert!(
        prepare_readable_project(
            &imported.project,
            &ProjectScope::Standalone,
            &imported.accepted,
            &test.0.join("foreign")
        )
        .is_err()
    );
    assert_eq!(Inventory::read(imported.project.root()).unwrap(), before);
    assert!(!test.0.join("new").exists());
}

#[cfg(unix)]
/// Filesystem aliases cannot turn ancillary or output paths into authority.
#[test]
fn symlink_entries_and_roots_are_refused() {
    let test = TestDir::new();
    let imported = fixture(&test, &[]);
    std::os::unix::fs::symlink(imported.project.root(), test.0.join("alias")).unwrap();
    assert!(
        prepare_readable_project(
            &imported.project,
            &ProjectScope::Standalone,
            &imported.accepted,
            &test.0.join("alias")
        )
        .is_err()
    );
    fs::remove_file(imported.project.root().join(".gitignore")).unwrap();
    std::os::unix::fs::symlink("eska.toml", imported.project.root().join(".gitignore")).unwrap();
    assert_eq!(
        prepare_readable_project(
            &imported.project,
            &ProjectScope::Standalone,
            &imported.accepted,
            &test.0.join("new")
        )
        .unwrap_err()
        .code,
        "source_entry_unsupported"
    );
}
