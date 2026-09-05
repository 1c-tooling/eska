use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use super::{ArtifactType, BuildPlan, Ibcmd, RunError};
use crate::project::{ProjectType, designer_xml};

static WORKSPACE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildStage {
    CreateInfobase,
    ImportSources,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildResult {
    output: PathBuf,
    duration: Duration,
    tool_output: Vec<u8>,
}

impl BuildResult {
    #[must_use]
    pub fn output(&self) -> &Path {
        &self.output
    }

    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    #[must_use]
    pub fn tool_output(&self) -> &[u8] {
        &self.tool_output
    }
}

#[derive(Debug)]
pub enum BuildError {
    OutputParentMissing(PathBuf),
    ConfiguredOutputOutsideProject { root: PathBuf, output: PathBuf },
    InvalidExistingOutput(PathBuf),
    CreateDirectory { path: PathBuf, source: io::Error },
    CreateWorkspace { path: PathBuf, source: io::Error },
    Run { stage: BuildStage, source: RunError },
    CommandFailed { stage: BuildStage, stderr: String },
    ArtifactMissing(PathBuf),
    ArtifactEmpty(PathBuf),
    Publish { path: PathBuf, source: io::Error },
    Restore { path: PathBuf, source: io::Error },
    DescriptorDirectory { path: PathBuf, source: io::Error },
    DescriptorRead { path: PathBuf, source: io::Error },
    DescriptorInvalid { path: PathBuf },
    DescriptorMissing(PathBuf),
    DescriptorsMultiple(PathBuf),
}

/// Build a native 1C artifact through an isolated temporary file infobase.
///
/// The verified pipeline is identical for `.cf`, `.cfe`, `.epf`, and `.erf`:
/// create a file infobase, then import Designer XML with `config import --out`.
/// The destination is replaced only after `ibcmd` produced a non-empty file.
///
/// # Errors
/// Returns a structured error for unsafe output resolution, process failures,
/// invalid generated artifacts, or publication failures.
pub fn execute(plan: &BuildPlan, ibcmd: &Ibcmd) -> Result<BuildResult, BuildError> {
    let started = Instant::now();
    ibcmd
        .begin_interruptible_operation()
        .map_err(|source| BuildError::Run {
            stage: BuildStage::CreateInfobase,
            source,
        })?;
    let parent = plan
        .output()
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| BuildError::OutputParentMissing(plan.output().to_owned()))?;
    if !plan.has_explicit_output() {
        ensure_existing_output_ancestor_is_inside_project(plan, parent)?;
    }
    let mut created_directories = CreatedDirectories::create(parent)?;
    if !plan.has_explicit_output() {
        ensure_configured_output_is_inside_project(plan, parent)?;
    }
    validate_existing_output(plan.output())?;

    let workspace = Workspace::create(parent)?;
    let data = workspace.path.join("data");
    let pid_file = workspace.path.join("ibcmd.pid");
    let artifact = workspace
        .path
        .join(format!("artifact.{}", plan.artifact_type().extension()));
    let import_source = import_source(plan)?;

    let create_output = run(
        ibcmd,
        BuildStage::CreateInfobase,
        [
            OsString::from("infobase"),
            OsString::from("create"),
            option("--data", &data),
        ],
        &pid_file,
    )?;
    let import_output = run(
        ibcmd,
        BuildStage::ImportSources,
        [
            OsString::from("config"),
            OsString::from("import"),
            option("--data", &data),
            option("--out", &artifact),
            import_source.into_os_string(),
        ],
        &pid_file,
    )?;
    if ibcmd.was_interrupted() {
        return Err(BuildError::Run {
            stage: BuildStage::ImportSources,
            source: RunError::Interrupted,
        });
    }

    let metadata =
        fs::metadata(&artifact).map_err(|_| BuildError::ArtifactMissing(artifact.clone()))?;
    if !metadata.is_file() {
        return Err(BuildError::ArtifactMissing(artifact));
    }
    if metadata.len() == 0 {
        return Err(BuildError::ArtifactEmpty(artifact));
    }
    publish(&artifact, plan.output())?;
    created_directories.keep();
    let mut tool_output = Vec::new();
    append_process_output(&mut tool_output, &create_output.stdout);
    append_process_output(&mut tool_output, &create_output.stderr);
    append_process_output(&mut tool_output, &import_output.stdout);
    append_process_output(&mut tool_output, &import_output.stderr);
    Ok(BuildResult {
        output: plan.output().to_owned(),
        duration: started.elapsed(),
        tool_output,
    })
}

