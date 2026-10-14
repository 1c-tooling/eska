//! The body path selects its own declared owner before dependency overlays are composed.

use std::path::Path;

use bsl_metadata::ModuleType;
use vfs::FileId;

use crate::{metadata, RootDatabaseImpl};

use super::has_slot;

/// External roots use their known kind and bare export layout, regardless of ancestor directory names.
pub(super) fn publish(
    database: &RootDatabaseImpl,
    file: FileId,
    path: &Path,
    listing: metadata::MetadataListingInput,
    roots: &metadata::WorkspaceConfigsSnapshot,
    module: &mut hir::ModuleMetadata,
) {
    let Some(own) = roots.owner_of_path(path) else { return };
    let relative = path
        .strip_prefix(&roots.paths[own].1)
        .or_else(|_| path.strip_prefix(&roots.canonical_paths[own]));
    let Ok(relative) = relative else { return };
    let identity = match roots.kinds[own] {
        metadata::RootKind::External(kind) => {
            if module.module_type != ModuleType::ObjectModule {
                return;
            }
            let Some(name) = external_name(relative) else { return };
            (kind.mdo_type(), name.to_owned())
        }
        metadata::RootKind::Base | metadata::RootKind::Extension => {
            let Some(info) = metadata::parse_module_path(&relative.to_string_lossy()) else {
                return;
            };
            let (Some(kind), Some(name)) = (info.mdo_type, info.name) else { return };
            (kind, name)
        }
    };
    let (kind, name) = identity;
    let Some(ids) = metadata::config_index(database, listing).lookup(kind, &name) else { return };
    if !has_slot(database, listing, ids.main) {
        return;
    }
    if kind.is_register() {
        module.register = register(database, file, listing, kind, &name);
    } else {
        module.mdo = object(database, file, listing, kind, &name);
    }
}

/// Only the bare `<Name>/Ext/ObjectModule.bsl` shape owns an external object module.
pub(super) fn external_name(relative: &Path) -> Option<&str> {
    use bsl_conventions::{conventional_of, ConventionalName};
    let mut components = relative.components();
    let name = components.next()?.as_os_str().to_str()?;
    let service = components.next()?.as_os_str().to_str()?;
    let body = components.next()?.as_os_str().to_str()?;
    (components.next().is_none()
        && conventional_of(service) == Some(ConventionalName::Ext)
        && conventional_of(body) == Some(ConventionalName::ObjectModule))
    .then_some(name)
}

/// Own and visible declarations must all resolve from canonical slots before native overlay folding.
fn object(
    database: &RootDatabaseImpl,
    file: FileId,
    own: metadata::MetadataListingInput,
    kind: bsl_metadata::MdoType,
    name: &str,
) -> Option<std::sync::Arc<bsl_metadata::MetadataObject>> {
    metadata::resolve_metadata_object(database, own, kind, name.to_owned())?;
    for listing in database.canonical_visible_listings(file)? {
        if let Some(ids) = metadata::config_index(database, listing).lookup(kind, name) {
            if !has_slot(database, listing, ids.main) {
                return None;
            }
            metadata::resolve_metadata_object(database, listing, kind, name.to_owned())?;
        }
    }
    database.resolve_metadata_object_for_file(file, kind, name)
}

/// Register owners retain dimensions/resources and use the same checked dependency composition.
fn register(
    database: &RootDatabaseImpl,
    file: FileId,
    own: metadata::MetadataListingInput,
    kind: bsl_metadata::MdoType,
    name: &str,
) -> Option<std::sync::Arc<bsl_metadata::Register>> {
    metadata::resolve_register(database, own, kind, name.to_owned())?;
    for listing in database.canonical_visible_listings(file)? {
        if let Some(ids) = metadata::config_index(database, listing).lookup(kind, name) {
            if !has_slot(database, listing, ids.main) {
                return None;
            }
            metadata::resolve_register(database, listing, kind, name.to_owned())?;
        }
    }
    database.resolve_register_for_file(file, kind, name)
}
