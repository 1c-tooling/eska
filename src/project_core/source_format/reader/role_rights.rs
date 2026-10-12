//! Only preserved manifest entries attached to declared role descriptors become rights inputs.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use super::{Error, MetadataKind, OwnerSpec, SourceManifest, SourcePath};

/// Register forward and reverse identities once; neither role YAML nor resources are read here.
pub(super) fn declared_role_rights(
    manifest: &SourceManifest,
    owners: &mut BTreeMap<SourcePath, OwnerSpec>,
) -> Result<BTreeMap<SourcePath, SourcePath>, Error> {
    let locations = owners
        .iter()
        .filter(|(_, spec)| spec.kind == MetadataKind::Role)
        .map(|(owner, spec)| {
            // Validated Designer main identities use `.xml`; a resource never supplies that identity.
            let prefix = spec.main.as_str().strip_suffix(".xml").ok_or_else(|| {
                Error::root("source_identity_mismatch").in_field(spec.main.as_str())
            })?;
            Ok((
                format!("{prefix}/Ext/Rights.xml").to_ascii_lowercase(),
                owner.clone(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let mut rights = BTreeMap::new();
    for file in &manifest.files {
        if !file.is_payload() {
            continue;
        }
        let Some(owner) = locations.get(&file.path.as_str().to_ascii_lowercase()) else {
            continue;
        };
        let spec = owners
            .get_mut(owner)
            .ok_or_else(|| Error::root("source_descriptor_undeclared").in_field(owner.as_str()))?;
        spec.rights = Some(file.source_path().clone());
        rights.insert(file.source_path().clone(), owner.clone());
    }
    Ok(rights)
}
