//! Validate the manifest's project type against a root descriptor without reading descendants.

use crate::project::{ProjectConfiguration, ProjectType, designer_xml};

/// Project metadata records the manifest type, including configuration versus extension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataProject {
    project_type: ProjectType,
}

impl MetadataProject {
    /// Check a root descriptor supplied by the resolver against the loaded manifest.
    ///
    /// This does not infer configuration from a directory or require a build platform.
    /// Filesystem discovery and the mandatory manifest belong to the opening boundary.
    ///
    /// # Errors
    /// Returns malformed XML, an unsupported root or a manifest/type mismatch.
    pub fn from_root_descriptor(
        configuration: &ProjectConfiguration,
        descriptor: &str,
    ) -> Result<Self, MetadataProjectError> {
        let actual = designer_xml::project_type(descriptor)
            .map_err(MetadataProjectError::InvalidXml)?
            .ok_or(MetadataProjectError::UnsupportedRoot)?;
        let expected = configuration.project_type();
        if actual != expected {
            return Err(MetadataProjectError::TypeMismatch { expected, actual });
        }
        Ok(Self {
            project_type: expected,
        })
    }

    /// Return the checked manifest type used to select a tree schema.
    #[must_use]
    pub const fn project_type(&self) -> ProjectType {
        self.project_type
    }
}

/// Structured root validation failures, localized only by a future presentation layer.
#[derive(Debug)]
pub enum MetadataProjectError {
    InvalidXml(roxmltree::Error),
    UnsupportedRoot,
    TypeMismatch {
        expected: ProjectType,
        actual: ProjectType,
    },
}