/// Resolve the source argument expected by ibcmd for each native artifact kind.
fn import_source(plan: &BuildPlan) -> Result<PathBuf, BuildError> {
    if matches!(
        plan.artifact_type(),
        ArtifactType::Configuration | ArtifactType::Extension
    ) {
        return Ok(plan.source().to_owned());
    }
    let expected = match plan.artifact_type() {
        ArtifactType::Processing => ProjectType::Processing,
        ArtifactType::Report => ProjectType::Report,
        ArtifactType::Configuration | ArtifactType::Extension => unreachable!(),
    };
    let entries =
        fs::read_dir(plan.source()).map_err(|source| BuildError::DescriptorDirectory {
            path: plan.source().to_owned(),
            source,
        })?;
    let mut descriptors = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| BuildError::DescriptorDirectory {
            path: plan.source().to_owned(),
            source,
        })?;
        let path = entry.path();
        if !entry
            .file_type()
            .map_err(|source| BuildError::DescriptorRead {
                path: path.clone(),
                source,
            })?
            .is_file()
            || path.extension() != Some(OsStr::new("xml"))
        {
            continue;
        }
        let contents = fs::read_to_string(&path).map_err(|source| BuildError::DescriptorRead {
            path: path.clone(),
            source,
        })?;
        let project_type = designer_xml::project_type(&contents)
            .map_err(|_| BuildError::DescriptorInvalid { path: path.clone() })?;
        if project_type == Some(expected) {
            descriptors.push(path);
        }
    }
    match descriptors.as_slice() {
        [descriptor] => Ok(descriptor.clone()),
        [] => Err(BuildError::DescriptorMissing(plan.source().to_owned())),
        _ => Err(BuildError::DescriptorsMultiple(plan.source().to_owned())),
    }
}

/// Run one ibcmd stage and retain only its stable stage plus diagnostic stderr.
fn run<const N: usize>(
    ibcmd: &Ibcmd,
    stage: BuildStage,
    arguments: [OsString; N],
    pid_file: &Path,
) -> Result<std::process::Output, BuildError> {
    let output = ibcmd
        .run_interruptible(arguments, pid_file)
        .map_err(|source| BuildError::Run { stage, source })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(BuildError::CommandFailed {
            stage,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

/// Append non-empty process output with a stable newline boundary between streams.
fn append_process_output(target: &mut Vec<u8>, output: &[u8]) {
    if output.is_empty() {
        return;
    }
    if !target.is_empty() && !target.ends_with(b"\n") {
        target.push(b'\n');
    }
    target.extend_from_slice(output);
}

/// Validate the nearest existing configured-output ancestor before creating directories.
fn ensure_existing_output_ancestor_is_inside_project(
    plan: &BuildPlan,
    parent: &Path,
) -> Result<(), BuildError> {
    let root =
        fs::canonicalize(plan.project_root()).map_err(|source| BuildError::CreateDirectory {
            path: plan.project_root().to_owned(),
            source,
        })?;
    let mut ancestor = parent;
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| BuildError::OutputParentMissing(parent.to_owned()))?;
    }
    let resolved = fs::canonicalize(ancestor).map_err(|source| BuildError::CreateDirectory {
        path: ancestor.to_owned(),
        source,
    })?;
    if resolved.starts_with(&root) {
        Ok(())
    } else {
        Err(BuildError::ConfiguredOutputOutsideProject {
            root,
            output: resolved,
        })
    }
}

/// Create `--name=path` without requiring the path to be UTF-8.
fn option(name: &str, path: &Path) -> OsString {
    let mut argument = OsString::from(name);
    argument.push(OsStr::new("="));
    argument.push(path);
    argument
}

/// Prevent a configured relative artifacts directory from escaping through a symlink.
fn ensure_configured_output_is_inside_project(
    plan: &BuildPlan,
    parent: &Path,
) -> Result<(), BuildError> {
    let root =
        fs::canonicalize(plan.project_root()).map_err(|source| BuildError::CreateDirectory {
            path: plan.project_root().to_owned(),
            source,
        })?;
    let parent = fs::canonicalize(parent).map_err(|source| BuildError::CreateDirectory {
        path: parent.to_owned(),
        source,
    })?;
    if parent.starts_with(&root) {
        Ok(())
    } else {
        Err(BuildError::ConfiguredOutputOutsideProject {
            root,
            output: parent,
        })
    }
}

/// Refuse links and special files as replacement targets.
fn validate_existing_output(output: &Path) -> Result<(), BuildError> {
    match fs::symlink_metadata(output) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(_) => Err(BuildError::InvalidExistingOutput(output.to_owned())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(BuildError::Publish {
            path: output.to_owned(),
            source: error,
        }),
    }
}

/// Replace an existing regular artifact while restoring it if publication fails.
fn publish(artifact: &Path, output: &Path) -> Result<(), BuildError> {
    if !output.exists() {
        fs::hard_link(artifact, output).map_err(|source| BuildError::Publish {
            path: output.to_owned(),
            source,
        })?;
        return fs::remove_file(artifact).map_err(|source| BuildError::Publish {
            path: artifact.to_owned(),
            source,
        });
    }
    let backup = create_backup_link(output)?;
    if let Err(source) = fs::remove_file(output) {
        let _ = fs::remove_file(&backup);
        return Err(BuildError::Publish {
            path: output.to_owned(),
            source,
        });
    }
    if let Err(source) = fs::hard_link(artifact, output) {
        restore_backup(&backup, output)?;
        return Err(BuildError::Publish {
            path: output.to_owned(),
            source,
        });
    }
    fs::remove_file(artifact).map_err(|source| BuildError::Publish {
        path: artifact.to_owned(),
        source,
    })?;
    fs::remove_file(&backup).map_err(|source| BuildError::Publish {
        path: backup,
        source,
    })
}

/// Reserve a collision-free hard-link backup without replacing unrelated files.
fn create_backup_link(output: &Path) -> Result<PathBuf, BuildError> {
    for _ in 0..32 {
        let backup = unique_sibling(output, "backup");
        match fs::hard_link(output, &backup) {
            Ok(()) => return Ok(backup),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(BuildError::Publish {
                    path: output.to_owned(),
                    source,
                });
            }
        }
    }
    Err(BuildError::Publish {
        path: output.to_owned(),
        source: io::Error::new(io::ErrorKind::AlreadyExists, "backup name exhausted"),
    })
}

