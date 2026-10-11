//! Canonical form slots and module metadata share current values without reading native dialogs.

use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use base_db::{SourceDatabase, SourceRoot, SourceRootId};
use bsl_metadata::{Form, FormType, Uuid};
use hir::DefDatabase;
use salsa::Setter;
use vfs::{file_set::FileSet, FileId, VfsPath};

use crate::{
    metadata::{resolve_form, CanonicalMetadata, CanonicalMetadataInput, MetadataListingData},
    RootDatabaseImpl,
};

/// Build a source-owned model with explicit identity and one observable handler.
fn form(name: &str, handler: &str) -> Arc<Form> {
    let mut form = Form::new(name.into(), FormType::Managed, Uuid::nil());
    form.command_handlers.push(handler.into());
    Arc::new(form)
}

/// No native main or resource files exist; all payloads live in tracked slots.
#[test]
fn canonical_forms_retain_neighbors_and_reject_unavailable_or_wrong_identity() {
    let mut database = RootDatabaseImpl::new_with_salsa_events();
    let saved = form("ТекущаяФорма", "Saved");
    let neighbor = form("Neighbor", "Other");
    let provided = CanonicalMetadataInput::new(
        &database,
        Some(Arc::new(CanonicalMetadata::Form(Arc::clone(&saved)))),
        "saved".into(),
    );
    let untouched = CanonicalMetadataInput::new(
        &database,
        Some(Arc::new(CanonicalMetadata::Form(Arc::clone(&neighbor)))),
        "neighbor".into(),
    );
    assert_eq!(resolve_form(&database, provided, "текущаяформа".into()), Some(saved));
    let retained = resolve_form(&database, untouched, "Neighbor".into()).unwrap();
    let executions = database.salsa_family_churn("resolve_form").unwrap().0;
    let current = form("ТекущаяФорма", "Current");
    provided
        .set_value(&mut database)
        .to(Some(Arc::new(CanonicalMetadata::Form(Arc::clone(&current)))));
    provided.set_source_revision(&mut database).to("editor:1".into());
    assert!(Arc::ptr_eq(
        &retained,
        &resolve_form(&database, untouched, "Neighbor".into()).unwrap()
    ));
    assert_eq!(database.salsa_family_churn("resolve_form").unwrap().0, executions);
    assert_eq!(resolve_form(&database, provided, "текущаяформа".into()), Some(current));
    provided.set_value(&mut database).to(None);
    assert!(resolve_form(&database, provided, "текущаяформа".into()).is_none());
    provided
        .set_value(&mut database)
        .to(Some(Arc::new(CanonicalMetadata::Form(form("Different", "Wrong")))));
    assert!(resolve_form(&database, provided, "текущаяформа".into()).is_none());
    provided.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::Role(Arc::new(
        bsl_metadata::Role::new("ТекущаяФорма"),
    )))));
    assert!(resolve_form(&database, provided, "текущаяформа".into()).is_none());
}

/// Body metadata routes by stable descriptor FileId; canonical misses stay explicitly unavailable.
#[test]
fn canonical_form_modules_follow_slots_and_keep_unprovided_nested_forms_unavailable() {
    let mut database = RootDatabaseImpl::new_with_salsa_events();
    let root = PathBuf::from("/canonical-form-slots");
    database.set_all_config_paths(vec![(None, root.clone())]);
    let mut files = FileSet::new();
    for (id, path) in [
        (0, "CommonForms/Dialog.xml"),
        (1, "CommonForms/Dialog/Ext/Form/Module.bsl"),
        (2, "CommonForms/Neighbor.xml"),
        (3, "CommonForms/Neighbor/Ext/Form/Module.bsl"),
        (4, "Catalogs/Items/Forms/Nested/Ext/Form/Module.bsl"),
    ] {
        files.insert(FileId(id), VfsPath::new(root.join(path)));
        database.set_file_source_root(FileId(id), SourceRootId(0));
        database.set_file_unreadable(FileId(id));
    }
    database.set_source_root(SourceRootId(0), SourceRoot::new_local(files));
    database.set_metadata_listing(root.to_str().unwrap(), MetadataListingData::default());
    let listing = database.metadata_listing(root.to_str().unwrap()).unwrap();
    let provided = CanonicalMetadataInput::new(
        &database,
        Some(Arc::new(CanonicalMetadata::Form(form("Dialog", "Saved")))),
        "saved".into(),
    );
    let neighbor = CanonicalMetadataInput::new(
        &database,
        Some(Arc::new(CanonicalMetadata::Form(form("Neighbor", "Other")))),
        "neighbor".into(),
    );
    listing
        .set_canonical_metadata(&mut database)
        .to(Arc::new([(FileId(0), provided), (FileId(2), neighbor)].into_iter().collect()));
    let metadata = |database: &RootDatabaseImpl, id| {
        database.module_metadata(hir::ModuleId { file_id: FileId(id) })
    };
    assert_eq!(metadata(&database, 1).form.as_ref().unwrap().command_handlers, ["Saved"]);
    let retained = metadata(&database, 3);
    let nested = metadata(&database, 4);
    assert_eq!(nested.module_type, bsl_metadata::ModuleType::FormModule);
    assert!(nested.form.is_none());
    provided
        .set_value(&mut database)
        .to(Some(Arc::new(CanonicalMetadata::Form(form("Dialog", "Current")))));
    provided.set_source_revision(&mut database).to("editor:1".into());
    assert_eq!(metadata(&database, 1).form.as_ref().unwrap().command_handlers, ["Current"]);
    assert!(Arc::ptr_eq(&retained, &metadata(&database, 3)));
    provided.set_value(&mut database).to(None);
    assert!(metadata(&database, 1).form.is_none());
    assert!(metadata(&database, 4).form.is_none());
}

