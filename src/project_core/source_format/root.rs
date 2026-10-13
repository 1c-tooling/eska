//! Native root bindings distinguish external objects from configuration composition.

use super::{Descriptor, Error, FileRole, NativeDocument, SourceManifest, SourcePath};
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

impl SourceManifest {
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
