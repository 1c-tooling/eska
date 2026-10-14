//! Independently retained recovery references are published before the project guard can be created.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use crate::{
    project_core::{Project, SourceFormat, publish},
    tool_cli::{Failure, path_text},
};

/// A new receipt cannot modify any input authority, source data or recovery journal.
pub(super) fn destination(
    project: &Project,
    ancestor: Option<&Path>,
    requested: &Path,
) -> Result<PathBuf, Failure> {
    path_text(requested)?;
    let name = requested
        .file_name()
        .ok_or_else(|| Failure::new("source_path_invalid"))?;
    let parent = requested
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|_| Failure::new("source_file_missing"))?;
    let destination = parent.join(name);
    path_text(&destination)?;
    let SourceFormat::Eska { accepted_base } = project.configuration().source_format() else {
        return Err(Failure::new("source_format_mismatch"));
    };
    let mut protected = vec![
        project.source().to_owned(),
        project.root().join(accepted_base.as_path()),
        project.root().join(".eska/source-import"),
    ];
    if let Some(ancestor) = ancestor {
        protected.push(
            ancestor
                .canonicalize()
                .map_err(|_| Failure::new("source_ancestor_missing"))?,
        );
    }
    if protected.iter().any(|root| destination.starts_with(root)) {
        return Err(Failure::new("source_import_receipt_path_invalid").at(path_text(&destination)?));
    }
    match fs::symlink_metadata(&destination) {
        Ok(_) => Err(Failure::new("source_destination_exists").at(path_text(&destination)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(destination),
        Err(_) => {
            Err(Failure::new("source_import_receipt_write_failed").at(path_text(&destination)?))
        }
    }
}

/// Atomic exclusive publication keeps a complete digest available if source publication is interrupted.
pub(super) fn retain(destination: &Path, preview: &Value) -> Result<(), Failure> {
    let text = serde_json::to_string_pretty(&json!({
        "schema_version":1,"project":preview["project"],"scope":preview["scope"],
        "transaction":preview["transaction"],"approved":preview["approved"],
        "ancestor":preview["ancestor"],"incoming":preview["incoming"],"resulting_ancestor":preview["resulting_ancestor"],
        "working_revision":preview["working_revision"],"output_revision":preview["output_revision"],
    }))
    .map_err(|_| Failure::new("output_failed"))?
        + "\n";
    let parent = destination
        .parent()
        .ok_or_else(|| Failure::new("source_path_invalid"))?;
    let name = destination
        .file_name()
        .ok_or_else(|| Failure::new("source_path_invalid"))?;
    publish::publish_new(parent, Path::new(name), &text).map_err(|_| {
        Failure::new("source_import_receipt_write_failed").at(destination.to_string_lossy())
    })
}
