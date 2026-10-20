//! Materialize immutable Git blobs and prepared module bodies into a caller-owned empty namespace.

use super::{PatchError, PatchPlan};
use crate::vcs::repository::Repository;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Write only a freshly claimed empty directory; the caller remains responsible for rollback.
///
/// # Errors
/// Rejects collisions, links, invalid snapshots and incomplete source publication.
pub fn materialize(plan: &PatchPlan, workspace: &Path) -> Result<(), PatchError> {
    if !fs::symlink_metadata(workspace)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink())
        || fs::read_dir(workspace)
            .map_err(|error| PatchError::new("io", error.to_string()))?
            .next()
            .is_some()
    {
        return Err(PatchError::new("path", workspace.display().to_string()));
    }
    stage_base(plan, workspace)?;
    for module in &plan.modules {
        write_source(
            &workspace.join(format!("modules/{}.xml", module.name)),
            &module.descriptor,
        )?;
        write_source(
            &workspace.join(format!("modules/{}/Ext/Module.bsl", module.name)),
            &module.code,
        )?;
    }
    Ok(())
}

/// Materialize the exact merge-base source tree in the owned workspace.
fn stage_base(plan: &PatchPlan, workspace: &Path) -> Result<PathBuf, PatchError> {
    let repository = Repository::discover(&plan.repository_root)
        .map_err(|e| PatchError::new("repository", format!("{e:?}")))?;
    let base = workspace.join("base");
    for (path, id) in &plan.source_files {
        let contents = repository
            .blob(*id)
            .map_err(|e| PatchError::new("repository", format!("{e:?}")))?;
        if contents.starts_with(b"version https://git-lfs.github.com/spec/v1") {
            return Err(PatchError::new("unsupported", path));
        }
        let target = base.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| PatchError::new("io", e.to_string()))?;
        }
        fs::write(target, contents).map_err(|e| PatchError::new("io", e.to_string()))?;
    }
    Ok(base)
}

/// Keep generated 1C sources in their native BOM/CRLF representation.
fn write_source(path: &Path, text: &str) -> Result<(), PatchError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| PatchError::new("io", e.to_string()))?;
    }
    let text = eska_project_core::patch::normalize_designer_text(text);
    fs::write(path, text).map_err(|e| PatchError::new("io", e.to_string()))
}
