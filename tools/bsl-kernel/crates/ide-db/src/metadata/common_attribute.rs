//! Common fields have declared current slots and never borrow a previous XML or unrelated root.

use super::{CanonicalMetadata, MetadataListingInput};
use std::sync::Arc;

/// Structure identifies a common declaration before its payload is ready.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CommonAttributeEntry {
    pub name: String,
    pub main: vfs::FileId,
}

/// Only an unambiguous current canonical declaration can supply typed membership.
#[salsa::tracked(heap_size = stdx::heap::zero, returns(clone))]
pub fn resolve_common_attribute(
    db: &dyn base_db::SourceDatabase,
    listing: MetadataListingInput,
    name: String,
) -> Option<Arc<bsl_metadata::CommonAttribute>> {
    let mut entries = listing
        .common_attributes(db)
        .iter()
        .filter(|entry| stdx::case::eq_ignore_case(&entry.name, &name));
    let main = entries.next()?.main;
    if entries.next().is_some() {
        return None;
    }
    let input = listing.canonical_metadata(db).get(&main)?;
    let _revision = input.source_revision(db);
    let value = input.value(db).as_ref()?;
    let CanonicalMetadata::CommonAttribute(value) = value.as_ref() else {
        return None;
    };
    stdx::case::eq_ignore_case(value.name(), &name).then(|| Arc::clone(value))
}
