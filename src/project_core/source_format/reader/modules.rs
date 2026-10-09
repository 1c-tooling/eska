//! A preserved file is active BSL only at a module location of a declared descriptor.

use std::collections::BTreeMap;

use crate::project_core::native;

use super::{Error, FileRole, OwnerSpec, SourceManifest, SourcePath};

/// Module identity is available even while its body or enclosing YAML is unreadable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeclaredModule<'a> {
    pub owner: &'a SourcePath,
    pub native_descriptor: &'a SourcePath,
    pub binary: bool,
}

pub(super) struct ModuleSpec {
    pub owner: SourcePath,
    pub native_descriptor: SourcePath,
    pub binary: bool,
}

/// Compose identities once from the verified map, never from an unrelated directory walk.
pub(super) fn declared_modules(
    manifest: &SourceManifest,
    owners: &BTreeMap<SourcePath, OwnerSpec>,
) -> Result<BTreeMap<SourcePath, ModuleSpec>, Error> {
    let descriptors = owners
        .iter()
        .flat_map(|(owner, spec)| {
            spec.descriptors
                .keys()
                .map(move |path| (path.clone(), owner.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let mut result = BTreeMap::new();
    for file in &manifest.files {
        if file.role != FileRole::Preserved || !native::is_module_path(file.path.as_path()) {
            continue;
        }
        let Some(descriptor) = descriptor_for(&file.path, &descriptors)? else {
            continue;
        };
        let Some(owner) = descriptors.get(&descriptor) else {
            continue;
        };
        result.insert(
            file.path.clone(),
            ModuleSpec {
                owner: owner.clone(),
                native_descriptor: descriptor,
                binary: file
                    .path
                    .as_path()
                    .extension()
                    .is_some_and(|extension| extension == "bin"),
            },
        );
    }
    Ok(result)
}

/// Only Ext/<module> and Ext/Form/Module are module bodies; extra arbitrary folders stay resources.
fn descriptor_for(
    path: &SourcePath,
    descriptors: &BTreeMap<SourcePath, SourcePath>,
) -> Result<Option<SourcePath>, Error> {
    let text = path.as_str();
    let Some((prefix, module)) = text.rsplit_once("/Ext/") else {
        if text.starts_with("Ext/") && text.split('/').count() == 2 {
            return Ok(Some(SourcePath::parse("Configuration.xml")?));
        }
        return Ok(None);
    };
    if module.contains('/') && module != "Form/Module.bsl" && module != "Form/Module.bin" {
        return Ok(None);
    }
    let descriptor = SourcePath::parse(&format!("{prefix}.xml"))?;
    Ok(descriptors.contains_key(&descriptor).then_some(descriptor))
}
