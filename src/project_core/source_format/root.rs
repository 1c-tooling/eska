//! Native root bindings distinguish external objects from configuration composition.

use super::{
    Descriptor, Error, FileEntry, FileRole, NativeDocument, SourceManifest, SourcePath,
    files::Inventory,
};
use crate::project_core::{MetadataKind, ProjectType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

/// External roots use the complete readable form/query layout, never a legacy implicit root.
pub const EXTERNAL_LAYOUT_VERSION: u32 = 6;
/// The native root tag is part of the reversible descriptor profile.
pub const PROCESSING_PROFILE: &str = "designer-2.20-external-data-processor";
/// Reports retain their external tag independently of their logical metadata kind.
pub const REPORT_PROFILE: &str = "designer-2.20-external-report";

/// Only external object profiles may select a non-configuration root in map v6.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExternalKind {
    Processing,
    Report,
}

impl ExternalKind {
    /// Select the shared metadata semantics without changing the native root spelling.
    pub(crate) const fn metadata_kind(self) -> MetadataKind {
        match self {
            Self::Processing => MetadataKind::DataProcessor,
            Self::Report => MetadataKind::Report,
        }
    }
    /// Keep project and artifact dispatch independent of input filenames.
    pub(crate) const fn project_type(self) -> ProjectType {
        match self {
            Self::Processing => ProjectType::Processing,
            Self::Report => ProjectType::Report,
        }
    }
    /// The descriptor profile supplies the exact external XML root tag.
    pub(crate) const fn profile(self) -> &'static str {
        match self {
            Self::Processing => PROCESSING_PROFILE,
            Self::Report => REPORT_PROFILE,
        }
    }
    /// Everyday authoring has one root document beside its modules and resource collections.
    pub(crate) const fn source(self) -> &'static str {
        match self {
            Self::Processing => "processing.yaml",
            Self::Report => "report.yaml",
        }
    }
}

/// The map binds an observed native filename to a profile; UUID and Name stay in the descriptor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalRoot {
    pub path: SourcePath,
    pub kind: ExternalKind,
}

impl ExternalRoot {
    /// External native exports have one immediate XML root, including dotted and Unicode stems.
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.path.as_str().contains('/') || self.path.as_str().strip_suffix(".xml").is_none() {
            return Err(Error::root("source_profile_unsupported"));
        }
        Ok(())
    }
    /// Strip only the verified native root directory from an authoring location.
    pub(super) fn relative<'a>(&self, path: &'a str) -> &'a str {
        let stem = self.path.as_str().strip_suffix(".xml").unwrap_or_default();
        path.strip_prefix(stem)
            .and_then(|tail| tail.strip_prefix('/'))
            .unwrap_or(path)
    }
}

/// Imported absence is provenance, never an empty module generated during conversion.
const ABSENT_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// Declare the sole external object-module slot only when neither readable nor binary body exists.
pub(super) fn declare_optional_module(
    manifest: &mut SourceManifest,
    inventory: &Inventory,
) -> Result<(), Error> {
    let Some(root) = &manifest.external_root else {
        return Ok(());
    };
    let candidates = module_candidates(root)?;
    for candidate in &candidates {
        let binary = SourcePath::parse(
            &candidate
                .as_str()
                .replace("ObjectModule.bsl", "ObjectModule.bin"),
        )?;
        if inventory.files.contains_key(candidate) || inventory.files.contains_key(&binary) {
            return Ok(());
        }
    }
    let existing = candidates
        .iter()
        .filter(|path| {
            path.as_path().parent().is_some_and(|parent| {
                inventory
                    .directories
                    .iter()
                    .any(|directory| directory.as_path() == parent)
            })
        })
        .collect::<Vec<_>>();
    let native = match existing.as_slice() {
        [path] => (*path).clone(),
        [] => candidates[1].clone(),
        _ => return Err(Error::root("source_module_ambiguous")),
    };
    manifest.files.push(FileEntry {
        path: native,
        bytes: 0,
        sha256: ABSENT_SHA256.to_owned(),
        role: FileRole::OptionalModule {
            source: SourcePath::parse("ObjectModule.bsl")?,
        },
    });
    manifest
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    Ok(())
}

