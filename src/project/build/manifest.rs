use std::{
    ffi::OsStr,
    fmt::Write as _,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{ArtifactType, BuildPlan};
use crate::{
    project::{Project, ProjectPathError, version},
    vcs::repository::{Error as RepositoryError, Head, Repository},
};

#[derive(Debug)]
pub enum ManifestError {
    SnapshotIo { path: PathBuf, source: io::Error },
    SnapshotEntryUnsupported(PathBuf),
    SnapshotProject(ProjectPathError),
    ArtifactRead { path: PathBuf, source: io::Error },
    Write { path: PathBuf, source: io::Error },
    Serialize(serde_json::Error),
}

pub(super) struct ManifestSeed {
    project_name: Option<String>,
    git: GitDocument,
}

impl ManifestSeed {
    /// Capture repository metadata before temporary build paths can affect Git status.
    #[must_use]
    pub(super) fn capture(project: &Project, project_name: Option<&str>) -> Self {
        Self {
            project_name: project_name.map(str::to_owned),
            git: inspect_git(project.root()),
        }
    }
}

pub(super) struct SourceSnapshot {
    source: PathBuf,
    id: String,
    version: VersionDocument,
}

impl SourceSnapshot {
    #[must_use]
    pub(super) fn source(&self) -> &Path {
        &self.source
    }
}

/// Return the stable sibling path used for an artifact manifest.
#[must_use]
pub fn path_for_artifact(artifact: &Path) -> PathBuf {
    let mut name = artifact.file_name().unwrap_or_default().to_os_string();
    name.push(".manifest.json");
    artifact.with_file_name(name)
}

/// Copy the exact Designer source tree that will be passed to ibcmd and identify its bytes.
///
/// # Errors
/// Returns a structured error for unreadable entries, failed copies, unsupported links or an
/// invalid isolated project path.
pub(super) fn capture_source(
    project: &Project,
    plan: &BuildPlan,
    workspace: &Path,
) -> Result<SourceSnapshot, ManifestError> {
    let snapshot_root = workspace.join("snapshot");
    let snapshot_source = snapshot_root.join("source");
    fs::create_dir(&snapshot_root).map_err(|source| ManifestError::SnapshotIo {
        path: snapshot_root.clone(),
        source,
    })?;
    let manifest_path = path_for_artifact(plan.output());
    let excluded = [
        plan.artifacts_directory(),
        plan.output(),
        manifest_path.as_path(),
        workspace,
    ];
    let mut hash = Sha256::new();
    copy_directory(
        plan.source(),
        &snapshot_source,
        plan.source(),
        &excluded,
        &mut hash,
    )?;
    let snapshot_project = Project::new(
        snapshot_root,
        snapshot_source.clone(),
        project.configuration().clone(),
    )
    .map_err(ManifestError::SnapshotProject)?;
    let version = version::inspect(&snapshot_project).map_or_else(
        |_| VersionDocument::unavailable(),
        |info| VersionDocument::available(info.version.to_string()),
    );
    Ok(SourceSnapshot {
        source: snapshot_source,
        id: format!("sha256:{}", hex(&hash.finalize())),
        version,
    })
}

/// Serialize a manifest for the staged artifact and the source snapshot used to build it.
///
/// # Errors
/// Returns a structured error when the artifact cannot be read or JSON serialization fails.
pub(super) fn write(
    path: &Path,
    artifact: &Path,
    artifact_type: ArtifactType,
    platform_version: &str,
    seed: ManifestSeed,
    snapshot: &SourceSnapshot,
    base_configuration: Option<&Path>,
) -> Result<(), ManifestError> {
    let document = ArtifactManifestDocument {
        schema_version: 1,
        kind: "artifact-manifest",
        project: ProjectDocument {
            name: seed.project_name,
            version: snapshot.version.clone(),
        },
        artifact: ArtifactDocument {
            r#type: artifact_type.as_str(),
            checksum: ChecksumDocument::sha256(file_checksum(artifact)?),
        },
        platform: PlatformDocument {
            version: platform_version.to_owned(),
        },
        source: SourceDocument {
            snapshot_id: snapshot.id.clone(),
            git: seed.git,
            base_configuration: base_configuration
                .map(file_checksum)
                .transpose()?
                .map(ChecksumDocument::sha256),
        },
    };
    let bytes = serde_json::to_vec_pretty(&document).map_err(ManifestError::Serialize)?;
    let mut file = File::create(path).map_err(|source| ManifestError::Write {
        path: path.to_owned(),
        source,
    })?;
    file.write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .map_err(|source| ManifestError::Write {
            path: path.to_owned(),
            source,
        })
}

/// Copy one directory without following links and hash its exact relative structure and bytes.
fn copy_directory(
    source: &Path,
    target: &Path,
    root: &Path,
    excluded: &[&Path],
    hash: &mut Sha256,
) -> Result<(), ManifestError> {
    fs::create_dir(target).map_err(|source| ManifestError::SnapshotIo {
        path: target.to_owned(),
        source,
    })?;
    let mut entries = fs::read_dir(source)
        .map_err(|source_error| ManifestError::SnapshotIo {
            path: source.to_owned(),
            source: source_error,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source_error| ManifestError::SnapshotIo {
            path: source.to_owned(),
            source: source_error,
        })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if entry.file_name() == OsStr::new(".git")
            || excluded
                .iter()
                .any(|excluded| path == **excluded || path.starts_with(excluded))
        {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("directory entries remain below the source root");
        let destination = target.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|source| ManifestError::SnapshotIo {
                path: path.clone(),
                source,
            })?;
        if file_type.is_dir() {
            hash_entry(hash, b'd', relative, &[]);
            copy_directory(&path, &destination, root, excluded, hash)?;
            copy_permissions(&path, &destination)?;
        } else if file_type.is_file() {
            let digest = copy_file(&path, &destination)?;
            hash_entry(hash, b'f', relative, &digest);
        } else {
            return Err(ManifestError::SnapshotEntryUnsupported(path));
        }
    }
    copy_permissions(source, target)
}

/// Stream one file once so copied bytes and their checksum cannot diverge.
fn copy_file(source: &Path, target: &Path) -> Result<Vec<u8>, ManifestError> {
    let mut input = File::open(source).map_err(|error| ManifestError::SnapshotIo {
        path: source.to_owned(),
        source: error,
    })?;
    let mut output = File::create(target).map_err(|error| ManifestError::SnapshotIo {
        path: target.to_owned(),
        source: error,
    })?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| ManifestError::SnapshotIo {
                path: source.to_owned(),
                source: error,
            })?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| ManifestError::SnapshotIo {
                path: target.to_owned(),
                source: error,
            })?;
        hash.update(&buffer[..read]);
    }
    copy_permissions(source, target)?;
    Ok(hash.finalize().to_vec())
}

