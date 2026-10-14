//! Canonical external roots contribute typed owners, never a fabricated whole Configuration.

use std::path::Path;

use crate::{RootDatabase, RootDatabaseImpl};

/// Whole-configuration consumers must skip a known canonical external root, including pending values.
pub(crate) fn is_canonical_external(database: &dyn RootDatabase, path: &Path) -> bool {
    let Some(database) = database.as_any().downcast_ref::<RootDatabaseImpl>() else {
        return false;
    };
    let roots = database.workspace_configs_snapshot();
    let Some(own) = roots.owner_of_path(path) else { return false };
    if !roots.is_external(own) {
        return false;
    }
    let root = &roots.paths[own].1;
    database
        .metadata_listing(&root.to_string_lossy())
        .is_some_and(|listing| !listing.canonical_metadata(database).is_empty())
}
