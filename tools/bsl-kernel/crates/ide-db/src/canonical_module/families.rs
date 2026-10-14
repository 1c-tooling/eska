//! Body backlinks claim the current own-root family before any name-based composition.

use std::sync::Arc;

use vfs::FileId;

use crate::{metadata, RootDatabaseImpl};

use super::has_slot;

/// A common module's own slot and every same-name visible contribution remain authoritative.
pub(super) fn common(
    database: &RootDatabaseImpl,
    file: FileId,
    own: metadata::MetadataListingInput,
) -> Option<Arc<bsl_metadata::CommonModule>> {
    let index = metadata::common_module_index(database, own);
    let name = index.name_for_module_file(file)?;
    if !has_slot(database, own, index.lookup(name)?) {
        return None;
    }
    metadata::resolve_common_module(database, own, name.to_owned())?;
    for listing in database.canonical_visible_listings(file)? {
        if let Some(main) = metadata::common_module_index(database, listing).lookup(name) {
            if !has_slot(database, listing, main) {
                return None;
            }
            metadata::resolve_common_module(database, listing, name.to_owned())?;
        }
    }
    database.resolve_common_module_for_file(file, name)
}

/// An HTTP module receives only its own declared service, with current channels/handlers.
pub(super) fn http(
    database: &RootDatabaseImpl,
    file: FileId,
    listing: metadata::MetadataListingInput,
) -> Option<Arc<bsl_metadata::HTTPService>> {
    let index = metadata::http_service_index(database, listing);
    let name = index.name_for_module_file(file)?;
    if !has_slot(database, listing, index.lookup(name)?) {
        return None;
    }
    metadata::resolve_http_service(database, listing, name.to_owned())
}

/// A Web-service body cannot acquire an unrelated visible service through a missing owner slot.
pub(super) fn web(
    database: &RootDatabaseImpl,
    file: FileId,
    listing: metadata::MetadataListingInput,
) -> Option<Arc<bsl_metadata::WebService>> {
    let index = metadata::web_service_index(database, listing);
    let name = index.name_for_module_file(file)?;
    if !has_slot(database, listing, index.lookup(name)?) {
        return None;
    }
    metadata::resolve_web_service(database, listing, name.to_owned())
}

/// Integration-service metadata stays attached to the exact registered module backlink.
pub(super) fn integration(
    database: &RootDatabaseImpl,
    file: FileId,
    listing: metadata::MetadataListingInput,
) -> Option<Arc<bsl_metadata::IntegrationService>> {
    let index = metadata::integration_service_index(database, listing);
    let name = index.name_for_module_file(file)?;
    if !has_slot(database, listing, index.lookup(name)?) {
        return None;
    }
    metadata::resolve_integration_service(database, listing, name.to_owned())
}
