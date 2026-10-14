//! Recovery tests remove original incoming staging and preserve every independently approved byte.

use super::*;
use crate::{
    project_core::{
        ProjectName,
        config::ProjectConfig,
        discovery::discover,
        eska_source::EskaSource,
        source_format::{
            NativeUuid, accept_source, capture_import_ancestor, synchronization::tests::source,
        },
    },
    test_support::TestDir,
};

const MODULE: &str = "CommonModules/Tools/Module.bsl";
const LOCAL: &[u8] = b"\xef\xbb\xbf// local survives\r\n";

/// All sources, native inputs, ancestors, support and artifacts belong to this unique test root.
pub(super) struct Fixture {
    pub(super) test: TestDir,
    pub(super) project: Project,
    pub(super) approved: AcceptedRef,
    pub(super) ancestor: ImportAncestor,
    pub(super) incoming: ImportAncestor,
}

impl Fixture {
    /// Incoming version and local module edits are independent and must both survive publication.
    pub(super) fn new(frozen: bool) -> Self {
        let test = TestDir::new();
        let source = source(&test);
        let root = test.0.join("project");
        fs::create_dir(&root).unwrap();
        fs::rename(source, root.join("src")).unwrap();
        fs::create_dir(root.join(".eska")).unwrap();
        let freeze = if frozen {
            vec![NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap()]
        } else {
            Vec::new()
        };
        let approved =
            accept_source(&root.join("src"), &root.join(".eska/accepted"), &freeze).unwrap();
        fs::write(root.join("eska.toml"), "# exact settings\r\n[project]\r\nname = 'sample'\r\ntype = 'configuration'\r\nsource_format = 'eska'\r\n[eska]\r\naccepted_base = '.eska/accepted'\r\n").unwrap();
        fs::write(root.join("notes.md"), "user notes").unwrap();
        fs::create_dir(root.join("build")).unwrap();
        fs::write(root.join("build/previous.cf"), [0, 255, 10]).unwrap();
        let project = discover(&root).unwrap();
        let store = test.0.join("ancestor");
        let pin = capture_import_ancestor(project.source(), &store).unwrap();
        let ancestor = ImportAncestor::open(&store, &pin).unwrap();
        let incoming_source = test.0.join("incoming-source");
        ancestor.restore(&incoming_source).unwrap();
        let path = incoming_source.join("configuration.yaml");
        fs::write(
            &path,
            fs::read_to_string(&path).unwrap().replace("1.0.0", "1.0.1"),
        )
        .unwrap();
        let store = test.0.join("incoming");
        let pin = capture_import_ancestor(&incoming_source, &store).unwrap();
        let incoming = ImportAncestor::open(&store, &pin).unwrap();
        fs::write(project.source().join(MODULE), LOCAL).unwrap();
        Self {
            test,
            project,
            approved,
            ancestor,
            incoming,
        }
    }

    /// Preview consumes independently selected support and ancestry references without any writes.
    fn plan(&self) -> ProjectSourceImportPlan {
        prepare_project_source_import(
            &self.project,
            &ProjectScope::Standalone,
            &self.approved,
            &self.ancestor,
            &self.incoming,
        )
        .unwrap()
    }
}

