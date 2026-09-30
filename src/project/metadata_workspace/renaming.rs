//! Rename previews resolve declarations through the same source and support boundaries as editing.

mod bsl;
mod context;

use std::{collections::BTreeMap, fmt::Write, path::PathBuf};

use sha2::{Digest, Sha256};

use super::{ProjectSession, WorkspaceError};
use crate::project::{
    metadata_edit::EditError,
    metadata_model::ObjectId,
    metadata_rename::{
        NameError, RenameIssue, RenamePlan,
        inventory::{Inventory, MAX_TEXT_BYTES, MAX_XML_BYTES, file_hash, read_text},
    },
};

/// Preview failures contain semantic identities or source paths, never instructions or XML supplied by clients.
#[derive(Debug)]
pub enum RenameError {
    Workspace(Box<WorkspaceError>),
    Name(NameError),
    Collision(ObjectId),
    Edit(EditError),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl From<WorkspaceError> for RenameError {
    /// Retain discovery diagnostics at the structural editing boundary.
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(Box::new(error))
    }
}

impl From<EditError> for RenameError {
    /// Keep exact-source and XML failures distinguishable from an invalid destination name.
    fn from(error: EditError) -> Self {
        Self::Edit(error)
    }
}

impl ProjectSession {
    /// Build a source-wide preview without writing files, caches or session history.
    ///
    /// # Errors
    /// Rejects invalid names, collisions, malformed identities, unsafe paths and unavailable sources.
    pub fn preview_rename(
        &mut self,
        id: &ObjectId,
        new_name: &str,
    ) -> Result<RenamePlan, RenameError> {
        let context = self.rename_context(id, new_name)?;
        let root = self.project().source().to_path_buf();
        let inventory = Inventory::read(&root, &self.rename_exclusions()).map_err(|source| {
            RenameError::Io {
                path: PathBuf::new(),
                source,
            }
        })?;
        let environment = bsl::Environment::read(
            &root,
            self.source.descriptor(),
            self.project().configuration().project_type(),
            &inventory,
        )?;
        let mut plan = scan_sources(&context, id, new_name, &root, inventory, &environment)?;
        let paths: Vec<_> = plan
            .files
            .iter()
            .filter(|file| {
                !file.replacements.is_empty()
                    && file.path != std::path::Path::new("ConfigDumpInfo.xml")
            })
            .map(|file| file.path.clone())
            .collect();
        let support = self.support_files(&paths)?;
        for file in support.files {
            if file.read_only || file.unknown {
                plan.issues.push(RenameIssue {
                    path: file.path,
                    reason: "source_read_only",
                });
            }
        }
        if !support.diagnostics.is_empty() {
            plan.issues.push(RenameIssue {
                path: PathBuf::new(),
                reason: "support_unavailable",
            });
        }
        Ok(plan)
    }

    /// A source at project root must not include Git internals, disposable caches or build output.
    fn rename_exclusions(&self) -> Vec<PathBuf> {
        let project = self.project();
        let mut excluded = vec![PathBuf::from(".git"), PathBuf::from(".eska")];
        let output = project.root().join(
            project
                .configuration()
                .build_settings()
                .artifacts_directory(),
        );
        if let Ok(relative) = output.strip_prefix(project.source())
            && let Some(first) = relative
                .components()
                .next()
                .and_then(|part| part.as_os_str().to_str())
            && first != "."
            && first != "Ext"
            && crate::project::metadata_model::MetadataKind::from_collection_folder(first).is_none()
            && Some(first)
                != self
                    .source
                    .descriptor()
                    .file_stem()
                    .and_then(|stem| stem.to_str())
        {
            excluded.push(relative.to_path_buf());
        }
        excluded
    }
}

/// Fingerprint all sources, including binary payloads, empty directories and earlier binding dependencies.
fn scan_sources(
    context: &context::RenameContext,
    id: &ObjectId,
    new_name: &str,
    root: &std::path::Path,
    inventory: Inventory,
    environment: &bsl::Environment,
) -> Result<RenamePlan, RenameError> {
    let mut plan = RenamePlan {
        object_id: id.clone(),
        new_object_id: context.new_id.clone(),
        uuid: context.uuid.clone(),
        old_name: context.old_name.clone(),
        new_name: new_name.to_owned(),
        snapshot: String::new(),
        files: Vec::new(),
        moves: context.moves(&inventory)?,
        issues: Vec::new(),
    };
    let mut digest = Sha256::new();
    digest.update(id.as_str());
    digest.update([0]);
    digest.update(new_name);
    let mut hashes = BTreeMap::new();
    for path in &inventory.files {
        let bytes_hash = scan_file(context, root, path, &mut plan, environment)?;
        hashes.insert(path.clone(), hex(&bytes_hash));
        digest.update([0]);
        digest.update((path.as_os_str().as_encoded_bytes().len() as u64).to_le_bytes());
        digest.update(path.as_os_str().as_encoded_bytes());
        digest.update(bytes_hash);
    }
    for path in inventory.directories {
        digest.update([1]);
        digest.update((path.as_os_str().as_encoded_bytes().len() as u64).to_le_bytes());
        digest.update(path.as_os_str().as_encoded_bytes());
    }
    context.check_snapshots(&hashes)?;
    environment.check_snapshots(&hashes)?;
    plan.snapshot = hex(&digest.finalize());
    Ok(plan)
}

/// Hash text only once and expose bounded-parser limitations without losing the rest of the preview.
fn scan_file(
    context: &context::RenameContext,
    root: &std::path::Path,
    path: &std::path::Path,
    plan: &mut RenamePlan,
    environment: &bsl::Environment,
) -> Result<[u8; 32], RenameError> {
    let kind = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if matches!(kind, "xml" | "bsl") {
        let size = std::fs::metadata(root.join(path))
            .map_err(|source| RenameError::Io {
                path: path.to_path_buf(),
                source,
            })?
            .len();
        let max_bytes = if kind == "xml" {
            MAX_XML_BYTES
        } else {
            MAX_TEXT_BYTES
        };
        if size > max_bytes {
            plan.issues.push(RenameIssue {
                path: path.to_path_buf(),
                reason: "text_too_large",
            });
        } else {
            let input =
                read_text(&root.join(path), max_bytes).map_err(|source| RenameError::Io {
                    path: path.to_path_buf(),
                    source,
                })?;
            let hash: [u8; 32] = Sha256::digest(input.as_bytes()).into();
            if plan.new_name != context.old_name {
                match context.analyze(path, kind, &input, &hex(&hash), environment) {
                    Ok(file) if !file.replacements.is_empty() || !file.uncertain.is_empty() => {
                        plan.files.push(file);
                    }
                    Ok(_) => (),
                    Err(RenameError::Edit(EditError::InvalidXml)) => {
                        plan.issues.push(RenameIssue {
                            path: path.to_path_buf(),
                            reason: "xml_unavailable",
                        });
                    }
                    Err(error) => return Err(error),
                }
            }
            return Ok(hash);
        }
    } else if kind == "bin"
        && path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem.ends_with("Module") || stem == "Form")
    {
        plan.issues.push(RenameIssue {
            path: path.to_path_buf(),
            reason: "opaque_code_or_form",
        });
    }
    file_hash(&root.join(path)).map_err(|source| RenameError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// The public snapshot is a fixed-width digest, independent of locale or filesystem timestamps.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        },
    )
}
