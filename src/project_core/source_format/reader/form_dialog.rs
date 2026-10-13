//! Form descriptors retain full native identities and their separately declared dialog resources.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use super::{Error, MetadataKind, NativeUuid, OwnerSpec, SourceManifest, SourcePath};

/// A form's full descriptor path disambiguates equal names belonging to different objects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeclaredForm<'a> {
    pub owner: &'a SourcePath,
    pub native_descriptor: &'a SourcePath,
    pub kind: MetadataKind,
    pub name: &'a str,
    pub uuid: &'a NativeUuid,
    /// Absence is structural; a declared but missing file retains its path here.
    pub dialog: Option<&'a SourcePath>,
}

pub(super) struct FormSpec {
    pub owner: SourcePath,
    pub kind: MetadataKind,
    pub name: String,
    pub uuid: NativeUuid,
    pub dialog: Option<SourcePath>,
}

/// Register all descriptor identities without reading owner YAML or interpreting dialog bytes.
pub(super) fn declared_forms(
    owners: &BTreeMap<SourcePath, OwnerSpec>,
) -> Result<BTreeMap<SourcePath, FormSpec>, Error> {
    let mut forms = BTreeMap::new();
    for (owner, spec) in owners {
        for (path, uuid) in &spec.descriptors {
            let kind = if path == &spec.main && spec.kind == MetadataKind::CommonForm {
                MetadataKind::CommonForm
            } else if spec.kind == MetadataKind::Form && path == &spec.main
                || path != &spec.main
                    && path
                        .as_str()
                        .rsplit_once("/Forms/")
                        .is_some_and(|(_, name)| !name.contains('/'))
            {
                MetadataKind::Form
            } else {
                continue;
            };
            let name = path
                .as_str()
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".xml"))
                .filter(|name| !name.is_empty())
                .ok_or_else(|| Error::root("source_identity_mismatch").in_field(path.as_str()))?;
            forms.insert(
                path.clone(),
                FormSpec {
                    owner: owner.clone(),
                    kind,
                    name: name.to_owned(),
                    uuid: uuid.clone(),
                    dialog: None,
                },
            );
        }
    }
    Ok(forms)
}

/// Attach exact preserved resources to descriptor paths, never to a short name or stray folder.
pub(super) fn declared_form_dialogs(
    manifest: &SourceManifest,
    forms: &mut BTreeMap<SourcePath, FormSpec>,
) -> Result<BTreeMap<SourcePath, SourcePath>, Error> {
    let locations = forms
        .keys()
        .map(|path| {
            let prefix = path
                .as_str()
                .strip_suffix(".xml")
                .ok_or_else(|| Error::root("source_identity_mismatch").in_field(path.as_str()))?;
            Ok((
                format!("{prefix}/Ext/Form.xml").to_ascii_lowercase(),
                path.clone(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let mut dialogs = BTreeMap::new();
    for file in &manifest.files {
        if !file.is_payload() && !matches!(file.role, super::FileRole::ManagedForm { .. }) {
            continue;
        }
        let Some(descriptor) = locations.get(&file.path.as_str().to_ascii_lowercase()) else {
            continue;
        };
        let spec = forms.get_mut(descriptor).ok_or_else(|| {
            Error::root("source_descriptor_undeclared").in_field(descriptor.as_str())
        })?;
        spec.dialog = Some(file.source_path().clone());
        dialogs.insert(file.source_path().clone(), descriptor.clone());
    }
    Ok(dialogs)
}