struct OwnedDirectory(PathBuf);

impl OwnedDirectory {
    /// Claim one exclusive playground directory without using existing user demos.
    fn new() -> Self {
        let root = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").expect("playground root"));
        let root = root.canonicalize().unwrap();
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = root.join(format!("eska-kernel-form-test-{}-{timestamp}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for OwnedDirectory {
    /// Remove only the directory this test successfully claimed.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Real native dialog bytes cannot rescue a pending canonical slot; legacy mode still reads them.
#[test]
fn current_form_slots_override_disk_and_legacy_loading_remains_available() {
    let directory = OwnedDirectory::new();
    let root = &directory.0;
    fs::create_dir_all(root.join("CommonForms/Dialog/Ext/Form")).unwrap();
    fs::write(root.join("CommonForms/Dialog.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><CommonForm uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>Dialog</Name><FormType>Managed</FormType></Properties></CommonForm></MetaDataObject>"#).unwrap();
    fs::write(
        root.join("CommonForms/Dialog/Ext/Form.xml"),
        "<Form><Commands><Command><Action>OnDisk</Action></Command></Commands></Form>",
    )
    .unwrap();
    let mut database = RootDatabaseImpl::new();
    database.set_all_config_paths(vec![(None, root.clone())]);
    let mut files = FileSet::new();
    for (id, path) in [(0, "CommonForms/Dialog.xml"), (1, "CommonForms/Dialog/Ext/Form/Module.bsl")]
    {
        files.insert(FileId(id), VfsPath::new(root.join(path)));
        database.set_file_source_root(FileId(id), SourceRootId(0));
        database.set_file_unreadable(FileId(id));
    }
    database.set_source_root(SourceRootId(0), SourceRoot::new_local(files));
    database.set_metadata_listing(root.to_str().unwrap(), MetadataListingData::default());
    let listing = database.metadata_listing(root.to_str().unwrap()).unwrap();
    let slot = CanonicalMetadataInput::new(&database, None, "pending".into());
    listing
        .set_canonical_metadata(&mut database)
        .to(Arc::new([(FileId(0), slot)].into_iter().collect()));
    let module = hir::ModuleId { file_id: FileId(1) };
    assert!(database.module_metadata(module).form.is_none());
    slot.set_value(&mut database)
        .to(Some(Arc::new(CanonicalMetadata::Form(form("Dialog", "Current")))));
    assert_eq!(
        database.module_metadata(module).form.as_ref().unwrap().command_handlers,
        ["Current"]
    );
    slot.set_value(&mut database).to(None);
    assert!(database.module_metadata(module).form.is_none());
    listing.set_canonical_metadata(&mut database).to(Arc::default());
    assert_eq!(
        database.module_metadata(module).form.as_ref().unwrap().command_handlers,
        ["OnDisk"]
    );
    // A visible base root cannot claim a standalone form that does not physically belong to it.
    let unrelated = root.join("Unrelated");
    database.set_all_config_paths(vec![(None, unrelated.clone())]);
    database.set_metadata_listing(unrelated.to_str().unwrap(), MetadataListingData::default());
    let unrelated_listing = database.metadata_listing(unrelated.to_str().unwrap()).unwrap();
    let foreign = CanonicalMetadataInput::new(
        &database,
        Some(Arc::new(CanonicalMetadata::Form(form("Dialog", "Foreign")))),
        "foreign".into(),
    );
    unrelated_listing
        .set_canonical_metadata(&mut database)
        .to(Arc::new([(FileId(0), foreign)].into_iter().collect()));
    assert_eq!(
        database.module_metadata(module).form.as_ref().unwrap().command_handlers,
        ["OnDisk"]
    );
    assert_eq!(
        fs::read_to_string(root.join("CommonForms/Dialog/Ext/Form.xml")).unwrap(),
        "<Form><Commands><Command><Action>OnDisk</Action></Command></Commands></Form>"
    );
}
