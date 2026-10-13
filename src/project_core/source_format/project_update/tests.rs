use super::*;
use crate::{
    project_core::{
        ProjectName, creation,
        discovery::discover,
        eska_source::EskaSource,
        source_format::{LayoutPublication, NativeUuid, files::Inventory},
    },
    test_support::TestDir,
};
use std::fs;

/// Two small native inputs stay independent and every temporary byte belongs to this test.
fn fixture(test: &TestDir, name: &str, frozen: &[NativeUuid]) -> creation::ImportedProject {
    let native = test.0.join(format!("{name}-native"));
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        fs::create_dir_all(native.join(&relative)).unwrap();
        for entry in fs::read_dir(fixture.join(&relative)).unwrap() {
            let entry = entry.unwrap();
            let path = relative.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
            } else {
                fs::copy(entry.path(), native.join(path)).unwrap();
            }
        }
    }
    fs::remove_file(native.join("Ext/ParentConfigurations.bin")).unwrap();
    if name != "old" {
        let path = native.join("Configuration.xml");
        fs::write(
            &path,
            fs::read_to_string(&path)
                .unwrap()
                .replace("Configuration &amp; resources", "Incoming supplier comment"),
        )
        .unwrap();
    }
    creation::import_readable_project(
        &native,
        &test.0.join(name),
        ProjectName::parse(name.into()).unwrap(),
        None,
        frozen,
    )
    .unwrap()
}

/// A full copy preserves local modules and exact settings without accepting incoming working edits.
#[test]
fn independent_bases_and_settings_remain_usable_after_publication() {
    let test = TestDir::new();
    let old = fixture(&test, "old", &[]);
    let new = fixture(&test, "new", &[]);
    let local = b"\xef\xbb\xbf// Local module remains editable\r\n";
    fs::write(
        old.project.source().join("CommonModules/Tools/Module.bsl"),
        local,
    )
    .unwrap();
    fs::write(
        new.project.source().join("CommonModules/Tools/Module.bsl"),
        "// Unapproved incoming working change\n",
    )
    .unwrap();
    let settings = old.project.root().join("eska.toml");
    fs::write(
        &settings,
        format!(
            "# Exact settings\r\n{}",
            fs::read_to_string(&settings).unwrap()
        ),
    )
    .unwrap();
    fs::write(
        old.project.root().join("notes.md"),
        "Keep user notes in the original",
    )
    .unwrap();
    let before = Inventory::read(old.project.root()).unwrap();
    let plan = prepare_project_base_update(
        &old.project,
        &ProjectScope::Standalone,
        &old.accepted,
        &new.project,
        &new.accepted,
        &test.0.join("updated"),
    )
    .unwrap();
    assert!(plan.source_plan().conflicts().is_empty());
    assert!(!plan.destination().exists());
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
    assert_eq!(Inventory::read(old.project.root()).unwrap(), before);
    assert!(!plan.destination().join("notes.md").exists());
    let updated = discover(plan.destination()).unwrap();
    assert_eq!(
        fs::read(updated.root().join("eska.toml")).unwrap(),
        fs::read(settings).unwrap()
    );
    assert_eq!(
        fs::read(updated.source().join("CommonModules/Tools/Module.bsl")).unwrap(),
        local
    );
    let source = EskaSource::open(updated, ProjectScope::Standalone).unwrap();
    let generation = source
        .prepare(&new.accepted, &test.0.join("generations"), true)
        .unwrap();
    assert!(generation.diagnostics().is_empty());
    assert!(AcceptedBase::open(&old.project.root().join(".eska/accepted"), &old.accepted).is_ok());
    assert!(AcceptedBase::open(&plan.destination().join(".eska/accepted"), &old.accepted).is_err());
}

