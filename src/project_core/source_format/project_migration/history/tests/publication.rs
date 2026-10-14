//! Resumption owns unchanged raw bytes through the ordinary storage and outer migration receipts.

use super::*;
use crate::project_core::source_format::{
    layout::migration::publication as storage,
    project_migration::{PROJECT_LAYOUT_RECEIPT, publish_new},
};

/// Even a copied active state stays unavailable until every source, base and history byte is ready.
#[test]
fn interrupted_history_copy_resumes_without_rewriting_journals() {
    let test = TestDir::new();
    let imported = creation::import_query_project(
        &native(&test),
        &test.0.join("old"),
        ProjectName::parse("sample".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let reference = publish_incoming(&imported, &test);
    let plan = prepare_query_project(
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
    let history = plan.history.as_ref().unwrap();
    let destination = plan.destination().join(history.path.as_path());
    storage::interrupt(history.view(), &destination);
    fs::copy(
        history.root.join("state.json"),
        destination.join("state.json"),
    )
    .unwrap();
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Resumed);
    assert_eq!(
        Inventory::read(&destination).unwrap(),
        Inventory::read(&history.root).unwrap()
    );
    let copied = discover(plan.destination()).unwrap();
    project_import_ancestor(&copied, &ProjectScope::Standalone, &reference).unwrap();
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
}

/// The root receipt cannot authorize an edited raw file or publish other trees before checking it.
#[test]
fn edited_partial_history_refuses_before_source_or_base_publication() {
    let test = TestDir::new();
    let imported = creation::import_query_project(
        &native(&test),
        &test.0.join("old"),
        ProjectName::parse("sample".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let plan = prepare_query_project(
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
    let history = plan.history.as_ref().unwrap();
    let destination = plan.destination().join(history.path.as_path());
    storage::interrupt(history.view(), &destination);
    fs::write(destination.join("state.json"), "{}").unwrap();
    let before = Inventory::read(&destination).unwrap();
    assert_eq!(
        plan.publish().unwrap_err().code,
        "source_layout_destination_mismatch"
    );
    assert_eq!(Inventory::read(&destination).unwrap(), before);
    assert!(!plan.source_destination().exists());
    assert!(!plan.base_destination().exists());
}
