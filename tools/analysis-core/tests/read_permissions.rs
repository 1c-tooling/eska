#![cfg(feature = "analyzer-cli")]

pub use eska_analysis::{AnalysisSession, RequestCancellation, SessionOptions};

#[path = "../../bsl-adapter/tests/support/mod.rs"]
mod directory;
#[path = "support/project.rs"]
mod project;
#[path = "../src/analyzer_cli/read_permissions.rs"]
mod read_permissions;
#[path = "support/structural_project.rs"]
mod structural_project;

use eska::{
    analysis_core::InputRequest,
    project_core::{
        eska_source::EskaSource,
        source_format::{NativeUuid, SourcePath, accept_source},
    },
    tool_cli,
};

/// Open one selected authoring source using the same manifest/discovery boundary as the server.
fn open_source(root: &std::path::Path) -> EskaSource {
    let selected = tool_cli::selected(root, None).unwrap_or_else(|error| panic!("{}", error.code));
    EskaSource::open(selected.project, selected.scope).unwrap()
}

/// Both module and extracted query permissions stay unknown until their current owner has been read.
#[test]
fn pending_module_and_resource_owners_require_current_input() {
    let directory = directory::TestDirectory::new();
    let root = structural_project::project(&directory.0);
    structural_project::settings(&root);
    std::fs::create_dir(root.join(".eska")).unwrap();
    let pin = accept_source(&root.join("src"), &root.join(".eska/accepted"), &[]).unwrap();
    let source = open_source(&root);
    let module = source.reader().module_paths().next().unwrap().clone();
    let module_owner = source
        .reader()
        .declared_module(&module)
        .unwrap()
        .owner
        .clone();
    let query = source.reader().query_paths().next().unwrap().clone();
    let query_owner = source
        .reader()
        .declared_query(&query)
        .unwrap()
        .owner
        .clone();
    let permissions = read_permissions::ReadPermissions::selected(&source, Some(pin));
    let mut session =
        AnalysisSession::from_reader(source.into_reader(), SessionOptions::default()).unwrap();
    let paths = [module, query];
    let files = permissions
        .files(&session, &paths, &RequestCancellation::default())
        .unwrap();
    assert!(
        files
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["unknown"] == true && file["read_only"] == true)
    );
    for owner in [module_owner, query_owner] {
        session
            .load_saved(&owner, session.revision(), &InputRequest::default())
            .unwrap();
    }
    let files = permissions
        .files(&session, &paths, &RequestCancellation::default())
        .unwrap();
    assert!(
        files
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["unknown"] == false && file["read_only"] == false)
    );
    let no_pin_source = open_source(&root);
    let no_pin = read_permissions::ReadPermissions::selected(&no_pin_source, None);
    let cancelled = RequestCancellation::default();
    cancelled.cancel();
    assert_eq!(
        no_pin.files(&session, &paths, &cancelled).unwrap_err().code,
        "source_read_cancelled"
    );
}

/// Missing approval does not read or adopt an arbitrary accepted candidate on disk.
#[test]
fn no_pin_is_unknown_without_opening_accepted_storage() {
    let directory = directory::TestDirectory::new();
    let root = project::project(&directory.0, "// Module.\n", false);
    std::fs::create_dir(root.join(".eska")).unwrap();
    std::fs::write(root.join(".eska/accepted"), "not an approved baseline").unwrap();
    let source = open_source(&root);
    let paths = source.reader().owner_paths().cloned().collect::<Vec<_>>();
    let permissions = read_permissions::ReadPermissions::selected(&source, None);
    let session =
        AnalysisSession::from_reader(source.into_reader(), SessionOptions::default()).unwrap();
    let files = permissions
        .files(&session, &paths, &RequestCancellation::default())
        .unwrap();
    assert!(
        files
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["unknown"] == true
                && file["read_only"] == true
                && file["objects"] == serde_json::json!([]))
    );
    let unknown = SourcePath::parse("undeclared.yaml").unwrap();
    assert_eq!(
        permissions
            .files(&session, &[unknown], &RequestCancellation::default())
            .unwrap_err()
            .code,
        "source_edit_undeclared"
    );
}

/// Native/freeze permissions protect both the physical owner and its separately saved BSL module.
#[test]
fn independent_pin_freeze_and_failed_overlay_remain_immutable() {
    let directory = directory::TestDirectory::new();
    let root = project::project(&directory.0, "// Module.\n", false);
    std::fs::create_dir(root.join(".eska")).unwrap();
    let frozen = NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap();
    let pin = accept_source(&root.join("src"), &root.join(".eska/accepted"), &[frozen]).unwrap();
    let source = open_source(&root);
    let module = source.reader().module_paths().next().unwrap().clone();
    let owner = source
        .reader()
        .declared_module(&module)
        .unwrap()
        .owner
        .clone();
    let permissions = read_permissions::ReadPermissions::selected(&source, Some(pin));
    let mut session =
        AnalysisSession::from_reader(source.into_reader(), SessionOptions::default()).unwrap();
    session
        .load_saved(&owner, session.revision(), &InputRequest::default())
        .unwrap();
    let paths = [owner.clone(), module];
    let files = permissions
        .files(&session, &paths, &RequestCancellation::default())
        .unwrap();
    assert!(
        files
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["read_only"] == true && file["unknown"] == false)
    );
    assert_eq!(files[0]["objects"][0]["reason"], "eska_frozen");
    session
        .apply_overlay(
            &owner,
            session.revision(),
            1,
            "not: [valid",
            &InputRequest::default(),
        )
        .unwrap();
    let files = permissions
        .files(&session, &paths, &RequestCancellation::default())
        .unwrap();
    assert!(
        files
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["unknown"] == true && file["read_only"] == true)
    );
}

/// A changed composition or import marker cannot turn a captured policy handle into fresh approval.
#[test]
fn import_guard_changed_root_and_cancellation_refuse_policy_reads() {
    let directory = directory::TestDirectory::new();
    let root = project::inline_command_project(&directory.0, "// Command.\n");
    std::fs::create_dir(root.join(".eska")).unwrap();
    let source = open_source(&root);
    let pin = accept_source(source.reader().root(), &root.join(".eska/accepted"), &[]).unwrap();
    let owner = source.reader().configuration().path.clone();
    let permissions = read_permissions::ReadPermissions::selected(&source, Some(pin));
    let session =
        AnalysisSession::from_reader(source.into_reader(), SessionOptions::default()).unwrap();
    let cancelled = RequestCancellation::default();
    cancelled.cancel();
    assert_eq!(
        permissions
            .files(&session, std::slice::from_ref(&owner), &cancelled)
            .unwrap_err()
            .code,
        "source_read_cancelled"
    );
    let marker = root.join(eska::project_core::source_format::PROJECT_SOURCE_IMPORT_RECEIPT);
    std::fs::write(&marker, "unfinished import").unwrap();
    assert!(
        permissions
            .files(
                &session,
                std::slice::from_ref(&owner),
                &RequestCancellation::default()
            )
            .is_err()
    );
    std::fs::remove_file(marker).unwrap();
    let path = session.root().join(owner.as_path());
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{text}\n# External root edit.\n")).unwrap();
    assert_eq!(
        permissions
            .files(
                &session,
                std::slice::from_ref(&owner),
                &RequestCancellation::default()
            )
            .unwrap_err()
            .code,
        "source_input_changed"
    );
}
