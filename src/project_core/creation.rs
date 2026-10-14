//! New ESKA projects from Designer XML, without platform, Git or presentation dependencies.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    Project, ProjectName, SourceFormat,
    config::{self, ProjectConfig},
    platform_settings::{BuildSettings, PlatformVersion},
    publish::{self, PublishError},
    source_format::{
        AcceptedRef, Error, NativeUuid, SourcePath, SourceReader, accept_source, import_designer,
        import_designer_queries, import_designer_readable, import_designer_separated,
    },
    transport::TransportPlan,
};

/// Disposable generations are separate from both tracked input authorities.
pub const DEFAULT_GENERATIONS_DIRECTORY: &str = ".eska/generations";
const ACCEPTED_DIRECTORY: &str = ".eska/accepted";

/// Only implemented authoring layouts can be selected; opening a project never upgrades it.
#[derive(Clone, Copy)]
enum Layout {
    Family,
    Separated,
    Readable,
    Queries,
}

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
    create_project(input, destination, name, platform, frozen, Layout::Family)
}

/// Create a new experimental project with separate descriptor files and an independently pinned base.
///
/// Native input is the supplied authority, not an existing editable source tree. The legacy
/// creation API retains family layout; neither path upgrades an existing checkout when opening it.
///
/// # Errors
/// Returns the same source, exclusive destination, policy and publication failures as family import.
pub fn import_separated_project(
    input: &Path,
    destination: &Path,
    name: ProjectName,
    platform: Option<&PlatformVersion>,
    frozen: &[NativeUuid],
) -> Result<ImportedProject, Error> {
    create_project(
        input,
        destination,
        name,
        platform,
        frozen,
        Layout::Separated,
    )
}

/// Create a new project with named YAML properties and explicit module/resource authoring paths.
///
/// Native UUIDs, version, support and resource bytes are preserved. The independently pinned base
/// uses the same readable layout; no existing project or native input is migrated in place.
///
/// # Errors
/// Returns unsupported input, composition, path collision, policy or exclusive-publication failures.
pub fn import_readable_project(
    input: &Path,
    destination: &Path,
    name: ProjectName,
    platform: Option<&PlatformVersion>,
    frozen: &[NativeUuid],
) -> Result<ImportedProject, Error> {
    create_project(input, destination, name, platform, frozen, Layout::Readable)
}

/// Create a complete project with unified managed forms and separately authored plain queries.
///
/// Configuration, extension, processing and report inputs use the same checked native import.
/// Existing creation layouts retain their defaults; native names, UUIDs, support and payload bytes
/// are preserved. The published accepted reference is a candidate for independent approval.
///
/// # Errors
/// Returns unsupported native or query/form profiles, incomplete composition, unsafe paths,
/// unknown freeze targets, changed input or exclusive-publication failures.
pub fn import_query_project(
    input: &Path,
    destination: &Path,
    name: ProjectName,
    platform: Option<&PlatformVersion>,
    frozen: &[NativeUuid],
) -> Result<ImportedProject, Error> {
    create_project(input, destination, name, platform, frozen, Layout::Queries)
}

/// Project settings and authority publication are shared by the explicit authoring layouts.
fn create_project(
    input: &Path,
    destination: &Path,
    name: ProjectName,
    platform: Option<&PlatformVersion>,
    frozen: &[NativeUuid],
    layout: Layout,
) -> Result<ImportedProject, Error> {
    let root = destination_path(input, destination)?;
    fs::create_dir(&root).map_err(|error| io_error(&error))?;
    let result = populate(input, &root, name, platform, frozen, layout);
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
    name: ProjectName,
    platform: Option<&PlatformVersion>,
    frozen: &[NativeUuid],
    layout: Layout,
) -> Result<ImportedProject, Error> {
    let source = root.join("src");
    match layout {
        Layout::Family => import_designer(input, &source)?,
        Layout::Separated => import_designer_separated(input, &source)?,
        Layout::Readable => import_designer_readable(input, &source)?,
        Layout::Queries => import_designer_queries(input, &source)?,
    };
    let project_type = SourceReader::open(&source)?.project_type()?;
    let mut config = ProjectConfig::new(project_type)
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
    publish::publish_new(root, Path::new(config::FILE_NAME), &manifest)
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
