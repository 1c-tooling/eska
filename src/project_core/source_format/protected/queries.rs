//! Plain values use accepted resource ownership and never recover a missing permitted file silently.

use std::path::Path;

#[cfg(test)]
mod tests;

use super::{Origin, merge::SelectedSources, read_working};
use crate::project_core::source_format::{
    AcceptedBase, Error, Operation, SourcePath,
    export::queries::{self, QueryInputs},
    files::Inventory,
};

/// A permitted value is required even when missing-file restoration would apply to other payloads.
pub(super) fn require_values(working: &Inventory, base: &AcceptedBase) -> Result<(), Error> {
    for resource in &base.package.manifest.query_resources {
        let document = base
            .package
            .documents
            .get(&resource.descriptor)
            .ok_or_else(|| Error::root("source_baseline_corrupt"))?;
        if !base
            .permission(&document.descriptor.object.uuid, Operation::Resource)
            .allowed
        {
            continue;
        }
        for path in &resource.sources {
            if !working.files.contains_key(path) {
                return Err(Error::root("source_query_value_missing").in_field(path.as_str()));
            }
        }
    }
    Ok(())
}

/// All selected resource links and values are captured from their independently chosen authorities.
pub(super) fn load(
    root: &Path,
    working: &Inventory,
    base: &AcceptedBase,
    selected: &SelectedSources,
    origins: &std::collections::BTreeMap<SourcePath, Origin>,
) -> Result<QueryInputs, Error> {
    queries::load(
        &base.package.manifest,
        &selected.forms,
        &selected.documents,
        &mut |path| match origins.get(path) {
            Some(Origin::Working) => read_working(root, working, path),
            Some(Origin::Accepted) => base.read_file(path),
            None => Err(Error::root("source_file_undeclared").in_field(path.as_str())),
        },
    )
}
