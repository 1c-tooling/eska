//! Session values have their own per-declaration inputs, outside MDO manager identity.

use super::{CanonicalMetadata, MetadataListingInput};
use bsl_metadata::SessionParameter;
use intern::NormName;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// A declared configured value and its authoring file, independent of payload readiness.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SessionParameterEntry {
    pub name: String,
    pub main: vfs::FileId,
}

/// The declared name index depends on structure, not on unrelated payload revisions.
#[derive(Default, PartialEq, Eq, Debug)]
pub struct SessionParameterIndex {
    by_name: FxHashMap<NormName, Option<vfs::FileId>>,
}

impl SessionParameterIndex {
    /// A conflicting declaration is present even when it cannot identify one source value.
    pub fn contains_name(&self, name: &str) -> bool {
        self.by_name.contains_key(&NormName::intern(name))
    }

    /// Keep pending declarations distinguishable from absent names.
    pub fn lookup(&self, name: &str) -> Option<vfs::FileId> {
        self.by_name.get(&NormName::intern(name)).copied().flatten()
    }
}

/// Account for the name-index allocation once at its owning Salsa memo.
fn index_heap(value: &Arc<SessionParameterIndex>) -> usize {
    stdx::heap::map_table_bytes::<NormName, Option<vfs::FileId>>(value.by_name.len())
}

/// Build a root's configured-value index without reading current values.
#[salsa::tracked(heap_size = index_heap, returns(ref))]
pub fn session_parameter_index(
    db: &dyn base_db::SourceDatabase,
    listing: MetadataListingInput,
) -> Arc<SessionParameterIndex> {
    let mut by_name = FxHashMap::default();
    for entry in listing.session_parameters(db).iter() {
        by_name
            .entry(NormName::intern(&entry.name))
            .and_modify(|value| *value = None)
            .or_insert(Some(entry.main));
    }
    Arc::new(SessionParameterIndex { by_name })
}

/// Parsed native XML owns its payload; canonical values are counted at their input.
fn parsed_heap(value: &Option<Arc<SessionParameter>>) -> usize {
    value.as_ref().map_or(0, |value| value.estimated_heap_size())
}

/// Only a native XML path lacking a canonical registration can reach this parser.
#[salsa::interned(debug, heap_size = stdx::heap::zero)]
struct SessionParameterFile<'db> {
    #[returns(copy)]
    main: vfs::FileId,
}

/// Parse the exact versioned VFS input, independently of all other descriptors.
#[salsa::tracked(lru = 8192, heap_size = parsed_heap, returns(clone))]
fn parse_session_parameter(
    db: &dyn base_db::SourceDatabase,
    file: SessionParameterFile<'_>,
) -> Option<Arc<SessionParameter>> {
    bsl_metadata::xml_parser::parse_session_parameter_xml(db.file_text_ref(file.main(db)))
        .ok()
        .map(Arc::new)
}

/// Resolve one current root contribution. Registered None or wrong kinds never read native XML.
#[salsa::tracked(heap_size = stdx::heap::zero, returns(clone))]
pub fn resolve_session_parameter(
    db: &dyn base_db::SourceDatabase,
    listing: MetadataListingInput,
    name: String,
) -> Option<Arc<SessionParameter>> {
    let main = session_parameter_index(db, listing).lookup(&name)?;
    let value = if let Some(input) = listing.canonical_metadata(db).get(&main) {
        let _revision = input.source_revision(db);
        let value = input.value(db).as_ref()?;
        let CanonicalMetadata::SessionParameter(value) = value.as_ref() else {
            return None;
        };
        Arc::clone(value)
    } else {
        if !listing.canonical_metadata(db).is_empty() {
            return None;
        }
        parse_session_parameter(db, SessionParameterFile::new(db, main))?
    };
    stdx::case::eq_ignore_case(value.name(), &name).then_some(value)
}