/// A merged owner is fully journaled and can recover after both original ancestry stores disappear.
#[test]
fn recovers_merged_descriptor_and_repeats_from_raw_incoming_ancestry() {
    let fixture = Fixture::new(false);
    let root = fixture.project.root();
    let owner = fixture.project.source().join("configuration.yaml");
    let text = fs::read_to_string(&owner).unwrap();
    fs::write(
        &owner,
        format!("# Keep local descriptor annotation\n{text}"),
    )
    .unwrap();
    let plan = fixture.plan();
    assert!(plan.source_plan().conflicts().is_empty());
    assert!(
        plan.source
            .selected
            .values()
            .any(|selected| matches!(selected, super::super::SelectedFile::Merged { .. }))
    );
    let reference = plan.reference().clone();
    let incoming = fixture.incoming.reference().clone();
    assert_eq!(
        plan.publish_cancellable(&|| root.join(PROJECT_SOURCE_IMPORT_RECEIPT).exists())
            .unwrap_err()
            .code,
        "source_read_cancelled"
    );
    drop(plan);
    fs::remove_dir_all(&fixture.ancestor.root).unwrap();
    fs::remove_dir_all(&fixture.incoming.root).unwrap();
    recover_project_source_import(root, &reference, SourceImportRecovery::Complete, &|| false)
        .unwrap();
    let merged = fs::read_to_string(&owner).unwrap();
    assert!(merged.starts_with("# Keep local descriptor annotation\n"));
    assert!(merged.contains("1.0.1"));
    let saved =
        project_import_ancestor(&fixture.project, &ProjectScope::Standalone, &incoming).unwrap();
    let restored = fixture.test.0.join("raw-incoming");
    saved.restore(&restored).unwrap();
    assert!(
        !fs::read_to_string(restored.join("configuration.yaml"))
            .unwrap()
            .contains("Keep local")
    );
    let repeated = prepare_project_source_import(
        &fixture.project,
        &ProjectScope::Standalone,
        &fixture.approved,
        &saved,
        &saved,
    )
    .unwrap();
    assert!(repeated.source_plan().conflicts().is_empty());
    assert_eq!(repeated.publish().unwrap(), LayoutPublication::Unchanged);
    EskaSource::open(fixture.project.clone(), ProjectScope::Standalone)
        .unwrap()
        .prepare(&fixture.approved, &root.join(".eska/generations"), true)
        .unwrap();
}

/// Incoming metadata already authored locally still advances the common ancestor for future merges.
#[test]
fn unchanged_source_bytes_do_not_hide_new_common_metadata() {
    let fixture = Fixture::new(false);
    fixture.plan().publish().unwrap();
    let saved = project_import_ancestor(
        &fixture.project,
        &ProjectScope::Standalone,
        fixture.incoming.reference(),
    )
    .unwrap();
    let incoming_source = fixture.test.0.join("next-metadata");
    saved.restore(&incoming_source).unwrap();
    for source in [&incoming_source, fixture.project.source()] {
        let path = source.join("configuration.yaml");
        let text = fs::read_to_string(&path).unwrap().replace("1.0.1", "1.0.2");
        fs::write(path, text).unwrap();
    }
    let store = fixture.test.0.join("next-metadata-store");
    let reference = capture_import_ancestor(&incoming_source, &store).unwrap();
    let incoming = ImportAncestor::open(&store, &reference).unwrap();
    let before = Inventory::read(fixture.project.source()).unwrap();
    let plan = prepare_project_source_import(
        &fixture.project,
        &ProjectScope::Standalone,
        &fixture.approved,
        &saved,
        &incoming,
    )
    .unwrap();
    assert!(plan.source_plan().conflicts().is_empty());
    assert_eq!(
        plan.source_plan().working_revision(),
        plan.source_plan().output_revision()
    );
    assert!(!plan.source.incoming_data_unchanged);
    assert_eq!(plan.resulting_ancestor_reference(), incoming.reference());
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    project_import_ancestor(
        &fixture.project,
        &ProjectScope::Standalone,
        incoming.reference(),
    )
    .unwrap();
    assert_eq!(Inventory::read(fixture.project.source()).unwrap(), before);
}

