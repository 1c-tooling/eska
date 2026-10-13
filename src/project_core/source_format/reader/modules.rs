//! Preserved modules bind to declared descriptors, including an inline command's enclosing owner.

use std::collections::BTreeMap;

use crate::project_core::native;

use super::{Error, OwnerSpec, SourceManifest, SourcePath};

/// Module identity is available even while its body or enclosing YAML is unreadable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeclaredModule<'a> {
    pub owner: &'a SourcePath,
    pub native_descriptor: &'a SourcePath,
    pub native_path: &'a SourcePath,
    pub binary: bool,
}

pub(super) struct ModuleSpec {
    pub owner: SourcePath,
    pub native_descriptor: SourcePath,
    pub native_path: SourcePath,
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
    let root = manifest.root_path()?;
    for file in &manifest.files {
        if !file.is_payload() || !native::is_module_path(file.path.as_path()) {
            continue;
        }
        let Some(descriptor) = descriptor_for(&file.path, &descriptors, &root)? else {
            continue;
        };
        let Some(owner) = descriptors.get(&descriptor) else {
            continue;
        };
        result.insert(
            file.source_path().clone(),
            ModuleSpec {
                owner: owner.clone(),
                native_descriptor: descriptor,
                native_path: file.path.clone(),
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

/// Recognize Designer body locations without reading YAML or activating arbitrary resource folders.
fn descriptor_for(
    path: &SourcePath,
    descriptors: &BTreeMap<SourcePath, SourcePath>,
    root: &SourcePath,
) -> Result<Option<SourcePath>, Error> {
    let text = path.as_str();
    let Some((prefix, module)) = text.rsplit_once("/Ext/") else {
        if text.starts_with("Ext/") && text.split('/').count() == 2 {
            return Ok(Some(root.clone()));
        }
        return Ok(None);
    };
    if module.contains('/') && module != "Form/Module.bsl" && module != "Form/Module.bin" {
        return Ok(None);
    }
    let descriptor = SourcePath::parse(&format!("{prefix}.xml"))?;
    if descriptors.contains_key(&descriptor) {
        return Ok(Some(descriptor));
    }
    // Object commands are inline in their enclosing descriptor, unlike separately saved forms.
    if matches!(module, "CommandModule.bsl" | "CommandModule.bin")
        && let Some((owner, command)) = prefix.rsplit_once("/Commands/")
        && !command.is_empty()
        && !command.contains('/')
    {
        let descriptor = SourcePath::parse(&format!("{owner}.xml"))?;
        return Ok(descriptors.contains_key(&descriptor).then_some(descriptor));
    }
    Ok(None)
}
