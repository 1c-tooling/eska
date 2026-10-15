//! Exact current settings_storage declarations own their module backlinks and never read an XML sidecar.

use super::{CanonicalMetadata, MetadataListingInput};
use std::sync::Arc;
use vfs::FileId;

/// Module identity is enrolled from the manifest before either YAML or BSL becomes readable.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SettingsStorageEntry {
    pub name: String,
    pub main: FileId,
    pub module_file: Option<FileId>,
}

/// Refuse ambiguous names, unavailable current inputs and values belonging to another family.
#[salsa::tracked(heap_size = stdx::heap::zero, returns(clone))]
pub fn resolve_settings_storage(
    db: &dyn base_db::SourceDatabase,
    listing: MetadataListingInput,
    name: String,
) -> Option<Arc<bsl_metadata::SettingsStorage>> {
    let mut entries = listing
        .settings_storages(db)
        .iter()
        .filter(|entry| stdx::case::eq_ignore_case(&entry.name, &name));
    let main = entries.next()?.main;
    if entries.next().is_some() {
        return None;
    }
    let input = listing.canonical_metadata(db).get(&main)?;
    let _revision = input.source_revision(db);
    let value = input.value(db).as_ref()?;
    let CanonicalMetadata::SettingsStorage(value) = value.as_ref() else {
        return None;
    };
    stdx::case::eq_ignore_case(value.name(), &name).then(|| Arc::clone(value))
}