/// Restore an existing artifact from its owned hard-link backup.
fn restore_backup(backup: &Path, output: &Path) -> Result<(), BuildError> {
    fs::hard_link(backup, output).map_err(|source| BuildError::Restore {
        path: output.to_owned(),
        source,
    })?;
    fs::remove_file(backup).map_err(|source| BuildError::Restore {
        path: backup.to_owned(),
        source,
    })
}

/// Produce a collision-resistant sibling name without inspecting user content.
fn unique_sibling(path: &Path, label: &str) -> PathBuf {
    let sequence = WORKSPACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".eska-{label}-{}-{sequence}", std::process::id()));
    path.with_file_name(name)
}

struct CreatedDirectories {
    paths: Vec<PathBuf>,
    keep: bool,
}

impl CreatedDirectories {
    /// Create the output hierarchy and remember only directories owned by this run.
    fn create(parent: &Path) -> Result<Self, BuildError> {
        let mut paths = Vec::new();
        let mut current = parent;
        while !current.exists() {
            paths.push(current.to_owned());
            current = current
                .parent()
                .ok_or_else(|| BuildError::OutputParentMissing(parent.to_owned()))?;
        }
        fs::create_dir_all(parent).map_err(|source| BuildError::CreateDirectory {
            path: parent.to_owned(),
            source,
        })?;
        Ok(Self { paths, keep: false })
    }

    /// Preserve the output directories after successful artifact publication.
    const fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for CreatedDirectories {
    /// Roll back only empty directories created by the active build.
    fn drop(&mut self) {
        if !self.keep {
            for directory in &self.paths {
                let _ = fs::remove_dir(directory);
            }
        }
    }
}

struct Workspace {
    path: PathBuf,
}

impl Workspace {
    /// Create one private workspace next to the destination for same-filesystem publication.
    fn create(parent: &Path) -> Result<Self, BuildError> {
        for _ in 0..32 {
            let path = unique_sibling(&parent.join("build"), "work");
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(source) => return Err(BuildError::CreateWorkspace { path, source }),
            }
        }
        let path = parent.join(".eska-build-work");
        Err(BuildError::CreateWorkspace {
            path,
            source: io::Error::new(io::ErrorKind::AlreadyExists, "workspace name exhausted"),
        })
    }
}

impl Drop for Workspace {
    /// Remove the complete temporary infobase and any unpublished artifact.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
