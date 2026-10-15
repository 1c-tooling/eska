//! Canonical module owners come from current tracked values, never a directory loader.

mod external;
mod families;
mod object;
#[cfg(test)]
mod tests;

pub(crate) use external::is_canonical_external;

use std::{path::Path, sync::Arc};

use bsl_metadata::ModuleType;
use vfs::FileId;

use crate::{metadata, RootDatabaseImpl};

impl RootDatabaseImpl {
    /// Claim the file's own canonical root before any native module or configuration reader runs.
    ///
    /// `Some` also carries unavailable metadata: a declared canonical owner cannot be rescued by
    /// another root, an XML sidecar or a previously loaded whole configuration.
    pub(crate) fn canonical_module_metadata(
        &self,
        file: FileId,
        path: &Path,
    ) -> Option<Arc<hir::ModuleMetadata>> {
        let roots = self.workspace_configs_snapshot();
        let own = roots.owner_of_path(path)?;
        let root = &roots.paths[own].1;
        let listing = self.metadata_listing(root.to_str()?)?;
        if listing.canonical_metadata(self).is_empty() {
            return None;
        }
        let external_object = roots.is_external(own)
            && path
                .strip_prefix(root)
                .or_else(|_| path.strip_prefix(&roots.canonical_paths[own]))
                .ok()
                .and_then(object::external_name)
                .is_some();
        let module_type = if external_object {
            ModuleType::ObjectModule
        } else {
            metadata::get_module_type_from_uri(&path.to_string_lossy())
                .unwrap_or(ModuleType::Unknown)
        };
        if module_type == ModuleType::FormModule {
            return self.canonical_form_module_metadata(file, path);
        }
        let mut module = hir::ModuleMetadata::unknown(module_type);
        match module_type {
            ModuleType::ManagerModule
            | ModuleType::ObjectModule
            | ModuleType::RecordSetModule
            | ModuleType::ValueManagerModule => {
                object::publish(self, file, path, listing, &roots, &mut module)
            }
            ModuleType::CommonModule => {
                module.common_module = families::common(self, file, listing);
                module.execution_context = module
                    .common_module
                    .as_ref()
                    .map(|owner| hir::compute_execution_context(owner));
            }
            ModuleType::CommandModule => {
                module.common_command = self.common_command_for_file(file).ok().flatten();
            }
            ModuleType::HTTPServiceModule => {
                module.http_service = families::http(self, file, listing);
            }
            ModuleType::WebServiceModule => {
                module.web_service = families::web(self, file, listing);
            }
            ModuleType::IntegrationServiceModule => {
                module.integration_service = families::integration(self, file, listing);
            }
            _ => {}
        }
        if module_type == ModuleType::ManagerModule {
            module.settings_storage = self.settings_storage_module_for_file(file).ok().flatten();
        }
        if module_type == ModuleType::RecordSetModule {
            module.sequence = self.sequence_module_for_file(file).ok().flatten();
        }
        Some(Arc::new(module))
    }

    /// A canonical visibility chain must be fully enrolled before a typed merge may run.
    ///
    /// Listings are enrolled before querying and composition changes reopen the database; the
    /// registry's absence is not itself a mutable tracked provider.
    fn canonical_visible_listings(
        &self,
        file: FileId,
    ) -> Option<Vec<metadata::MetadataListingInput>> {
        let roots = self.visible_roots_for_file(file)?;
        roots
            .main
            .iter()
            .chain(roots.chain.iter().map(|(_, root)| root))
            .map(|root| {
                let listing = self.metadata_listing(root.to_str()?)?;
                (!listing.canonical_metadata(self).is_empty()).then_some(listing)
            })
            .collect()
    }

    /// Compose the exact declared command module, rejecting unavailable or unproven contributions.
    ///
    /// # Errors
    /// Returns the current declaration's FileId when a required value or adopted UUID is unavailable.
    pub fn common_command_for_file(
        &self,
        file: FileId,
    ) -> Result<Option<Arc<bsl_metadata::CommonCommand>>, FileId> {
        let Some(listings) = self.canonical_visible_listings(file) else {
            return Ok(None);
        };
        let mut own = listings
            .iter()
            .flat_map(|listing| listing.common_commands(self).iter())
            .filter(|entry| entry.module_file == Some(file));
        let Some(entry) = own.next() else {
            return Ok(None);
        };
        if own.next().is_some() {
            return Err(entry.main);
        }
        let name = &entry.name;
        let mut merged: Option<Arc<bsl_metadata::CommonCommand>> = None;
        for listing in listings.iter().copied() {
            let Some(current) = listing
                .common_commands(self)
                .iter()
                .find(|entry| stdx::case::eq_ignore_case(&entry.name, name))
            else {
                continue;
            };
            let value = metadata::resolve_common_command(self, listing, name.clone())
                .ok_or(current.main)?;
            match &mut merged {
                Some(base) if value.adopts(base) => {
                    Arc::make_mut(base).apply_extension_overlay(&value)
                }
                _ if value.object_belonging() == bsl_metadata::ObjectBelonging::Adopted => {
                    return Err(current.main)
                }
                _ => merged = Some(value),
            }
        }
        Ok(merged)
    }
}

/// An unprovided descriptor in an otherwise canonical root must not invoke its XML parser.
fn has_slot(
    database: &RootDatabaseImpl,
    listing: metadata::MetadataListingInput,
    main: FileId,
) -> bool {
    listing.canonical_metadata(database).contains_key(&main)
}
