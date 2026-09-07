//! Transactional enrollment of a project directory into an existing workspace.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use toml_edit::DocumentMut;

use crate::config::{FILE_NAME, WorkspaceConfig, WorkspaceConfigError};

use super::{ProjectName, Workspace, discovery::ContextDiscoveryError};

/// A validated root-manifest change prepared before project files are written.
#[derive(Debug)]
pub struct WorkspaceMemberEnrollment {
    workspace_root: PathBuf,
    member_root: PathBuf,
    member_path: PathBuf,
    manifest: PathBuf,
    original_manifest: Vec<u8>,
    updated_manifest: Vec<u8>,
}

impl WorkspaceMemberEnrollment {
    #[must_use]
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    #[must_use]
    pub fn member_root(&self) -> &Path {
        &self.member_root
    }

    #[must_use]
    pub fn member_path(&self) -> &Path {
        &self.member_path
    }

    /// Publishes the prepared manifest only if it has not changed since preflight.
    ///
    /// # Errors
    /// Returns a structured I/O or concurrent-change error without overwriting a
    /// newer manifest.
    pub fn publish(&self) -> Result<(), WorkspaceEnrollmentError> {
        replace_exact(
            &self.manifest,
            &self.original_manifest,
            &self.updated_manifest,
        )
    }

    /// Restores the exact original bytes after a failed enrollment validation.
    ///
    /// # Errors
    /// Refuses to overwrite a manifest changed by another process.
    pub fn restore(&self) -> Result<(), WorkspaceEnrollmentError> {
        replace_exact(
            &self.manifest,
            &self.updated_manifest,
            &self.original_manifest,
        )
    }
}

