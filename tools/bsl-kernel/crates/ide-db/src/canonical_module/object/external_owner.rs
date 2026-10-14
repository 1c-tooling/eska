//! Physical external export stems select a registered descriptor identity, never a logical name.

use base_db::SourceDatabase;
use bsl_metadata::{ExternalObjectKind, MdoType};
use vfs::FileId;

use crate::{metadata, RootDatabaseImpl};

/// The bare body belongs to its exact native descriptor FileId in the same enrolled source root.
pub(super) fn identity(
    database: &RootDatabaseImpl,
    file: FileId,
    listing: metadata::MetadataListingInput,
    roots: &metadata::WorkspaceConfigsSnapshot,
    own: usize,
    kind: ExternalObjectKind,
    stem: &str,
) -> Option<(MdoType, String)> {
    let source_root = database.file_source_root_input(file).source_root_id(database);
    let source_root = database.source_root_input(source_root).root(database);
    let mut entries = listing.entries(database).iter().filter(|entry| {
        if entry.kind != kind.mdo_type() {
            return false;
        }
        let Some(path) = source_root.file_set().path_for_file(&entry.main) else { return false };
        [&roots.paths[own].1, &roots.canonical_paths[own]].into_iter().any(|root| {
            let Ok(relative) = path.as_path().strip_prefix(root) else { return false };
            relative.components().count() == 1
                && relative.file_stem() == Some(std::ffi::OsStr::new(stem))
                && bsl_conventions::has_extension(relative, bsl_conventions::XML_EXTENSION)
        })
    });
    let entry = entries.next()?;
    if entries.next().is_some()
        || metadata::config_index(database, listing).lookup(entry.kind, &entry.name)?.main
            != entry.main
    {
        return None;
    }
    Some((entry.kind, entry.name.clone()))
}