/// Optional declarations cannot activate arbitrary resources, internal modules or alternate owners.
pub(super) fn validate_optional_modules(manifest: &SourceManifest) -> Result<(), Error> {
    for file in &manifest.files {
        let FileRole::OptionalModule { source } = &file.role else {
            continue;
        };
        let root = manifest
            .external_root
            .as_ref()
            .ok_or_else(|| Error::root("source_optional_module_invalid"))?;
        if manifest.schema_version != EXTERNAL_LAYOUT_VERSION
            || source.as_str() != "ObjectModule.bsl"
            || !module_candidates(root)?.contains(&file.path)
            || file.bytes != 0
            || file.sha256 != ABSENT_SHA256
        {
            return Err(Error::root("source_optional_module_invalid").in_field(file.path.as_str()));
        }
        for candidate in module_candidates(root)? {
            let binary = candidate
                .as_str()
                .replace("ObjectModule.bsl", "ObjectModule.bin");
            if manifest.files.iter().any(|entry| {
                entry.path.as_str() == binary
                    || (entry.path == candidate && entry.path != file.path)
            }) {
                return Err(Error::root("source_optional_module_invalid"));
            }
        }
    }
    Ok(())
}

/// The native root binding determines exactly two supported export locations.
fn module_candidates(root: &ExternalRoot) -> Result<[SourcePath; 2], Error> {
    let stem = root
        .path
        .as_str()
        .strip_suffix(".xml")
        .ok_or_else(|| Error::root("source_profile_unsupported"))?;
    Ok([
        SourcePath::parse("Ext/ObjectModule.bsl")?,
        SourcePath::parse(&format!("{stem}/Ext/ObjectModule.bsl"))?,
    ])
}

impl SourceManifest {
    /// Only a pinned external declaration may grant creation of the absent module body.
    pub(super) fn optional_module(&self, path: &SourcePath) -> Option<&FileEntry> {
        self.files.iter().find(
            |file| matches!(&file.role, FileRole::OptionalModule { source } if source == path),
        )
    }

    /// Resolve the explicit native root without reading modules or materializing XML.
    ///
    /// # Errors
    /// Returns an invalid external native location.
    pub fn root_path(&self) -> Result<SourcePath, Error> {
        Ok(match &self.external_root {
            Some(root) => {
                root.validate()?;
                root.path.clone()
            }
            None => SourcePath::parse("Configuration.xml")?,
        })
    }
    /// Root ownership follows the source map, never the current native Name value.
    pub(super) fn root_source(&self) -> Result<SourcePath, Error> {
        SourcePath::parse(
            self.external_root
                .as_ref()
                .map_or("configuration.yaml", |root| root.kind.source()),
        )
    }
    /// External and internal metadata share kinds while their root profiles remain distinct.
    pub(super) fn root_kind(&self) -> MetadataKind {
        self.external_root
            .as_ref()
            .map_or(MetadataKind::Configuration, |root| {
                root.kind.metadata_kind()
            })
    }
    /// Reject an explicit root in an older schema or a root profile inconsistent with its map.
    pub(super) fn validate_root(&self) -> Result<(), Error> {
        match &self.external_root {
            Some(root) => {
                root.validate()?;
                if self.schema_version != EXTERNAL_LAYOUT_VERSION || self.profile != root.kind.profile()
                    || !self.files.iter().any(|file| file.path == root.path && matches!(&file.role, FileRole::Descriptor {source, ..} if source.as_str() == root.kind.source())) {
                    return Err(Error::root("source_profile_unsupported"));
                }
            }
            None if self.schema_version == EXTERNAL_LAYOUT_VERSION
                || self.profile != super::PROFILE =>
            {
                return Err(Error::root("source_profile_unsupported"));
            }
            None => {}
        }
        Ok(())
    }
}

/// A validated external descriptor may change its root tag, never a nested object's spelling.
pub(super) fn external_kind(descriptor: &Descriptor) -> Option<ExternalKind> {
    match descriptor.profile.as_str() {
        PROCESSING_PROFILE => Some(ExternalKind::Processing),
        REPORT_PROFILE => Some(ExternalKind::Report),
        _ => None,
    }
}

/// Exactly one validated project root must own the complete typed descriptor graph.
pub(super) fn document_root(
    documents: &BTreeMap<SourcePath, NativeDocument>,
) -> Result<&NativeDocument, Error> {
    let mut candidates = documents.values().filter(|document| {
        document.descriptor.object.kind == MetadataKind::Configuration
            || external_kind(&document.descriptor).is_some()
    });
    let root = candidates
        .next()
        .ok_or_else(|| Error::root("source_descriptor_missing"))?;
    if candidates.next().is_some() {
        return Err(Error::root("source_descriptor_duplicate"));
    }
    Ok(root)
}

/// Map source filenames through the explicit external root while preserving every native path.
pub(super) fn descriptor_path(
    root: Option<&ExternalRoot>,
    kind: MetadataKind,
    native: &SourcePath,
) -> Result<SourcePath, Error> {
    if let Some(root) = root {
        if native == &root.path {
            return SourcePath::parse(root.kind.source());
        }
        let relative = SourcePath::parse(root.relative(native.as_str()))?;
        return super::layout::descriptor_path(kind, &relative);
    }
    super::layout::descriptor_path(kind, native)
}