/// Finds a workspace for onboarding while preserving standalone command behavior.
///
/// # Errors
/// Returns invalid project/workspace configuration errors instead of silently
/// treating a broken manifest as an ordinary directory.
pub fn find_workspace(start: &Path) -> Result<Option<Workspace>, ContextDiscoveryError> {
    match super::discovery::discover_context(start) {
        Ok(super::discovery::DiscoveryContext::Workspace { workspace, .. }) => Ok(Some(workspace)),
        Ok(super::discovery::DiscoveryContext::Standalone(_))
        | Err(ContextDiscoveryError::Project(super::discovery::DiscoveryError::NotFound {
            ..
        })) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Prepares an append-only edit of `workspace.members` without writing files.
///
/// `member_root` must be resolved against the canonical workspace root. Existing
/// member names and directory boundaries are checked before any mutation.
///
/// # Errors
/// Returns structured name, path, TOML, configuration or I/O errors.
pub fn prepare(
    workspace: &Workspace,
    member_root: PathBuf,
    member_path: PathBuf,
    name: &ProjectName,
) -> Result<WorkspaceMemberEnrollment, WorkspaceEnrollmentError> {
    validate_member_path(workspace, &member_root, &member_path, name)?;
    let manifest = workspace.root().join(FILE_NAME);
    let original_manifest = fs::read(&manifest).map_err(|source| WorkspaceEnrollmentError::Io {
        path: manifest.clone(),
        source,
    })?;
    let input =
        std::str::from_utf8(&original_manifest).map_err(|source| WorkspaceEnrollmentError::Io {
            path: manifest.clone(),
            source: io::Error::new(io::ErrorKind::InvalidData, source),
        })?;
    WorkspaceConfig::from_toml(input).map_err(WorkspaceEnrollmentError::Config)?;
    let mut document = input
        .parse::<DocumentMut>()
        .map_err(WorkspaceEnrollmentError::TomlEdit)?;
    let members = document["workspace"]["members"]
        .as_array_mut()
        .ok_or(WorkspaceEnrollmentError::MembersNotArray)?;
    let member =
        member_path
            .to_str()
            .ok_or_else(|| WorkspaceEnrollmentError::NonUtf8MemberPath {
                path: member_path.clone(),
            })?;
    members.push(member);
    let updated_manifest = document.to_string().into_bytes();
    let updated =
        std::str::from_utf8(&updated_manifest).map_err(|source| WorkspaceEnrollmentError::Io {
            path: manifest.clone(),
            source: io::Error::new(io::ErrorKind::InvalidData, source),
        })?;
    WorkspaceConfig::from_toml(updated).map_err(WorkspaceEnrollmentError::Config)?;

    Ok(WorkspaceMemberEnrollment {
        workspace_root: workspace.root().to_path_buf(),
        member_root,
        member_path,
        manifest,
        original_manifest,
        updated_manifest,
    })
}

fn validate_member_path(
    workspace: &Workspace,
    member_root: &Path,
    member_path: &Path,
    name: &ProjectName,
) -> Result<(), WorkspaceEnrollmentError> {
    if member_path.as_os_str().is_empty()
        || member_path.is_absolute()
        || !member_path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        || member_root == workspace.root()
        || !member_root.starts_with(workspace.root())
    {
        return Err(WorkspaceEnrollmentError::OutsideWorkspace {
            workspace: workspace.root().to_path_buf(),
            member: member_root.to_path_buf(),
        });
    }
    if workspace
        .members()
        .iter()
        .any(|member| member.name() == name)
    {
        return Err(WorkspaceEnrollmentError::DuplicateName { name: name.clone() });
    }
    for existing in workspace.members() {
        if existing.root() == member_root {
            return Err(WorkspaceEnrollmentError::DuplicateMember {
                path: member_root.to_path_buf(),
            });
        }
        if existing.root().starts_with(member_root) || member_root.starts_with(existing.root()) {
            return Err(WorkspaceEnrollmentError::NestedMembers {
                first: existing.root().to_path_buf(),
                second: member_root.to_path_buf(),
            });
        }
    }
    Ok(())
}

fn replace_exact(
    path: &Path,
    expected: &[u8],
    replacement: &[u8],
) -> Result<(), WorkspaceEnrollmentError> {
    let current = fs::read(path).map_err(|source| WorkspaceEnrollmentError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if current != expected {
        return Err(WorkspaceEnrollmentError::ManifestChanged {
            path: path.to_path_buf(),
        });
    }
    let (temporary, mut file) = create_unique_sibling(path)?;
    let result = file.write_all(replacement).and_then(|()| file.sync_all());
    drop(file);
    if let Err(source) = result {
        let _ = fs::remove_file(&temporary);
        return Err(WorkspaceEnrollmentError::Io {
            path: temporary,
            source,
        });
    }
    if let Ok(metadata) = fs::metadata(path)
        && let Err(source) = fs::set_permissions(&temporary, metadata.permissions())
    {
        let _ = fs::remove_file(&temporary);
        return Err(WorkspaceEnrollmentError::Io {
            path: temporary,
            source,
        });
    }
    let current = fs::read(path).map_err(|source| WorkspaceEnrollmentError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if current != expected {
        let _ = fs::remove_file(&temporary);
        return Err(WorkspaceEnrollmentError::ManifestChanged {
            path: path.to_path_buf(),
        });
    }
    replace_published(&temporary, path).map_err(|source| {
        let _ = fs::remove_file(&temporary);
        WorkspaceEnrollmentError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// Claims a temporary manifest path without touching any earlier temporary file.
fn create_unique_sibling(path: &Path) -> Result<(PathBuf, File), WorkspaceEnrollmentError> {
    for sequence in 0..1000_u16 {
        let candidate = path.with_extension(format!("toml.member-{sequence}"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(WorkspaceEnrollmentError::Io {
                    path: candidate,
                    source,
                });
            }
        }
    }
    Err(WorkspaceEnrollmentError::Io {
        path: path.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no unique workspace manifest path available",
        ),
    })
}

#[cfg(not(windows))]
/// Atomically replaces the manifest on platforms where rename overwrites a file.
fn replace_published(temporary: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(windows)]
/// Replaces the manifest through a backup because Windows rename cannot overwrite.
fn replace_published(temporary: &Path, path: &Path) -> io::Result<()> {
    let backup = path.with_extension("toml.member-backup");
    fs::rename(path, &backup)?;
    if let Err(source) = fs::rename(temporary, path) {
        let _ = fs::rename(&backup, path);
        return Err(source);
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

/// Structured workspace-member enrollment failures; localization belongs to CLI.
#[derive(Debug)]
pub enum WorkspaceEnrollmentError {
    Io { path: PathBuf, source: io::Error },
    Config(WorkspaceConfigError),
    TomlEdit(toml_edit::TomlError),
    MembersNotArray,
    NonUtf8MemberPath { path: PathBuf },
    OutsideWorkspace { workspace: PathBuf, member: PathBuf },
    DuplicateMember { path: PathBuf },
    NestedMembers { first: PathBuf, second: PathBuf },
    DuplicateName { name: ProjectName },
    ManifestChanged { path: PathBuf },
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{WorkspaceEnrollmentError, prepare};
    use crate::{
        project::{
            ProjectName,
            discovery::{DiscoveryContext, discover_context},
        },
        test_support::TestDir,
    };

    /// Builds a valid empty workspace with formatting worth preserving.
    fn workspace() -> (TestDir, crate::project::Workspace, Vec<u8>) {
        let fixture = TestDir::new();
        let original = b"# keep this comment\n[workspace]\nmembers = [] # members comment\n\n[build]\nplatform_version = \"\"\n"
            .to_vec();
        fs::write(fixture.0.join("eska.toml"), &original).unwrap();
        let context = discover_context(&fixture.0).unwrap();
        let DiscoveryContext::Workspace { workspace, .. } = context else {
            panic!("workspace context");
        };
        (fixture, workspace, original)
    }

    #[test]
    fn enrollment_preserves_formatting_and_restores_exact_original_bytes() {
        let (fixture, workspace, original) = workspace();
        let name = ProjectName::parse("my-orders".to_owned()).unwrap();
        let plan = prepare(
            &workspace,
            fixture.0.join("src/my-orders"),
            PathBuf::from("src/my-orders"),
            &name,
        )
        .unwrap();
        plan.publish().unwrap();
        let updated = fs::read_to_string(fixture.0.join("eska.toml")).unwrap();
        assert!(updated.starts_with("# keep this comment\n"));
        assert!(updated.contains("# members comment"));
        assert!(updated.contains("\"src/my-orders\""));
        plan.restore().unwrap();
        assert_eq!(fs::read(fixture.0.join("eska.toml")).unwrap(), original);
    }

    #[test]
    fn enrollment_never_overwrites_a_concurrently_changed_manifest() {
        let (fixture, workspace, _original) = workspace();
        let name = ProjectName::parse("my-orders".to_owned()).unwrap();
        let plan = prepare(
            &workspace,
            fixture.0.join("src/my-orders"),
            PathBuf::from("src/my-orders"),
            &name,
        )
        .unwrap();
        fs::write(fixture.0.join("eska.toml"), "concurrent change\n").unwrap();
        assert!(matches!(
            plan.publish(),
            Err(WorkspaceEnrollmentError::ManifestChanged { .. })
        ));
        assert_eq!(
            fs::read_to_string(fixture.0.join("eska.toml")).unwrap(),
            "concurrent change\n"
        );
    }
}
