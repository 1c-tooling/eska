//! Sequence declarations and their record dimensions compose through the caller's explicit roots.

use super::{metadata, RootDatabaseImpl};
use bsl_metadata::{ObjectBelonging, Sequence};
use std::sync::Arc;
use vfs::FileId;

impl RootDatabaseImpl {
    /// Resolve one current name without parsing unrelated sequence declarations or whole configurations.
    pub fn resolve_sequence_for_file(&self, file: FileId, name: &str) -> Option<Arc<Sequence>> {
        self.checked_sequence(file, name).ok().flatten()
    }

    /// Enumerate only the registered visibility chain; hidden extension names never enter the namespace.
    pub fn sequence_names_for_file(&self, file: FileId) -> Vec<String> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return Vec::new();
        };
        let mut result: Vec<String> = Vec::new();
        for listing in base.into_iter().chain(chain.into_iter().flatten()) {
            for entry in listing.sequences(self).iter() {
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
    pub fn sequences_for_file(&self, file: FileId) -> Result<Vec<Arc<Sequence>>, FileId> {
        self.sequence_names_for_file(file)
            .into_iter()
            .filter_map(|name| self.checked_sequence(file, &name).transpose())
            .collect()
    }

    /// Missing names remain distinct from registered unavailable contributions and incompatible borrowed types.
    fn checked_sequence(&self, file: FileId, name: &str) -> Result<Option<Arc<Sequence>>, FileId> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return Ok(None);
        };
        let mut current: Option<Arc<Sequence>> = None;
        for listing in base.into_iter().chain(chain.into_iter().flatten()) {
            let Some(entry) = listing
                .sequences(self)
                .iter()
                .find(|entry| stdx::case::eq_ignore_case(&entry.name, name))
            else {
                continue;
            };
            let value = metadata::resolve_sequence(self, listing, name.into()).ok_or(entry.main)?;
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

    /// A record-set module uses its registered backlink, never a same-spelled directory in another root.
    ///
    /// # Errors
    /// Returns the current required owner when its identity, payload or adoption is unavailable.
    pub fn sequence_module_for_file(&self, file: FileId) -> Result<Option<Arc<Sequence>>, FileId> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return Ok(None);
        };
        let listings = base.into_iter().chain(chain.into_iter().flatten()).collect::<Vec<_>>();
        let mut own = listings
            .iter()
            .flat_map(|listing| listing.sequences(self).iter())
            .filter(|entry| entry.module_file == Some(file));
        let Some(entry) = own.next() else {
            return Ok(None);
        };
        if own.next().is_some() {
            return Err(entry.main);
        }
        self.checked_sequence(file, &entry.name)
    }
}