/// Recovery remains unavailable to readers even when an outer manifest has already appeared.
#[test]
fn exact_interrupted_publication_resumes_but_changed_source_and_extra_data_refuse() {
    for foreign in [false, true] {
        let test = TestDir::new();
        let old = fixture(&test, "old", &[]);
        let new = fixture(&test, "new", &[]);
        let plan = prepare_project_base_update(
            &old.project,
            &ProjectScope::Standalone,
            &old.accepted,
            &new.project,
            &new.accepted,
            &test.0.join("updated"),
        )
        .unwrap();
        fs::create_dir(plan.destination()).unwrap();
        fs::write(
            plan.destination().join(PROJECT_BASE_UPDATE_RECEIPT),
            serde_json::to_vec(&plan.receipt).unwrap(),
        )
        .unwrap();
        plan.source
            .publish_source(&plan.destination().join("src"))
            .unwrap();
        fs::remove_file(
            plan.destination()
                .join("src/CommonModules/Tools/Module.bsl"),
        )
        .unwrap();
        fs::write(
            plan.destination().join("src/.eska-base-update-write"),
            "Interrupted private staging",
        )
        .unwrap();
        fs::write(
            plan.destination().join("eska.toml"),
            plan.ancillary[&SourcePath::parse("eska.toml").unwrap()]
                .as_ref()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            super::super::require_project_ready(plan.destination())
                .unwrap_err()
                .code,
            "source_update_incomplete"
        );
        if foreign {
            fs::write(
                plan.destination()
                    .join("src/CommonModules/Tools/Module.bsl"),
                "Foreign changed output",
            )
            .unwrap();
            let before = Inventory::read(plan.destination()).unwrap();
            assert_eq!(
                plan.publish().unwrap_err().code,
                "source_update_output_mismatch"
            );
            assert_eq!(Inventory::read(plan.destination()).unwrap(), before);
            assert!(!plan.destination().join(".eska/accepted").exists());
        } else {
            assert_eq!(plan.publish().unwrap(), LayoutPublication::Resumed);
            assert!(
                !plan
                    .destination()
                    .join(PROJECT_BASE_UPDATE_RECEIPT)
                    .exists()
            );
            assert!(
                !plan
                    .destination()
                    .join("src/.eska-base-update-write")
                    .exists()
            );
            let source = EskaSource::open(
                discover(plan.destination()).unwrap(),
                ProjectScope::Standalone,
            )
            .unwrap();
            source
                .prepare(&new.accepted, &test.0.join("generations"), true)
                .unwrap();
        }
    }
}

/// Conflict preview, wrong pins and stale manifests cannot publish an apparently usable project.
#[test]
fn conflicts_approval_and_stale_settings_refuse_before_writing() {
    let test = TestDir::new();
    let old = fixture(&test, "old", &[]);
    let module = NativeUuid::parse("60000000-0000-4000-8000-000000000001").unwrap();
    let new = fixture(&test, "new", &[module]);
    let local = b"// Retain denied local data\r\n";
    fs::write(
        old.project.source().join("CommonModules/Tools/Module.bsl"),
        local,
    )
    .unwrap();
    let destination = test.0.join("updated");
    let plan = prepare_project_base_update(
        &old.project,
        &ProjectScope::Standalone,
        &old.accepted,
        &new.project,
        &new.accepted,
        &destination,
    )
    .unwrap();
    assert!(!plan.source_plan().conflicts().is_empty());
    assert_eq!(plan.publish().unwrap_err().code, "source_update_conflicts");
    assert!(!destination.exists());
    let wrong = AcceptedRef {
        sha256: "0".repeat(64),
    };
    assert_eq!(
        prepare_project_base_update(
            &old.project,
            &ProjectScope::Standalone,
            &old.accepted,
            &new.project,
            &wrong,
            &destination
        )
        .err()
        .unwrap()
        .code,
        "source_baseline_untrusted"
    );
    fs::write(
        new.project.root().join("eska.toml"),
        "Changed incoming settings",
    )
    .unwrap();
    assert_eq!(plan.publish().unwrap_err().code, "source_input_changed");
    assert!(!destination.exists());
    assert_eq!(
        fs::read(old.project.source().join("CommonModules/Tools/Module.bsl")).unwrap(),
        local
    );
}

/// A supplied Project cannot select an old store after the incoming manifest declares another one.
#[test]
fn stale_incoming_authority_selection_is_refused_before_preview() {
    let test = TestDir::new();
    let old = fixture(&test, "old", &[]);
    let new = fixture(&test, "new", &[]);
    let settings = new.project.root().join("eska.toml");
    fs::write(
        &settings,
        fs::read_to_string(&settings)
            .unwrap()
            .replace(".eska/accepted", ".eska/different-accepted"),
    )
    .unwrap();
    let destination = test.0.join("updated");
    assert_eq!(
        prepare_project_base_update(
            &old.project,
            &ProjectScope::Standalone,
            &old.accepted,
            &new.project,
            &new.accepted,
            &destination,
        )
        .err()
        .unwrap()
        .code,
        "source_input_changed"
    );
    assert!(!destination.exists());
}