/// Preserve permissions only after a copied directory has been populated.
fn copy_permissions(source: &Path, target: &Path) -> Result<(), ManifestError> {
    let permissions = fs::metadata(source)
        .map_err(|error| ManifestError::SnapshotIo {
            path: source.to_owned(),
            source: error,
        })?
        .permissions();
    fs::set_permissions(target, permissions).map_err(|error| ManifestError::SnapshotIo {
        path: target.to_owned(),
        source: error,
    })
}

/// Hash an unambiguous entry record independent of traversal buffering.
fn hash_entry(hash: &mut Sha256, kind: u8, path: &Path, digest: &[u8]) {
    let encoded = path_bytes(path);
    hash.update([kind]);
    hash.update((encoded.len() as u64).to_le_bytes());
    hash.update(encoded);
    hash.update((digest.len() as u64).to_le_bytes());
    hash.update(digest);
}

#[cfg(unix)]
fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(windows)]
fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Read and hash a completed artifact without loading it into memory.
fn file_checksum(path: &Path) -> Result<String, ManifestError> {
    let mut file = File::open(path).map_err(|source| ManifestError::ArtifactRead {
        path: path.to_owned(),
        source,
    })?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| ManifestError::ArtifactRead {
                path: path.to_owned(),
                source,
            })?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(hex(&hash.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

/// Treat missing Git separately while keeping broken repository details out of the manifest.
fn inspect_git(root: &Path) -> GitDocument {
    let repository = match Repository::discover(root) {
        Ok(repository) => repository,
        Err(RepositoryError::NotFound { .. }) => return GitDocument::absent(),
        Err(_) => return GitDocument::unavailable(),
    };
    let commit = match repository.head() {
        Ok(Head::Unborn { .. }) => None,
        Ok(head) => head.id().map(|id| id.to_string()),
        Err(_) => return GitDocument::unavailable(),
    };
    let dirty = match repository.status() {
        Ok(status) => status.is_dirty(),
        Err(_) => return GitDocument::unavailable(),
    };
    GitDocument::available(commit, dirty)
}

#[derive(Clone, Serialize)]
struct VersionDocument {
    status: &'static str,
    value: Option<String>,
}

impl VersionDocument {
    const fn available(value: String) -> Self {
        Self {
            status: "available",
            value: Some(value),
        }
    }

    const fn unavailable() -> Self {
        Self {
            status: "unavailable",
            value: None,
        }
    }
}

#[derive(Serialize)]
struct ArtifactManifestDocument {
    schema_version: u8,
    kind: &'static str,
    project: ProjectDocument,
    artifact: ArtifactDocument,
    platform: PlatformDocument,
    source: SourceDocument,
}

#[derive(Serialize)]
struct ProjectDocument {
    name: Option<String>,
    version: VersionDocument,
}

#[derive(Serialize)]
struct ArtifactDocument {
    r#type: &'static str,
    checksum: ChecksumDocument,
}

#[derive(Serialize)]
struct ChecksumDocument {
    algorithm: &'static str,
    value: String,
}

impl ChecksumDocument {
    const fn sha256(value: String) -> Self {
        Self {
            algorithm: "sha256",
            value,
        }
    }
}

#[derive(Serialize)]
struct PlatformDocument {
    version: String,
}

#[derive(Serialize)]
struct SourceDocument {
    snapshot_id: String,
    git: GitDocument,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_configuration: Option<ChecksumDocument>,
}

#[derive(Serialize)]
struct GitDocument {
    status: &'static str,
    commit: Option<String>,
    dirty: Option<bool>,
}

impl GitDocument {
    const fn available(commit: Option<String>, dirty: bool) -> Self {
        Self {
            status: "available",
            commit,
            dirty: Some(dirty),
        }
    }

    const fn absent() -> Self {
        Self {
            status: "absent",
            commit: None,
            dirty: None,
        }
    }

    const fn unavailable() -> Self {
        Self {
            status: "unavailable",
            commit: None,
            dirty: None,
        }
    }
}
