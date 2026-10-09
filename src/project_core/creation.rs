//! New ESKA projects from Designer XML, without platform, Git or presentation dependencies.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    Project, ProjectName, ProjectType, SourceFormat,
    config::{self, ProjectConfig},
    platform_settings::{BuildSettings, PlatformVersion},
    publish::{self, PublishError},
    source_format::{AcceptedRef, Error, NativeUuid, SourcePath, accept_source, import_designer},
    transport::TransportPlan,
};

/// Disposable generations are separate from both tracked input authorities.
pub const DEFAULT_GENERATIONS_DIRECTORY: &str = ".eska/generations";
const ACCEPTED_DIRECTORY: &str = ".eska/accepted";

/// Creation reports a candidate pin; CI approval must still be supplied independently.
#[derive(Debug)]
pub struct ImportedProject {
    pub project: Project,
    pub accepted: AcceptedRef,
}

/// Create one complete project in a new directory, publishing its manifest last.
///
/// The native input stays read-only. Import records the native metadata name and UUIDs;
/// the project name labels the workspace/artifact and never renames the configuration.
///
/// # Errors
/// Returns unsupported native input, unsafe/existing output, unknown freeze target or I/O.
/// On failure only the directory exclusively claimed by this operation is removed.
pub fn import_project(
    input: &Path,
    destination: &Path,
    name: ProjectName,
    platform: Option<&PlatformVersion>,
    frozen: &[NativeUuid],
) -> Result<ImportedProject, Error> {
    let mut config = ProjectConfig::new(ProjectType::Configuration)
        .with_name(name)
        .with_source_format(SourceFormat::Eska {
            accepted_base: SourcePath::parse(ACCEPTED_DIRECTORY)?,
        })
        .map_err(|_| Error::root("source_project_settings_invalid"))?;
    // An absent platform remains inheritable if this project becomes a workspace member.
    if let Some(platform) = platform {
        let settings = BuildSettings::new(platform.as_str(), PathBuf::from("build"))
            .map_err(|_| Error::root("source_project_settings_invalid"))?;
        config = config.with_build_settings(settings);
    }
    let manifest = config
        .to_workspace_member_toml()
        .map_err(|_| Error::root("source_schema_invalid"))?;
    let root = destination_path(input, destination)?;
    fs::create_dir(&root).map_err(|error| io_error(&error))?;
    let result = populate(input, &root, config, &manifest, frozen);
    if let Err(error) = result {
        fs::remove_dir_all(&root)
            .map_err(|_| Error::root("source_cleanup_failed").in_field(error.code))?;
        return Err(error);
    }
    result
}

/// Build tracked authorities and transport before the manifest can make a project discoverable.
fn populate(
    input: &Path,
    root: &Path,
    config: ProjectConfig,
    manifest: &str,
    frozen: &[NativeUuid],
) -> Result<ImportedProject, Error> {
    let source = root.join(config.source());
    import_designer(input, &source)?;
    fs::create_dir(root.join(".eska")).map_err(|error| io_error(&error))?;
    let accepted = accept_source(&source, &root.join(ACCEPTED_DIRECTORY), frozen)?;
    let project = config
        .into_project(root.to_owned())
        .map_err(|_| Error::root("source_path_invalid"))?;
    TransportPlan::prepare(&project)?.publish()?;
    publish::publish_new(
        root,
        Path::new(".gitignore"),
        "/build/\n/.eska/generations/\n",
    )
    .map_err(publication_error)?;
    publish::publish_new(root, Path::new(config::FILE_NAME), manifest)
        .map_err(publication_error)?;
    Ok(ImportedProject { project, accepted })
}

/// Resolve a new child of an existing parent before adding entries to the native inventory.
fn destination_path(input: &Path, destination: &Path) -> Result<PathBuf, Error> {
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    SourcePath::parse(name)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let root = parent
        .canonicalize()
        .map_err(|error| io_error(&error))?
        .join(name);
    let input = input.canonicalize().map_err(|error| io_error(&error))?;
    if root.starts_with(&input) || input.starts_with(&root) {
        return Err(Error::root("source_tree_overlap"));
    }
    match fs::symlink_metadata(&root) {
        Ok(_) => Err(Error::root("source_destination_exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(root),
        Err(error) => Err(io_error(&error)),
    }
}

/// Keep file publication failures machine-facing at the source-operation boundary.
fn publication_error(error: PublishError) -> Error {
    match error {
        PublishError::Conflict => Error::root("source_input_changed"),
        PublishError::UnsafePath => Error::root("source_path_invalid"),
        PublishError::ReadOnly => Error::root("source_read_only"),
        PublishError::Io(error) => io_error(&error),
    }
}

/// Creation only reports stable filesystem classes, never OS-localized diagnostics.
fn io_error(error: &std::io::Error) -> Error {
    Error::root(match error.kind() {
        std::io::ErrorKind::NotFound => "source_file_missing",
        std::io::ErrorKind::AlreadyExists => "source_destination_exists",
        _ => "source_io",
    })
}

#[cfg(test)]
mod tests;