/// Ordinary application advances raw ancestry, retains local edits, settings, support and last build.
#[test]
fn publishes_existing_project_and_repeated_import_is_a_noop() {
    let fixture = Fixture::new(false);
    let root = fixture.project.root();
    let support = Inventory::read(&root.join(".eska/accepted")).unwrap();
    let config = fs::read(root.join("eska.toml")).unwrap();
    let plan = fixture.plan();
    assert!(plan.source_plan().conflicts().is_empty());
    assert!(!root.join(SERVICE).exists());
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
    assert_eq!(
        fs::read(fixture.project.source().join(MODULE)).unwrap(),
        LOCAL
    );
    assert!(
        fs::read_to_string(fixture.project.source().join("configuration.yaml"))
            .unwrap()
            .contains("1.0.1")
    );
    assert_eq!(fs::read(root.join("eska.toml")).unwrap(), config);
    assert_eq!(
        fs::read(root.join("build/previous.cf")).unwrap(),
        [0, 255, 10]
    );
    assert_eq!(
        fs::read_to_string(root.join("notes.md")).unwrap(),
        "user notes"
    );
    assert_eq!(
        Inventory::read(&root.join(".eska/accepted")).unwrap(),
        support
    );
    assert_eq!(
        Inventory::read(&transaction(root, plan.reference()).join("previous-source"))
            .unwrap()
            .revision,
        plan.source.working_revision()
    );
    let saved = project_import_ancestor(
        &fixture.project,
        &ProjectScope::Standalone,
        fixture.incoming.reference(),
    )
    .unwrap();
    let restored = fixture.test.0.join("saved-incoming");
    saved.restore(&restored).unwrap();
    assert_eq!(
        fs::read(restored.join(MODULE)).unwrap(),
        b"\xef\xbb\xbf// original\r\n"
    );
    let repeated = prepare_project_source_import(
        &fixture.project,
        &ProjectScope::Standalone,
        &fixture.approved,
        &saved,
        &fixture.incoming,
    )
    .unwrap();
    let before = Inventory::read(root).unwrap();
    assert_eq!(repeated.publish().unwrap(), LayoutPublication::Unchanged);
    assert_eq!(Inventory::read(root).unwrap(), before);
    EskaSource::open(fixture.project.clone(), ProjectScope::Standalone)
        .unwrap()
        .prepare(&fixture.approved, &fixture.test.0.join("generations"), true)
        .unwrap();
}

/// Staging plus an exact guard models each process-stop boundary without fault-injection hooks.
fn interrupt(plan: &ProjectSourceImportPlan, point: &str) {
    plan.stage(&|| false).unwrap();
    fs::write(
        plan.root.join(PROJECT_SOURCE_IMPORT_RECEIPT),
        serde_json::to_vec(plan.reference()).unwrap(),
    )
    .unwrap();
    let directory = transaction(&plan.root, plan.reference());
    if point != "prepared" {
        fs::rename(
            plan.root.join(plan.receipt.source.as_path()),
            directory.join("previous-source"),
        )
        .unwrap();
    }
    if matches!(point, "installed" | "state") {
        fs::rename(
            directory.join("candidate"),
            plan.root.join(plan.receipt.source.as_path()),
        )
        .unwrap();
    }
    if point == "state" {
        fs::write(
            plan.root.join(STATE),
            serde_json::to_vec(&State {
                schema_version: 1,
                source: plan.receipt.source.clone(),
                transaction: plan.reference.clone(),
                ancestor: plan.receipt.incoming.clone(),
            })
            .unwrap(),
        )
        .unwrap();
    }
}

/// Every guarded boundary can complete or roll back after the plan and input staging disappear.
#[test]
fn recovers_each_directory_and_state_boundary_without_original_inputs() {
    for point in ["prepared", "backed-up", "installed", "state"] {
        for action in [
            SourceImportRecovery::Complete,
            SourceImportRecovery::RollBack,
        ] {
            let fixture = Fixture::new(false);
            let source =
                EskaSource::open(fixture.project.clone(), ProjectScope::Standalone).unwrap();
            let plan = fixture.plan();
            let old = plan.source.working.inventory.revision.clone();
            let new = plan.source.output.revision.clone();
            let reference = plan.reference().clone();
            interrupt(&plan, point);
            assert_eq!(
                require_project_ready(fixture.project.root())
                    .unwrap_err()
                    .code,
                "source_import_incomplete"
            );
            assert_eq!(
                source
                    .prepare(
                        &fixture.approved,
                        &fixture.test.0.join("blocked-generation"),
                        true
                    )
                    .unwrap_err()
                    .error
                    .code,
                "source_import_incomplete"
            );
            assert!(!fixture.test.0.join("blocked-generation").exists());
            drop(plan);
            fs::remove_dir_all(&fixture.ancestor.root).unwrap();
            fs::remove_dir_all(&fixture.incoming.root).unwrap();
            assert_eq!(
                recover_project_source_import(fixture.project.root(), &reference, action, &|| {
                    false
                })
                .unwrap(),
                LayoutPublication::Resumed
            );
            assert_eq!(
                recover_project_source_import(fixture.project.root(), &reference, action, &|| {
                    false
                })
                .unwrap(),
                LayoutPublication::Unchanged
            );
            assert_eq!(
                Inventory::read(fixture.project.source()).unwrap().revision,
                if action == SourceImportRecovery::Complete {
                    new
                } else {
                    old
                }
            );
            assert_eq!(
                fs::read(fixture.project.source().join(MODULE)).unwrap(),
                LOCAL
            );
            require_project_ready(fixture.project.root()).unwrap();
            let baseline = AcceptedBase::open(
                &fixture.project.root().join(".eska/accepted"),
                &fixture.approved,
            )
            .unwrap();
            baseline.verify().unwrap();
            EskaSource::open(fixture.project.clone(), ProjectScope::Standalone)
                .unwrap()
                .prepare(&fixture.approved, &fixture.test.0.join("generation"), true)
                .unwrap();
        }
    }
}

