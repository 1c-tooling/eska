//! Current option composition keeps functional storage and content file-scoped.

use super::{metadata, RootDatabaseImpl};
use bsl_metadata::{FunctionalOption, ObjectBelonging};
use std::sync::Arc;
use vfs::FileId;

impl RootDatabaseImpl {
    /// Resolve one current name without parsing unrelated option declarations or whole configurations.
    pub fn resolve_functional_option_for_file(
        &self,
        file: FileId,
        name: &str,
    ) -> Option<Arc<FunctionalOption>> {
        self.functional_option_for_file(file, name).ok().flatten()
    }

    /// Enumerate only the registered visibility chain; hidden extension names never enter the namespace.
    pub fn functional_option_names_for_file(&self, file: FileId) -> Vec<String> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return Vec::new();
        };
        let mut result: Vec<String> = Vec::new();
        for listing in base.into_iter().chain(chain.into_iter().flatten()) {
            for entry in listing.functional_options(self).iter() {
                if !result.iter().any(|name| stdx::case::eq_ignore_case(name, &entry.name)) {
                    result.push(entry.name.clone());
                }
            }
        }
        result
    }

    /// Preflight shares the exact named resolver used by the native global call rather than composing a second model.
    ///
    /// # Errors
    /// Returns the required current declaration whose payload or adoption cannot be established.
    pub fn functional_options_for_file(
        &self,
        file: FileId,
    ) -> Result<Vec<Arc<FunctionalOption>>, FileId> {
        self.functional_option_names_for_file(file)
            .into_iter()
            .filter_map(|name| self.functional_option_for_file(file, &name).transpose())
            .collect()
    }

    /// Missing names remain distinct from registered unavailable contributions and incompatible borrowed storage.
    ///
    /// # Errors
    /// Returns the current contribution whose declaration or exact original storage cannot be proven.
    pub fn functional_option_for_file(
        &self,
        file: FileId,
        name: &str,
    ) -> Result<Option<Arc<FunctionalOption>>, FileId> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return Ok(None);
        };
        let mut current: Option<Arc<FunctionalOption>> = None;
        for listing in base.into_iter().chain(chain.into_iter().flatten()) {
            let Some(entry) = listing
                .functional_options(self)
                .iter()
                .find(|entry| stdx::case::eq_ignore_case(&entry.name, name))
            else {
                continue;
            };
            let value = metadata::resolve_functional_option(self, listing, name.into())
                .ok_or(entry.main)?;
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
}
