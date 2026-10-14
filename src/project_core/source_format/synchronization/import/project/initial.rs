//! Creation and attachment store candidate ancestry before exposing project settings.

use super::{
    Error, ImportAncestor, ImportAncestorRef, InitialOrigin, InitialState, Project, SERVICE, STATE,
    SourcePath, checked_directory, files, fs, portable, read_optional, read_state,
};
use crate::project_core::source_format::{capture_import_ancestor, project_migration::publish_new};
use std::path::Path;

#[cfg(test)]
mod tests;

/// Verify newly created ancestry before a project can be enrolled as a workspace member.
///
/// No workspace selection, published import history or support approval is inferred. The exact
/// initial state, configured source path, caller reference and stored bytes must agree.
///
/// # Errors
/// Returns unresolved import, non-initial/malformed state, changed paths or untrusted stored bytes.
pub fn open_initial_project_import_ancestor(
    project: &Project,
    expected: &ImportAncestorRef,
) -> Result<ImportAncestor, Error> {
    let root = project.root();
    super::require_project_ready(root)?;
    checked_directory(root)?;
    let source = portable(
        project
            .source()
            .strip_prefix(root)
            .map_err(|_| Error::root("source_path_invalid"))?,
    )?;
    let text =
        read_optional(&root.join(STATE))?.ok_or_else(|| Error::root("source_ancestor_missing"))?;
    // Parse only the closed initial schema. A published state must never cause this getter
    // to traverse a history store, even when that history would eventually be refused.
    let state: InitialState =
        serde_json::from_str(&text).map_err(|_| Error::root("source_import_state_invalid"))?;
    if state.schema_version != 2 || state.source != source {
        return Err(Error::root("source_import_state_invalid"));
    }
    if state.ancestor != *expected {
        return Err(Error::root("source_ancestor_untrusted"));
    }
    checked_directory(&root.join(SERVICE))?;
    let store = root.join(SERVICE).join("initial");
    checked_directory(&store)?;
    let result = ImportAncestor::open(&store, expected)?;
    if read_optional(&root.join(STATE))?.as_deref() != Some(text.as_str()) {
        return Err(Error::root("source_import_state_invalid"));
    }
    super::require_project_ready(root)?;
    Ok(result)
}

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
    let ancestor = capture_in(project.source(), source, &root.join(SERVICE))?;
    let text = read_optional(&root.join(STATE))?
        .ok_or_else(|| Error::root("source_import_state_invalid"))?;
    read_state(root, source, &text)?;
    Ok(ancestor)
}

/// Existing-root attachment captures real source bytes in an exclusively owned private store.
pub(in crate::project_core::source_format) fn stage_project_import_ancestor(
    project: &Project,
    destination: &Path,
) -> Result<ImportAncestorRef, Error> {
    checked_directory(project.root())?;
    if read_optional(&project.root().join(crate::project_core::config::FILE_NAME))?.is_some() {
        return Err(Error::root("source_destination_exists"));
    }
    let source = portable(
        project
            .source()
            .strip_prefix(project.root())
            .map_err(|_| Error::root("source_path_invalid"))?,
    )?;
    let parent = destination
        .parent()
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    checked_directory(parent)?;
    fs::create_dir(destination).map_err(|error| files::io(&error))?;
    let result = capture_in(project.source(), &source, destination);
    if result.is_err() {
        fs::remove_dir_all(destination).map_err(|_| Error::root("source_cleanup_failed"))?;
    }
    result
}

/// The state retains the original project's relative source, independent of temporary storage.
fn capture_in(
    source_root: &Path,
    source: &SourcePath,
    destination: &Path,
) -> Result<ImportAncestorRef, Error> {
    let ancestor = capture_import_ancestor(source_root, &destination.join("initial"))?;
    let state = InitialState {
        schema_version: 2,
        origin: InitialOrigin::Initial,
        source: source.clone(),
        ancestor: ancestor.clone(),
    };
    let text = serde_json::to_string(&state).map_err(|_| Error::root("source_schema_invalid"))?;
    publish_new(destination, "state.json", &text)?;
    if read_optional(&destination.join("state.json"))?.as_deref() != Some(text.as_str()) {
        return Err(Error::root("source_import_state_invalid"));
    }
    ImportAncestor::open(&destination.join("initial"), &ancestor)?;
    Ok(ancestor)
}
