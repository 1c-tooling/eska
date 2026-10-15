//! SettingsStorage declarations and their manager identities compose through the caller's explicit roots.

use super::{metadata, RootDatabaseImpl};
use base_db::SourceDatabase;
use bsl_metadata::{ObjectBelonging, SettingsStorage};
use std::sync::Arc;
use vfs::FileId;

impl RootDatabaseImpl {
    /// Resolve one current name without parsing unrelated settings_storage declarations or whole configurations.
    pub fn resolve_settings_storage_for_file(
        &self,
        file: FileId,
        name: &str,
    ) -> Option<Arc<SettingsStorage>> {
        self.checked_settings_storage(file, name).ok().flatten()
    }

    /// Enumerate only the registered visibility chain; hidden extension names never enter the namespace.
    pub fn settings_storage_names_for_file(&self, file: FileId) -> Vec<String> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return Vec::new();
        };
        let mut result: Vec<String> = Vec::new();
        for listing in base.into_iter().chain(chain.into_iter().flatten()) {
            for entry in listing.settings_storages(self).iter() {
                if !result.iter().any(|name| stdx::case::eq_ignore_case(name, &entry.name)) {
                    result.push(entry.name.clone());
                }
            }
        }
        result
    }

    /// Preflight shares the exact named resolver used by the configured namespace and module context rather than composing a second model.
    ///
    /// # Errors
    /// Returns the required current declaration whose payload or adoption cannot be established.
    pub fn settings_storages_for_file(
        &self,
        file: FileId,
    ) -> Result<Vec<Arc<SettingsStorage>>, FileId> {
        self.settings_storage_names_for_file(file)
            .into_iter()
            .filter_map(|name| self.checked_settings_storage(file, &name).transpose())
            .collect()
    }

    /// Exact enrolled manager bodies follow current owner composition and preserve unread priority.
    ///
    /// An absent optional module is complete absence; an invalid owner or missing canonical
    /// visibility returns `None` so consumers cannot answer from unrelated path-index entries.
    pub fn settings_storage_manager_module_files_for_file(
        &self,
        file: FileId,
        name: &str,
    ) -> Option<hir::CommonModuleBodies> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return None;
        };
        let mut bodies = hir::CommonModuleBodies::default();
        if self.checked_settings_storage(file, name).ok()?.is_none() {
            return Some(bodies);
        }
        for listing in base.into_iter().chain(chain.into_iter().flatten()) {
            if let Some(body) = listing
                .settings_storages(self)
                .iter()
                .find(|entry| stdx::case::eq_ignore_case(&entry.name, name))
                .and_then(|entry| entry.module_file)
            {
                bodies.push(body, self.file_is_unread(body));
            }
        }
        Some(bodies)
    }

    /// Missing names remain distinct from registered unavailable contributions and incompatible borrowed identities.
    fn checked_settings_storage(
        &self,
        file: FileId,
        name: &str,
    ) -> Result<Option<Arc<SettingsStorage>>, FileId> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return Ok(None);
        };
        let mut current: Option<Arc<SettingsStorage>> = None;
        for listing in base.into_iter().chain(chain.into_iter().flatten()) {
            let Some(entry) = listing
                .settings_storages(self)
                .iter()
                .find(|entry| stdx::case::eq_ignore_case(&entry.name, name))
            else {
                continue;
            };
            let value =
                metadata::resolve_settings_storage(self, listing, name.into()).ok_or(entry.main)?;
            match &mut current {
                Some(base) if value.adopts(base) => {
                    Arc::make_mut(base).apply_extension_overlay(&value)
                }
                Some(_) => return Err(entry.main),
                None if value.object_belonging() == ObjectBelonging::Adopted => {
                    return Err(entry.main)
                }
                _ => current = Some(value),
            }
        }
        Ok(current)
    }

    /// A manager module uses its registered backlink, never a same-spelled directory in another root.
    ///
    /// # Errors
    /// Returns the current required owner when its identity, payload or adoption is unavailable.
    pub fn settings_storage_module_for_file(
        &self,
        file: FileId,
    ) -> Result<Option<Arc<SettingsStorage>>, FileId> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return Ok(None);
        };
        let listings = base.into_iter().chain(chain.into_iter().flatten()).collect::<Vec<_>>();
        let mut own = listings
            .iter()
            .flat_map(|listing| listing.settings_storages(self).iter())
            .filter(|entry| entry.module_file == Some(file));
        let Some(entry) = own.next() else {
            return Ok(None);
        };
        if own.next().is_some() {
            return Err(entry.main);
        }
        self.checked_settings_storage(file, &entry.name)
    }
}
