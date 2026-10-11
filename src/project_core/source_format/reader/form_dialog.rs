//! Preserved managed dialogs bind only to declared main common forms, never to stray files.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use super::{Error, FileRole, MetadataKind, OwnerSpec, SourceManifest, SourcePath};

/// Register exact declared resource paths without loading YAML or interpreting dialog bytes.
pub(super) fn declared_form_dialogs(
    manifest: &SourceManifest,
    owners: &mut BTreeMap<SourcePath, OwnerSpec>,
) -> Result<BTreeMap<SourcePath, SourcePath>, Error> {
    let locations = owners
        .iter()
        .filter(|(_, spec)| spec.kind == MetadataKind::CommonForm)
        .map(|(owner, spec)| {
            let prefix = spec.main.as_str().strip_suffix(".xml").ok_or_else(|| {
                Error::root("source_identity_mismatch").in_field(spec.main.as_str())
            })?;
            Ok((
                format!("{prefix}/Ext/Form.xml").to_ascii_lowercase(),
                owner.clone(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let mut dialogs = BTreeMap::new();
    for file in &manifest.files {
        if file.role != FileRole::Preserved {
            continue;
        }
        let Some(owner) = locations.get(&file.path.as_str().to_ascii_lowercase()) else {
            continue;
        };
        let spec = owners
            .get_mut(owner)
            .ok_or_else(|| Error::root("source_descriptor_undeclared").in_field(owner.as_str()))?;
        spec.dialog = Some(file.path.clone());
        dialogs.insert(file.path.clone(), owner.clone());
    }
    Ok(dialogs)
}
