//! File visibility and adoption identity apply before a configured session value becomes typed.

use super::{metadata, RootDatabaseImpl};
use std::sync::Arc;
use stdx::case::CaseExt;
use vfs::FileId;

impl RootDatabaseImpl {
    /// Fold only registered base/dependency/own slots; no whole-configuration fallback is allowed.
    pub fn resolve_session_parameter_for_file(
        &self,
        file: FileId,
        name: &str,
    ) -> Option<Arc<bsl_metadata::SessionParameter>> {
        let (base, chain, ready) = self.metadata_listings_for_file(file)?;
        if !ready {
            return None;
        }
        let mut current: Option<Arc<bsl_metadata::SessionParameter>> = None;
        for listing in base.into_iter().chain(chain.into_iter().flatten()) {
            if !metadata::session_parameter_index(self, listing).contains_name(name) {
                continue;
            }
            // A present declaration must produce its current value even if a previous root was ready.
            let value = metadata::resolve_session_parameter(self, listing, name.into())?;
            match current.as_ref() {
                Some(base) if !value.adopts(base) => return None,
                None if value.object_belonging() == bsl_metadata::ObjectBelonging::Adopted => {
                    return None
                }
                _ => current = Some(value),
            }
        }
        current
    }

    /// Names are declared structure, so listing them never parses values or unrelated descriptors.
    pub fn session_parameter_names_for_file(&self, file: FileId) -> Vec<String> {
        let Some((base, chain, true)) = self.metadata_listings_for_file(file) else {
            return Vec::new();
        };
        let mut seen = std::collections::HashSet::new();
        let mut names = Vec::new();
        for listing in base.into_iter().chain(chain.into_iter().flatten()) {
            for value in listing.session_parameters(self).iter() {
                if seen.insert(value.name.fold_lower()) {
                    names.push(value.name.clone());
                }
            }
        }
        names
    }
}
