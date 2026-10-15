//! Declared parameters resolve only through current canonical values, not native XML or another root.

use super::{CanonicalMetadata, MetadataListingInput};
use std::sync::Arc;

/// A name remains declared while its current descriptor is pending or invalid.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FunctionalOptionsParameterEntry {
    pub name: String,
    pub main: vfs::FileId,
}

/// An ambiguous name or wrong/current-unavailable model cannot supply a parameter.
#[salsa::tracked(heap_size = stdx::heap::zero, returns(clone))]
pub fn resolve_functional_option_parameter(
    db: &dyn base_db::SourceDatabase,
    listing: MetadataListingInput,
    name: String,
) -> Option<Arc<bsl_metadata::FunctionalOptionsParameter>> {
    let mut entries = listing
        .functional_option_parameters(db)
        .iter()
        .filter(|entry| stdx::case::eq_ignore_case(&entry.name, &name));
    let main = entries.next()?.main;
    if entries.next().is_some() {
        return None;
    }
    let input = listing.canonical_metadata(db).get(&main)?;
    let _revision = input.source_revision(db);
    let value = input.value(db).as_ref()?;
    let CanonicalMetadata::FunctionalOptionsParameter(value) = value.as_ref() else {
        return None;
    };
    stdx::case::eq_ignore_case(value.name(), &name).then(|| Arc::clone(value))
}
