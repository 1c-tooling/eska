//! Current criterion composition keeps namespace membership and Find's configured type file-scoped.

use super::{metadata, RootDatabaseImpl};
use bsl_metadata::{FilterCriterion, ObjectBelonging};
use std::sync::Arc;
use vfs::FileId;

impl RootDatabaseImpl {
    /// Resolve one current name without parsing unrelated criterion declarations or whole configurations.
    pub fn resolve_filter_criterion_for_file(
        &self,
        file: FileId,
        name: &str,
    ) -> Option<Arc<FilterCriterion>> {
        self.checked_filter_criterion(file, name).ok().flatten()
    }

    /// Enumerate only the registered visibility chain; hidden extension names never enter the namespace.
    pub fn filter_criterion_names_for_file(&self, file: FileId) -> Vec<String> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return Vec::new();
        };
        let mut result: Vec<String> = Vec::new();
        for listing in base.into_iter().chain(chain.into_iter().flatten()) {
            for entry in listing.filter_criteria(self).iter() {
                if !result.iter().any(|name| stdx::case::eq_ignore_case(name, &entry.name)) {
                    result.push(entry.name.clone());
                }
            }
        }
        result
    }

    /// Preflight shares the exact named resolver used by fields and Find rather than composing a second model.
    ///
    /// # Errors
    /// Returns the required current declaration whose payload or adoption cannot be established.
    pub fn filter_criteria_for_file(
        &self,
        file: FileId,
    ) -> Result<Vec<Arc<FilterCriterion>>, FileId> {
        self.filter_criterion_names_for_file(file)
            .into_iter()
            .filter_map(|name| self.checked_filter_criterion(file, &name).transpose())
            .collect()
    }

    /// Missing names remain distinct from registered unavailable contributions and incompatible borrowed types.
    fn checked_filter_criterion(
        &self,
        file: FileId,
        name: &str,
    ) -> Result<Option<Arc<FilterCriterion>>, FileId> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return Ok(None);
        };
        let mut current: Option<Arc<FilterCriterion>> = None;
        for listing in base.into_iter().chain(chain.into_iter().flatten()) {
            let Some(entry) = listing
                .filter_criteria(self)
                .iter()
                .find(|entry| stdx::case::eq_ignore_case(&entry.name, name))
            else {
                continue;
            };
            let value =
                metadata::resolve_filter_criterion(self, listing, name.into()).ok_or(entry.main)?;
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
