//! Payload relocation retains native identity and the internal structure of opaque resource sets.

use std::collections::{BTreeMap, BTreeSet};

use super::{DESCRIPTOR_LAYOUT_VERSION, RESOURCE_LAYOUT_VERSION};
use crate::project_core::MetadataKind;

#[cfg(test)]
mod tests;
use crate::project_core::source_format::{Error, FileRole, SourceManifest, SourcePath};

/// Known descriptor roots disambiguate the native `Ext` separator from an object's own name.
struct Locations<'a> {
    descriptors: BTreeMap<&'a str, MetadataKind>,
    root: Option<&'a super::super::ExternalRoot>,
}

impl<'a> Locations<'a> {
    /// Only the verified reverse contexts supply resource roots; directories activate nothing.
    fn new(manifest: &'a SourceManifest) -> Self {
        Self {
            root: manifest.external_root.as_ref(),
            descriptors: manifest
                .descriptor_contexts
                .iter()
                .map(|context| (context.path.as_str(), context.metadata_kind))
                .collect(),
        }
    }

    /// Move a whole resource root, leaving all opaque inner paths and textual references intact.
    fn path(&self, native: &str) -> String {
        if let Some(tail) = native.strip_prefix("Ext/") {
            return tail.into();
        }
        for (index, _) in native.match_indices('/').rev() {
            let Some(tail) = native[index..].strip_prefix("/Ext/") else {
                continue;
            };
            let prefix = &native[..index];
            let kind = self.descriptors.get(format!("{prefix}.xml").as_str());
            let command = prefix
                .rsplit_once("/Commands/")
                .is_some_and(|(owner, name)| {
                    !name.is_empty()
                        && !name.contains('/')
                        && self
                            .descriptors
                            .contains_key(format!("{owner}.xml").as_str())
                });
            if kind.is_none() && !command {
                continue;
            }
            // A form module is independent BSL input; `Form/Items` stays a complete resource set.
            let tail = if matches!(kind, Some(MetadataKind::Form | MetadataKind::CommonForm)) {
                match tail {
                    "Form/Module.bsl" => "Module.bsl",
                    "Form/Module.bin" => "Module.bin",
                    _ => tail,
                }
            } else {
                tail
            };
            return self.authoring(&format!("{prefix}/{tail}"));
        }
        self.authoring(native)
    }

    /// The external root's observed stem is a reverse location, not an authoring directory.
    fn authoring(&self, path: &str) -> String {
        self.root.map_or(path, |root| root.relative(path)).into()
    }
}

/// Record every moved payload explicitly; native input fingerprints never become source hashes.
pub(in crate::project_core::source_format) fn relocate(
    manifest: &mut SourceManifest,
) -> Result<(), Error> {
    let locations = Locations::new(manifest);
    let mappings = manifest
        .files
        .iter()
        .filter(|file| file.is_payload())
        .map(|file| SourcePath::parse(&locations.path(file.path.as_str())))
        .collect::<Result<Vec<_>, Error>>()?;
    let mut mappings = mappings.into_iter();
    for file in &mut manifest.files {
        if file.is_payload() {
            let source = mappings
                .next()
                .ok_or_else(|| Error::root("source_layout_payload_invalid"))?;
            file.role = if source == file.path {
                FileRole::Preserved
            } else {
                FileRole::Relocated { source }
            };
        }
    }
    manifest.schema_version = RESOURCE_LAYOUT_VERSION;
    Ok(())
}

/// Older maps cannot smuggle new paths; v3 accepts exactly the documented relocation rules.
pub(super) fn validate(manifest: &SourceManifest) -> Result<(), Error> {
    let locations = Locations::new(manifest);
    for file in &manifest.files {
        if manifest.schema_version < RESOURCE_LAYOUT_VERSION {
            if matches!(file.role, FileRole::Relocated { .. }) {
                return Err(Error::root("source_layout_payload_invalid"));
            }
        } else if file.is_payload() {
            let expected = locations.path(file.path.as_str());
            if file.source_path().as_str() != expected
                || matches!(&file.role, FileRole::Relocated { source } if source == &file.path)
            {
                return Err(
                    Error::root("source_layout_payload_invalid").in_field(file.path.as_str())
                );
            }
        }
    }
    Ok(())
}

/// Preserve native empty directories in provenance while deriving only authoring directories.
pub(in crate::project_core::source_format) fn source_directories(
    manifest: &SourceManifest,
) -> Result<Vec<SourcePath>, Error> {
    if manifest.schema_version < RESOURCE_LAYOUT_VERSION {
        return Ok(manifest.directories.clone());
    }
    let locations = Locations::new(manifest);
    let mut directories = BTreeSet::new();
    for path in &manifest.directories {
        let path = locations.path(&format!("{}/", path.as_str()));
        if let Some(path) = path.strip_suffix('/')
            && !path.is_empty()
        {
            directories.insert(SourcePath::parse(path)?);
        }
    }
    for file in &manifest.files {
        for (end, _) in file.source_path().as_str().match_indices('/') {
            directories.insert(SourcePath::parse(&file.source_path().as_str()[..end])?);
        }
    }
    for path in manifest
        .query_resources
        .iter()
        .flat_map(|resource| &resource.sources)
    {
        for (end, _) in path.as_str().match_indices('/') {
            directories.insert(SourcePath::parse(&path.as_str()[..end])?);
        }
    }
    // All source directories, including newly introduced YAML parents, have explicit ancestors.
    let ancestors = directories
        .iter()
        .flat_map(|directory| {
            directory
                .as_str()
                .match_indices('/')
                .map(|(end, _)| SourcePath::parse(&directory.as_str()[..end]))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    directories.extend(ancestors);
    Ok(directories.into_iter().collect())
}

/// A versioned target keeps descriptor-only migration available without silently relocating files.
#[derive(Clone, Copy)]
pub(in crate::project_core::source_format) enum Layout {
    Family,
    Descriptors,
    Resources,
    Readable,
    Forms,
    Queries,
}

impl Layout {
    /// Source map versions select an explicit authoring layout, independently of XML envelopes.
    pub(in crate::project_core::source_format) const fn version(self) -> u32 {
        match self {
            Self::Family => crate::project_core::source_format::SCHEMA_VERSION,
            Self::Descriptors => DESCRIPTOR_LAYOUT_VERSION,
            Self::Resources | Self::Readable => RESOURCE_LAYOUT_VERSION,
            Self::Forms => super::MANAGED_FORM_LAYOUT_VERSION,
            Self::Queries => super::QUERY_LAYOUT_VERSION,
        }
    }
}