/// Altered journal, source, ancestry, approval or settings block both actions before a first write.
#[test]
fn refuses_foreign_data_and_independently_wrong_recovery_references() {
    for mode in [
        "working",
        "candidate",
        "backup",
        "incoming",
        "ancestor",
        "approved",
        "settings",
        "receipt",
        "marker",
        "extra",
        "state",
    ] {
        let fixture = Fixture::new(false);
        let plan = fixture.plan();
        interrupt(
            &plan,
            if mode == "backup" {
                "installed"
            } else {
                "prepared"
            },
        );
        let directory = transaction(&plan.root, plan.reference());
        let path = match mode {
            "working" => fixture.project.source().join(MODULE),
            "candidate" => directory.join("candidate").join(MODULE),
            "backup" => directory.join("previous-source").join(MODULE),
            "incoming" | "ancestor" => directory.join(mode).join("ancestor.json"),
            "approved" => fixture.project.root().join(".eska/accepted/accepted.json"),
            "settings" => fixture.project.root().join("eska.toml"),
            "receipt" => directory.join(RECEIPT),
            "marker" => fixture.project.root().join(PROJECT_SOURCE_IMPORT_RECEIPT),
            "state" => fixture.project.root().join(STATE),
            _ => directory.join("foreign.txt"),
        };
        fs::write(path, "foreign data must survive").unwrap();
        let before = Inventory::read(fixture.project.root()).unwrap();
        for action in [
            SourceImportRecovery::Complete,
            SourceImportRecovery::RollBack,
        ] {
            assert!(
                recover_project_source_import(
                    fixture.project.root(),
                    plan.reference(),
                    action,
                    &|| false
                )
                .is_err(),
                "{mode}"
            );
            assert_eq!(
                Inventory::read(fixture.project.root()).unwrap(),
                before,
                "{mode}"
            );
        }
    }
    let fixture = Fixture::new(false);
    let plan = fixture.plan();
    interrupt(&plan, "prepared");
    let wrong = ProjectSourceImportRef::from_sha256(&"f".repeat(64)).unwrap();
    let before = Inventory::read(fixture.project.root()).unwrap();
    assert!(
        recover_project_source_import(
            fixture.project.root(),
            &wrong,
            SourceImportRecovery::Complete,
            &|| false
        )
        .is_err()
    );
    assert_eq!(Inventory::read(fixture.project.root()).unwrap(), before);
    assert!(ProjectSourceImportRef::from_sha256("../../other").is_err());
}

