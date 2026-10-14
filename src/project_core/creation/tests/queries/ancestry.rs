//! Initial snapshots remain independent from accepted support and later merged working files.

use super::*;
use crate::project_core::source_format::{
    ImportAncestor, ImportAncestorRef, LayoutPublication, PROJECT_SOURCE_IMPORT_RECEIPT,
    SourceImportRecovery, capture_import_ancestor, prepare_project_source_import,
    project_import_ancestor, recover_project_source_import,
};

/// A new project retains exact initial source bytes even after local edits and a later native import.
#[test]
fn initial_ancestry_is_separate_from_support_and_advances_after_first_import() {
    for kind in fixture::TYPES {
        let test = TestDir::new();
        let native = fixture::native(&test.0, kind);
        let imported =
            import_query_project(&native.input, &test.0.join("project"), name(), None, &[])
                .unwrap();
        let reference = imported.initial_ancestor.as_ref().unwrap();
        assert_ne!(reference.sha256(), imported.accepted.sha256);
        let project = &imported.project;
        let scope = ProjectScope::Standalone;
        let initial = fixture::snapshot(project.source());
        let accepted = fixture::snapshot(&project.root().join(ACCEPTED_DIRECTORY));
        let state: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(project.root().join(".eska/source-import/state.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(state["schema_version"], 2);
        assert_eq!(state["origin"], "initial");
        assert_eq!(state["source"], "src");
        assert!(state.get("transaction").is_none());
        let wrong = ImportAncestorRef::from_sha256(&imported.accepted.sha256).unwrap();
        assert_eq!(
            project_import_ancestor(project, &scope, &wrong)
                .err()
                .unwrap()
                .code,
            "source_ancestor_untrusted"
        );
        let wrong = AcceptedRef {
            sha256: reference.sha256().into(),
        };
        assert_eq!(
            AcceptedBase::open(&project.root().join(ACCEPTED_DIRECTORY), &wrong)
                .err()
                .unwrap()
                .code,
            "source_baseline_untrusted"
        );
        let ancestor = project_import_ancestor(project, &scope, reference).unwrap();
        let incoming = test.0.join("incoming source");
        ancestor.restore(&incoming).unwrap();
        assert_eq!(fixture::snapshot(&incoming), initial);
        let owner = incoming.join(native.root_source);
        fs::write(
            &owner,
            fs::read_to_string(&owner)
                .unwrap()
                .replace("Accepted", "Incoming"),
        )
        .unwrap();
        let incoming_store = test.0.join("incoming store");
        let next = capture_import_ancestor(&incoming, &incoming_store).unwrap();
        let incoming = ImportAncestor::open(&incoming_store, &next).unwrap();
        let module = project.source().join(native.module_source);
        let local = format!(
            "{}// local survives\r\n",
            fs::read_to_string(&module).unwrap()
        );
        fs::write(&module, &local).unwrap();
        let plan = prepare_project_source_import(
            project,
            &scope,
            &imported.accepted,
            &ancestor,
            &incoming,
        )
        .unwrap();
        assert!(plan.source_plan().conflicts().is_empty());
        assert_eq!(plan.publish().unwrap(), LayoutPublication::Created);
        assert_eq!(fs::read_to_string(&module).unwrap(), local);
        let state: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(project.root().join(".eska/source-import/state.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(state["schema_version"], 1);
        assert!(state.get("origin").is_none());
        assert!(state.get("transaction").is_some());
        let saved = project_import_ancestor(project, &scope, &next).unwrap();
        let restored = test.0.join("raw incoming");
        saved.restore(&restored).unwrap();
        assert!(
            !fs::read_to_string(restored.join(native.module_source))
                .unwrap()
                .contains("local survives")
        );
        assert_eq!(
            fixture::snapshot(&project.root().join(ACCEPTED_DIRECTORY)),
            accepted
        );
        assert_eq!(
            project_import_ancestor(project, &scope, reference)
                .err()
                .unwrap()
                .code,
            "source_ancestor_untrusted"
        );
    }
}

/// Initial no-op confirmation creates a real journal and rollback restores the exact candidate pointer.
#[test]
fn first_noop_is_journaled_and_can_roll_back_to_initial_state() {
    for kind in fixture::TYPES {
        let test = TestDir::new();
        let native = fixture::native(&test.0, kind);
        let imported =
            import_query_project(&native.input, &test.0.join("project"), name(), None, &[])
                .unwrap();
        let project = &imported.project;
        let reference = imported.initial_ancestor.as_ref().unwrap();
        let scope = ProjectScope::Standalone;
        let state = project.root().join(".eska/source-import/state.json");
        let initial_state = fs::read(&state).unwrap();
        let initial_source = fixture::snapshot(project.source());
        let ancestor = project_import_ancestor(project, &scope, reference).unwrap();
        let plan = prepare_project_source_import(
            project,
            &scope,
            &imported.accepted,
            &ancestor,
            &ancestor,
        )
        .unwrap();
        assert!(!plan.is_unchanged());
        let guard = project.root().join(PROJECT_SOURCE_IMPORT_RECEIPT);
        assert_eq!(
            plan.publish_cancellable(&|| guard.exists())
                .unwrap_err()
                .code,
            "source_read_cancelled"
        );
        assert!(guard.is_file());
        recover_project_source_import(
            project.root(),
            plan.reference(),
            SourceImportRecovery::RollBack,
            &|| false,
        )
        .unwrap();
        assert!(!guard.exists());
        assert_eq!(fs::read(&state).unwrap(), initial_state);
        assert_eq!(fixture::snapshot(project.source()), initial_source);
        project_import_ancestor(project, &scope, reference).unwrap();
        assert_eq!(plan.publish().unwrap(), LayoutPublication::Resumed);
        let confirmed = project_import_ancestor(project, &scope, reference).unwrap();
        let repeated = prepare_project_source_import(
            project,
            &scope,
            &imported.accepted,
            &confirmed,
            &confirmed,
        )
        .unwrap();
        assert!(repeated.is_unchanged());
        let before = fixture::snapshot(project.root());
        assert_eq!(repeated.publish().unwrap(), LayoutPublication::Unchanged);
        assert_eq!(fixture::snapshot(project.root()), before);
    }
}

/// Invalid initial variants or altered blobs cannot select a fallback source or advertise recovery.
#[test]
fn corrupted_initial_state_and_storage_refuse_without_changes() {
    for failure in [
        "version",
        "source",
        "origin",
        "transaction",
        "blob",
        "missing",
    ] {
        let test = TestDir::new();
        let native = fixture::native(&test.0, "configuration");
        let imported =
            import_query_project(&native.input, &test.0.join("project"), name(), None, &[])
                .unwrap();
        let reference = imported.initial_ancestor.as_ref().unwrap();
        let project = &imported.project;
        let path = project.root().join(".eska/source-import/state.json");
        let mut state: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        match failure {
            "version" => state["schema_version"] = 3.into(),
            "source" => state["source"] = "another".into(),
            "origin" => state["origin"] = "published".into(),
            "transaction" => {
                state["transaction"] = serde_json::json!({"sha256":reference.sha256()});
            }
            "blob" => fs::write(
                project
                    .root()
                    .join(".eska/source-import/initial/ancestor.json"),
                "{}",
            )
            .unwrap(),
            "missing" => fs::remove_file(&path).unwrap(),
            _ => unreachable!(),
        }
        if !matches!(failure, "blob" | "missing") {
            fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
        }
        let before = fixture::snapshot(project.root());
        let error = project_import_ancestor(project, &ProjectScope::Standalone, reference)
            .err()
            .unwrap();
        assert_eq!(
            error.code,
            match failure {
                "blob" => "source_ancestor_untrusted",
                "missing" => "source_ancestor_missing",
                _ => "source_import_state_invalid",
            }
        );
        assert_eq!(fixture::snapshot(project.root()), before);
    }
}
