//! Editor publication reuses import's fresh project selection and exact inherited settings fence.

use super::{
    BTreeMap, Error, Path, Project, ProjectScope, SourcePath, WorkspaceSettings, checked_directory,
    files, require_project_ready, selected_workspace, settings,
};

/// An initialized project cannot acquire write authority from a newly selected settings graph.
#[derive(Clone, Debug)]
pub(in crate::project_core::source_format) struct ProjectSettingsGuard {
    project: Project,
    scope: ProjectScope,
    settings: BTreeMap<SourcePath, Option<String>>,
    workspace: Option<WorkspaceSettings>,
}

impl ProjectSettingsGuard {
    /// Re-discovery must still select the caller's initialized project with the same resolved config.
    pub(in crate::project_core::source_format) fn capture(
        project: &Project,
        scope: &ProjectScope,
    ) -> Result<Self, Error> {
        checked_directory(project.root())?;
        let workspace = selected_workspace(project, scope).map_err(selection_error)?;
        let guard = Self {
            project: project.clone(),
            scope: scope.clone(),
            settings: settings(project.root())?,
            workspace,
        };
        guard.verify(&|| false)?;
        Ok(guard)
    }

    /// This exact root selects the same stable project write lock used by import and recovery.
    pub(in crate::project_core::source_format) fn root(&self) -> &Path {
        self.project.root()
    }

    /// Save repeats this check under the project lock; arbitrary external writers remain outside it.
    pub(in crate::project_core::source_format) fn verify(
        &self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Error> {
        files::check_cancelled(cancelled)?;
        require_project_ready(self.root())?;
        checked_directory(self.root())?;
        if selected_workspace(&self.project, &self.scope).map_err(selection_error)?
            != self.workspace
            || settings(self.root())? != self.settings
        {
            return Err(Error::root("source_input_changed"));
        }
        files::check_cancelled(cancelled)?;
        require_project_ready(self.root())
    }
}

/// The initialized selection is already known; a newly different resolved project is settings drift.
fn selection_error(error: Error) -> Error {
    if error.code == "source_import_scope_mismatch" {
        Error::root("source_input_changed")
    } else {
        error
    }
}
