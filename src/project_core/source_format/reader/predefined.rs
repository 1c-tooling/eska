//! Preserved predefined data belongs only to a declared catalog or chart descriptor.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use super::{Error, FileRole, MetadataKind, OwnerSpec, SourceManifest, SourcePath};

/// Structural registration reads no owner YAML, item names or resource bytes.
pub(super) fn declared_predefined(
    manifest: &SourceManifest,
    owners: &mut BTreeMap<SourcePath, OwnerSpec>,
) -> Result<BTreeMap<SourcePath, SourcePath>, Error> {
    let locations = owners
        .iter()
        .filter(|(_, spec)| supports_predefined(spec.kind))
        .map(|(owner, spec)| {
            let prefix = spec.main.as_str().strip_suffix(".xml").ok_or_else(|| {
                Error::root("source_identity_mismatch").in_field(spec.main.as_str())
            })?;
            Ok((
                format!("{prefix}/Ext/Predefined.xml").to_ascii_lowercase(),
                owner.clone(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let mut resources = BTreeMap::new();
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
        spec.predefined = Some(file.path.clone());
        resources.insert(file.path.clone(), owner.clone());
    }
    Ok(resources)
}

/// Only these Designer families store predefined items; inline enum values have a different source.
const fn supports_predefined(kind: MetadataKind) -> bool {
    matches!(
        kind,
        MetadataKind::Catalog
            | MetadataKind::ChartOfAccounts
            | MetadataKind::ChartOfCalculationTypes
            | MetadataKind::ChartOfCharacteristicTypes
    )
}
