//! Minimal native roots feed the existing complete Queries creation pipeline.

mod identity;
mod template;
#[cfg(test)]
mod tests;

use std::{fs, path::Path};

use crate::project_core::{
    ProjectName, ProjectType, native_version::ProjectVersion, platform_settings::PlatformVersion,
    source_format::Error,
};

use super::{ImportedProject, Layout, create_project, io_error};

/// The initial native version used by the current new-project command.
pub const INITIAL_PROJECT_VERSION: &str = "0.1.0";

/// A selected native profile and version; source authority is produced by the shared importer.
#[derive(Clone, Debug)]
pub struct NewProjectOptions {
    project_type: ProjectType,
    metadata_name: Option<String>,
    version: ProjectVersion,
    platform: Option<PlatformVersion>,
}

impl NewProjectOptions {
    /// Select a native profile and an independently validated initial version.
    #[must_use]
    pub const fn new(project_type: ProjectType, version: ProjectVersion) -> Self {
        Self {
            project_type,
            metadata_name: None,
            version,
            platform: None,
        }
    }

    /// Retain an explicit metadata identifier without normalizing its spelling.
    ///
    /// # Errors
    /// Returns `source_metadata_name_invalid` for a non-identifier or nonportable native filename.
    pub fn with_metadata_name(mut self, name: String) -> Result<Self, Error> {
        template::validate_name(&name)?;
        self.metadata_name = Some(name);
        Ok(self)
    }

    /// Leave an absent platform available for ordinary workspace inheritance.
    #[must_use]
    pub fn with_platform(mut self, platform: Option<PlatformVersion>) -> Self {
        self.platform = platform;
        self
    }

    /// Resolve only the default from a portable label; an explicit identifier remains exact.
    #[must_use]
    pub fn metadata_name(&self, label: &ProjectName) -> String {
        self.metadata_name.clone().unwrap_or_else(|| {
            let identifier = label.as_str().replace('-', "_");
            if identifier.starts_with(|ch: char| ch.is_ascii_digit()) {
                format!("Project_{identifier}")
            } else {
                identifier
            }
        })
    }

    /// Expose the selected native version without reading derivative XML.
    #[must_use]
    pub const fn version(&self) -> &ProjectVersion {
        &self.version
    }
}

/// Create a minimal configuration, extension, processing or report in an exclusively new directory.
///
/// A temporary native sibling supplies fresh identities to the same importer as user-supplied XML.
/// The importer publishes editable source, separate candidates for independent confirmation,
/// byte-safe transport and settings last. Neither platform processes nor Git are used here.
/// The operation creates a standalone project; workspace enrollment and inherited version-scheme
/// checks still belong to normal discovery and are not bypassed by this API.
///
/// # Errors
/// Returns invalid names/paths, unavailable system entropy, exclusive-publication, source or cleanup
/// failures. Cleanup removes only the temporary native sibling owned by this invocation; creation
/// failures retain the shared importer's destination-cleanup contract.
pub fn new_project(
    destination: &Path,
    name: ProjectName,
    options: &NewProjectOptions,
) -> Result<ImportedProject, Error> {
    let metadata_name = options.metadata_name(&name);
    template::validate_name(&metadata_name)?;
    let stage = template::NativeStage::claim(destination)?;
    let result = (|| {
        template::write(&stage.path, options, &metadata_name)?;
        create_project(
            &stage.path,
            destination,
            name,
            options.platform.as_ref(),
            &[],
            Layout::Queries,
            Some(options.version.scheme()),
        )
    })();
    stage.finish(result)
}

/// Report a failed owned-stage cleanup even when the shared importer published a complete project.
fn finish_stage(
    path: &Path,
    result: Result<ImportedProject, Error>,
) -> Result<ImportedProject, Error> {
    if fs::remove_dir_all(path).is_err() {
        let cause = result
            .as_ref()
            .err()
            .map_or("source_project_created", |error| error.code);
        return Err(Error::root("source_cleanup_failed")
            .in_field(cause)
            .in_field(path.to_string_lossy().into_owned()));
    }
    result
}