/// Cancellation at all mutation boundaries preserves guarded recovery and never exposes partial source.
#[test]
fn cancellation_retains_recovery_at_each_mutation_boundary() {
    for point in ["before", "guard", "backup", "installed", "state"] {
        let fixture = Fixture::new(false);
        let plan = fixture.plan();
        let directory = transaction(&plan.root, plan.reference());
        let cancelled = || match point {
            "before" => true,
            "guard" => fixture
                .project
                .root()
                .join(PROJECT_SOURCE_IMPORT_RECEIPT)
                .exists(),
            "backup" => directory.join("previous-source").exists(),
            "installed" => {
                directory.join("previous-source").exists() && fixture.project.source().exists()
            }
            _ => fixture.project.root().join(STATE).exists(),
        };
        assert_eq!(
            plan.publish_cancellable(&cancelled).unwrap_err().code,
            "source_read_cancelled",
            "{point}"
        );
        if point == "before" {
            assert!(!fixture.project.root().join(SERVICE).exists());
        } else {
            assert_eq!(
                require_project_ready(fixture.project.root())
                    .unwrap_err()
                    .code,
                "source_import_incomplete"
            );
            recover_project_source_import(
                fixture.project.root(),
                plan.reference(),
                SourceImportRecovery::Complete,
                &|| false,
            )
            .unwrap();
        }
        require_project_ready(fixture.project.root()).unwrap();
        assert_eq!(
            fs::read(fixture.project.source().join(MODULE)).unwrap(),
            LOCAL
        );
    }
}

/// A preview cannot be reused after settings, support or any source participant changes.
#[test]
fn conflicts_and_stale_inputs_create_no_service_journal() {
    let fixture = Fixture::new(true);
    let plan = fixture.plan();
    assert!(!plan.source_plan().conflicts().is_empty());
    assert_eq!(plan.publish().unwrap_err().code, "source_import_conflicts");
    assert!(!fixture.project.root().join(SERVICE).exists());
    for mode in ["working", "incoming", "ancestor", "approved", "settings"] {
        let fixture = Fixture::new(false);
        let plan = fixture.plan();
        let path = match mode {
            "working" => fixture.project.source().join(MODULE),
            "incoming" => fixture.incoming.root.join("ancestor.json"),
            "ancestor" => fixture.ancestor.root.join("ancestor.json"),
            "approved" => fixture.project.root().join(".eska/accepted/accepted.json"),
            _ => fixture.project.root().join("eska.toml"),
        };
        fs::write(path, "changed after preview").unwrap();
        let before = Inventory::read(fixture.project.root()).unwrap();
        assert!(plan.publish().is_err(), "{mode}");
        assert_eq!(Inventory::read(fixture.project.root()).unwrap(), before);
        assert!(!fixture.project.root().join(SERVICE).exists());
    }
}

/// The containing workspace and a different member remain byte-identical through selected import.
#[test]
fn selected_member_retains_inheritance_and_refuses_wrong_scope() {
    let fixture = Fixture::new(false);
    let member = fixture.project.root();
    let manifest = member.join("eska.toml");
    fs::write(
        &manifest,
        ProjectConfig::load(&manifest)
            .unwrap()
            .to_workspace_member_toml()
            .unwrap(),
    )
    .unwrap();
    let other = fixture.test.0.join("other");
    fs::create_dir(&other).unwrap();
    fixture.ancestor.restore(&other.join("src")).unwrap();
    fs::create_dir(other.join(".eska")).unwrap();
    accept_source(&other.join("src"), &other.join(".eska/accepted"), &[]).unwrap();
    fs::write(
        other.join("eska.toml"),
        fs::read_to_string(&manifest)
            .unwrap()
            .replace("sample", "other"),
    )
    .unwrap();
    fs::write(fixture.test.0.join("eska.toml"), "[workspace]\nmembers = ['project', 'other']\n[build]\nplatform_version = '8.3.27.2325'\nartifacts_directory = 'dist'\n[vcs.workflow]\npreset = 'trunk'\n").unwrap();
    let DiscoveryContext::Workspace { workspace, .. } = discover_context(member).unwrap() else {
        panic!("workspace")
    };
    let project = workspace
        .members()
        .iter()
        .find(|member| member.name().as_str() == "sample")
        .unwrap()
        .project();
    let scope = ProjectScope::Member(ProjectName::parse("sample".into()).unwrap());
    for wrong in [
        ProjectScope::Standalone,
        ProjectScope::Member(ProjectName::parse("other".into()).unwrap()),
    ] {
        assert_eq!(
            prepare_project_source_import(
                project,
                &wrong,
                &fixture.approved,
                &fixture.ancestor,
                &fixture.incoming
            )
            .err()
            .unwrap()
            .code,
            "source_import_scope_mismatch"
        );
    }
    let other_before = Inventory::read(&other).unwrap();
    let settings = fs::read(&manifest).unwrap();
    let plan = prepare_project_source_import(
        project,
        &scope,
        &fixture.approved,
        &fixture.ancestor,
        &fixture.incoming,
    )
    .unwrap();
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
    assert_eq!(Inventory::read(&other).unwrap(), other_before);
    assert_eq!(fs::read(&manifest).unwrap(), settings);
    assert!(project_import_ancestor(project, &scope, fixture.incoming.reference()).is_ok());
    let reloaded = discover_context(member).unwrap();
    assert!(matches!(reloaded, DiscoveryContext::Workspace { .. }));
}

