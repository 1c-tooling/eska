//! Only exclusively new project creation may persist a candidate ancestor before publishing settings.

use super::{
    Error, ImportAncestorRef, InitialOrigin, InitialState, Project, SERVICE, STATE, SourcePath,
    checked_directory, files, fs, portable, read_optional, read_state,
};
use crate::project_core::source_format::{capture_import_ancestor, project_migration::publish_new};

/// Capture a complete freshly imported source without deriving ancestry from an existing project.
///
/// The new project root belongs exclusively to creation and has no published manifest. Initial
/// state cannot grant support, confirmation or journal recovery authority; callers retain its
/// candidate digest independently. The caller owns cleanup of the unpublished project on failure.
pub fn initialize_project_import_ancestor(project: &Project) -> Result<ImportAncestorRef, Error> {
    let root = project.root();
    checked_directory(root)?;
    if read_optional(&root.join(crate::project_core::config::FILE_NAME))?.is_some() {
        return Err(Error::root("source_destination_exists"));
    }
    let source = portable(
        project
            .source()
            .strip_prefix(root)
            .map_err(|_| Error::root("source_path_invalid"))?,
    )?;
    let service = root.join(SERVICE);
    checked_directory(&root.join(".eska"))?;
    fs::create_dir(&service).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::root("source_destination_exists")
        } else {
            files::io(&error)
        }
    })?;
    let result = capture(project, &source);
    if result.is_err() {
        fs::remove_dir_all(service).map_err(|_| Error::root("source_cleanup_failed"))?;
    }
    result
}

/// Store bytes and the verified state pointer before the manifest can expose the new project.
fn capture(project: &Project, source: &SourcePath) -> Result<ImportAncestorRef, Error> {
    let root = project.root();
    let ancestor = capture_import_ancestor(project.source(), &root.join(SERVICE).join("initial"))?;
    let state = InitialState {
        schema_version: 2,
        origin: InitialOrigin::Initial,
        source: source.clone(),
        ancestor: ancestor.clone(),
    };
    let text = serde_json::to_string(&state).map_err(|_| Error::root("source_schema_invalid"))?;
    publish_new(root, STATE, &text)?;
    read_state(root, source, &text)?;
    Ok(ancestor)
}
