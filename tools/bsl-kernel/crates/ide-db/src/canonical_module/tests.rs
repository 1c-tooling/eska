//! Canonical owners share exact current slots with native consumers while disk XML stays inert.

mod external;
mod external_configuration;
mod external_identity;
mod families;
mod inheritance;
mod resolution;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use base_db::{SourceDatabase, SourceRoot, SourceRootId};
use bsl_metadata::{metadata_object::Attribute, AttributeType, MdoType, MetadataObject};
use hir::DefDatabase;
use salsa::Setter;
use vfs::{FileId, FileSet, VfsPath};

use crate::{
    metadata::{
        CanonicalMetadata, CanonicalMetadataInput, MetadataListingData, RootKind,
        WorkspaceConfigsSnapshot,
    },
    RootDatabaseImpl,
};

struct Directory(PathBuf);

impl Directory {
    /// Claim only a unique sibling-playground directory, preserving every pre-existing demo.
    fn new() -> Self {
        let root = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").expect("playground root"));
        let root = root.canonicalize().expect("absolute playground root");
        let time = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
        let path = root.join(format!("eska-kernel-owner-{}-{time}", std::process::id()));
        fs::create_dir(&path).expect("exclusive test directory");
        Self(path)
    }
}

impl Drop for Directory {
    /// Remove only the directory whose creation this fixture exclusively owned.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    directory: Directory,
    database: RootDatabaseImpl,
    files: FileSet,
}

impl Fixture {
    /// Real directory roots and virtual native descriptors exercise the production module query.
    fn new() -> Self {
        Self {
            directory: Directory::new(),
            database: RootDatabaseImpl::new_with_salsa_events(),
            files: FileSet::new(),
        }
    }

    /// Every root has an explicit kind and closure; no project loader discovers another source set.
    fn roots(&mut self, roots: &[(&str, RootKind, &[usize])]) -> Vec<PathBuf> {
        let paths = roots
            .iter()
            .map(|(name, kind, _)| {
                let path = self.directory.0.join(name);
                fs::create_dir_all(&path).expect("test root");
                ((!matches!(kind, RootKind::Base)).then(|| (*name).into()), path)
            })
            .collect::<Vec<_>>();
        self.database.set_workspace_configs_snapshot(WorkspaceConfigsSnapshot {
            canonical_paths: paths.iter().map(|(_, path)| path.clone()).collect(),
            kinds: roots.iter().map(|(_, kind, _)| *kind).collect(),
            closures: roots.iter().map(|(_, _, dependencies)| dependencies.to_vec()).collect(),
            topological_order: (0..roots.len()).collect(),
            fingerprint: None,
            source_exclusions: Default::default(),
            paths: paths.clone(),
        });
        paths.into_iter().map(|(_, path)| path).collect()
    }

    /// Main XML identities are registered unreadable; bodies retain only their supplied BSL bytes.
    fn file(&mut self, root: &Path, id: u32, relative: &str, text: Option<&str>) {
        let id = FileId(id);
        self.files.insert(id, VfsPath::new(root.join(relative)));
        self.database.set_file_source_root(id, SourceRootId(0));
        if let Some(text) = text {
            self.database.set_file_text(id, text);
        } else {
            self.database.set_file_unreadable(id);
        }
        self.database.set_source_root(SourceRootId(0), SourceRoot::new_local(self.files.clone()));
    }

    /// Immutable listings retain mutable values under stable file identities.
    fn listing(
        &mut self,
        root: &Path,
        listing: MetadataListingData,
        slots: &[(u32, CanonicalMetadataInput)],
    ) {
        self.database.set_metadata_listing(root.to_str().expect("UTF-8 root"), listing);
        let listing = self.database.metadata_listing(root.to_str().unwrap()).expect("listing");
        listing
            .set_canonical_metadata(&mut self.database)
            .to(Arc::new(slots.iter().map(|(id, slot)| (FileId(*id), *slot)).collect()));
    }

    /// A pending slot is still an explicit canonical provider and never a fallback permission.
    fn slot(&self, value: Option<CanonicalMetadata>) -> CanonicalMetadataInput {
        CanonicalMetadataInput::new(&self.database, value.map(Arc::new), "saved".into())
    }

    /// The production Salsa query must return the tracked owner without a synthetic configuration.
    fn module(&self, id: u32) -> Arc<hir::ModuleMetadata> {
        self.database.module_metadata(hir::ModuleId { file_id: FileId(id) })
    }
}

/// Observable field names and real source UUIDs distinguish current payloads from native fallbacks.
fn object(kind: MdoType, name: &str, field: &str) -> Arc<MetadataObject> {
    let mut value = MetadataObject::new(kind, name);
    value.set_uuid(bsl_metadata::Uuid::from_u128(1));
    value.add_attribute(Attribute {
        name: field.into(),
        name_en: None,
        attr_type: AttributeType::String { length: None },
    });
    Arc::new(value)
}

/// Updating one source input uses the same accepted value/revision boundary as the SDK host.
fn update(fixture: &mut Fixture, slot: CanonicalMetadataInput, value: Option<CanonicalMetadata>) {
    slot.set_value(&mut fixture.database).to(value.map(Arc::new));
    slot.set_source_revision(&mut fixture.database).to("editor".into());
}