/// A missing or self-advertised state cannot replace the independently supplied common ancestor.
#[test]
fn state_requires_the_independently_confirmed_ancestor() {
    let fixture = Fixture::new(false);
    assert_eq!(
        project_import_ancestor(
            &fixture.project,
            &ProjectScope::Standalone,
            fixture.ancestor.reference()
        )
        .err()
        .unwrap()
        .code,
        "source_ancestor_missing"
    );
    let plan = fixture.plan();
    plan.publish().unwrap();
    assert_eq!(
        project_import_ancestor(
            &fixture.project,
            &ProjectScope::Standalone,
            fixture.ancestor.reference()
        )
        .err()
        .unwrap()
        .code,
        "source_ancestor_untrusted"
    );
    assert_eq!(
        prepare_project_source_import(
            &fixture.project,
            &ProjectScope::Standalone,
            &fixture.approved,
            &fixture.ancestor,
            &fixture.incoming
        )
        .err()
        .unwrap()
        .code,
        "source_ancestor_untrusted"
    );
}

/// Inherited root settings are exact inputs in both a preview and an already guarded recovery.
#[test]
fn changed_workspace_settings_block_publication_and_recovery() {
    for interrupted in [false, true] {
        let fixture = Fixture::new(false);
        let path = fixture.project.root().join("eska.toml");
        fs::write(
            &path,
            ProjectConfig::load(&path)
                .unwrap()
                .to_workspace_member_toml()
                .unwrap(),
        )
        .unwrap();
        let settings = fixture.test.0.join("eska.toml");
        fs::write(
            &settings,
            "[workspace]\nmembers = ['project']\n[build]\nplatform_version = '8.3.27.2325'\n",
        )
        .unwrap();
        let DiscoveryContext::Workspace { workspace, .. } =
            discover_context(fixture.project.root()).unwrap()
        else {
            panic!("workspace")
        };
        let project = workspace.members()[0].project();
        let scope = ProjectScope::Member(ProjectName::parse("sample".into()).unwrap());
        let plan = prepare_project_source_import(
            project,
            &scope,
            &fixture.approved,
            &fixture.ancestor,
            &fixture.incoming,
        )
        .unwrap();
        if interrupted {
            interrupt(&plan, "backed-up");
        }
        fs::write(
            &settings,
            format!(
                "{}\n# changed inheritance\n",
                fs::read_to_string(&settings).unwrap()
            ),
        )
        .unwrap();
        let before = Inventory::read(&fixture.test.0).unwrap();
        assert_eq!(plan.publish().unwrap_err().code, "source_input_changed");
        if interrupted {
            assert_eq!(
                recover_project_source_import(
                    project.root(),
                    plan.reference(),
                    SourceImportRecovery::RollBack,
                    &|| false
                )
                .unwrap_err()
                .code,
                "source_input_changed"
            );
        }
        assert_eq!(Inventory::read(&fixture.test.0).unwrap(), before);
    }
}

/// Stop inside a large module copy removes only unguarded staging and retains authored bytes.
#[test]
fn cancellation_during_module_copy_preserves_the_working_tree() {
    let fixture = Fixture::new(false);
    fs::write(
        fixture.project.source().join(MODULE),
        vec![b'/'; 2 * 1024 * 1024],
    )
    .unwrap();
    let before = Inventory::read(fixture.project.source()).unwrap();
    let plan = fixture.plan();
    assert!(plan.source_plan().conflicts().is_empty());
    let directory = transaction(fixture.project.root(), plan.reference());
    let payload = directory.join("candidate").join(MODULE);
    let cancelled = || fs::metadata(&payload).is_ok_and(|metadata| metadata.len() >= 16 * 1024);
    assert_eq!(
        plan.publish_cancellable(&cancelled).unwrap_err().code,
        "source_read_cancelled"
    );
    assert!(!directory.exists());
    require_project_ready(fixture.project.root()).unwrap();
    assert_eq!(Inventory::read(fixture.project.source()).unwrap(), before);
}

/// A second journal preserves and restores the previous successful pointer during rollback.
#[test]
fn rollback_restores_existing_ancestry_and_the_same_plan_can_resume() {
    let fixture = Fixture::new(false);
    fixture.plan().publish().unwrap();
    let previous = fs::read(fixture.project.root().join(STATE)).unwrap();
    let working = Inventory::read(fixture.project.source()).unwrap();
    let saved = project_import_ancestor(
        &fixture.project,
        &ProjectScope::Standalone,
        fixture.incoming.reference(),
    )
    .unwrap();
    let next_source = fixture.test.0.join("next-source");
    saved.restore(&next_source).unwrap();
    let path = next_source.join("configuration.yaml");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap().replace("1.0.1", "1.0.2"),
    )
    .unwrap();
    let store = fixture.test.0.join("next");
    let reference = capture_import_ancestor(&next_source, &store).unwrap();
    let incoming = ImportAncestor::open(&store, &reference).unwrap();
    let plan = prepare_project_source_import(
        &fixture.project,
        &ProjectScope::Standalone,
        &fixture.approved,
        &saved,
        &incoming,
    )
    .unwrap();
    interrupt(&plan, "state");
    recover_project_source_import(
        fixture.project.root(),
        plan.reference(),
        SourceImportRecovery::RollBack,
        &|| false,
    )
    .unwrap();
    assert_eq!(
        fs::read(fixture.project.root().join(STATE)).unwrap(),
        previous
    );
    assert_eq!(Inventory::read(fixture.project.source()).unwrap(), working);
    assert!(
        project_import_ancestor(
            &fixture.project,
            &ProjectScope::Standalone,
            fixture.incoming.reference()
        )
        .is_ok()
    );
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Resumed);
    assert!(
        project_import_ancestor(&fixture.project, &ProjectScope::Standalone, &reference).is_ok()
    );
    assert_eq!(
        fs::read(fixture.project.source().join(MODULE)).unwrap(),
        LOCAL
    );
}

#[cfg(unix)]
/// Neither a service alias nor a source alias redirects staging or recovery into a foreign tree.
#[test]
fn symlinks_refuse_publication_without_touching_their_targets() {
    use std::os::unix::fs::symlink;
    for alias in ["source", "service"] {
        let fixture = Fixture::new(false);
        let plan = fixture.plan();
        let foreign = fixture.test.0.join("foreign");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("keep.txt"), "foreign").unwrap();
        if alias == "source" {
            fs::rename(
                fixture.project.source(),
                fixture.project.root().join("saved-source"),
            )
            .unwrap();
            symlink(&foreign, fixture.project.source()).unwrap();
        } else {
            symlink(&foreign, fixture.project.root().join(SERVICE)).unwrap();
        }
        let before = Inventory::read(&foreign).unwrap();
        assert!(plan.publish().is_err());
        assert_eq!(Inventory::read(&foreign).unwrap(), before);
        assert!(
            !fixture
                .project
                .root()
                .join(PROJECT_SOURCE_IMPORT_RECEIPT)
                .exists()
        );
    }
}
