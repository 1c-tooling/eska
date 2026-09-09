use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use super::{ArtifactType, BuildPlan, Ibcmd, ManifestError, ProcessStream, RunError, manifest};
use crate::project::{Project, ProjectType, designer_xml};

static WORKSPACE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildStage {
    CreateInfobase,
    ImportSources,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildResult {
    output: PathBuf,
    manifest: Option<PathBuf>,
    duration: Duration,
    tool_output: Vec<u8>,
}

impl BuildResult {
    #[must_use]
    pub fn output(&self) -> &Path {
        &self.output
    }

    #[must_use]
    pub fn manifest(&self) -> Option<&Path> {
        self.manifest.as_deref()
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
    Manifest(ManifestError),
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
    execute_streaming(plan, ibcmd, |_, _, _| {})
}

/// Build an artifact while reporting each complete ibcmd output line immediately.
///
/// # Errors
/// Returns the same structured failures as [`execute`].
pub fn execute_streaming<F>(
    plan: &BuildPlan,
    ibcmd: &Ibcmd,
    mut on_output: F,
) -> Result<BuildResult, BuildError>
where
    F: FnMut(BuildStage, ProcessStream, &[u8]),
{
    execute_streaming_inner(plan, ibcmd, None, &mut on_output)
}

/// Build from an immutable source snapshot and publish its artifact manifest as one pair.
///
/// # Errors
/// Returns a structured failure without replacing either previous result unless both new files
/// can be published.
pub fn execute_streaming_with_manifest<F>(
    plan: &BuildPlan,
    project: &Project,
    project_name: Option<&str>,
    ibcmd: &Ibcmd,
    mut on_output: F,
) -> Result<BuildResult, BuildError>
where
    F: FnMut(BuildStage, ProcessStream, &[u8]),
{
    execute_streaming_inner(plan, ibcmd, Some((project, project_name)), &mut on_output)
}

/// Share the verified platform pipeline between ordinary and manifested builds.
fn execute_streaming_inner<F>(
    plan: &BuildPlan,
    ibcmd: &Ibcmd,
    manifest_request: Option<(&Project, Option<&str>)>,
    on_output: &mut F,
) -> Result<BuildResult, BuildError>
where
    F: FnMut(BuildStage, ProcessStream, &[u8]),
{
    let started = Instant::now();
    if manifest_request.is_some() {
        preflight_manifest(plan)?;
    } else {
        preflight(plan)?;
    }
    let manifest_seed =
        manifest_request.map(|(project, name)| manifest::ManifestSeed::capture(project, name));
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
    let mut created_directories = CreatedDirectories::create(parent)?;
    if !plan.has_explicit_output() {
        ensure_configured_output_is_inside_project(plan, parent)?;
    }
    let workspace = Workspace::create(parent)?;
    let data = workspace.path.join("data");
    let pid_file = workspace.path.join("ibcmd.pid");
    let artifact = workspace
        .path
        .join(format!("artifact.{}", plan.artifact_type().extension()));
    let snapshot = manifest_request
        .map(|(project, _)| manifest::capture_source(project, plan, &workspace.path))
        .transpose()
        .map_err(BuildError::Manifest)?;
    let effective_plan = snapshot.as_ref().map_or_else(
        || plan.clone(),
        |snapshot| plan.with_snapshot_source(snapshot.source().to_owned()),
    );
    let import_source = import_source(&effective_plan)?;

    let create_output = run(
        ibcmd,
        BuildStage::CreateInfobase,
        [
            OsString::from("infobase"),
            OsString::from("create"),
            option("--data", &data),
        ],
        &pid_file,
        on_output,
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
        on_output,
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
    let manifest_path = publish_result(
        plan,
        ibcmd,
        &workspace.path,
        &artifact,
        manifest_seed,
        snapshot.as_ref(),
    )?;
    created_directories.keep();
    let mut tool_output = Vec::new();
    append_process_output(&mut tool_output, &create_output.stdout);
    append_process_output(&mut tool_output, &create_output.stderr);
    append_process_output(&mut tool_output, &import_output.stdout);
    append_process_output(&mut tool_output, &import_output.stderr);
    Ok(BuildResult {
        output: plan.output().to_owned(),
        manifest: manifest_path,
        duration: started.elapsed(),
        tool_output,
    })
}

/// Publish either one ordinary artifact or one checksummed artifact/manifest pair.
fn publish_result(
    plan: &BuildPlan,
    ibcmd: &Ibcmd,
    workspace: &Path,
    artifact: &Path,
    manifest_seed: Option<manifest::ManifestSeed>,
    snapshot: Option<&manifest::SourceSnapshot>,
) -> Result<Option<PathBuf>, BuildError> {
    let Some((seed, snapshot)) = manifest_seed.zip(snapshot) else {
        publish(artifact, plan.output())?;
        return Ok(None);
    };
    let staged = workspace.join("artifact.manifest.json");
    manifest::write(
        &staged,
        artifact,
        plan.artifact_type(),
        ibcmd.version().as_str(),
        seed,
        snapshot,
    )
    .map_err(BuildError::Manifest)?;
    let destination = manifest::path_for_artifact(plan.output());
    publish_pair(artifact, plan.output(), &staged, &destination)?;
    Ok(Some(destination))
}

/// Validate every filesystem input and output invariant without creating files or invoking ibcmd.
///
/// # Errors
/// Returns a structured source, descriptor, or output error.
pub fn preflight(plan: &BuildPlan) -> Result<(), BuildError> {
    let parent = plan
        .output()
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| BuildError::OutputParentMissing(plan.output().to_owned()))?;
    if !plan.has_explicit_output() {
        ensure_existing_output_ancestor_is_inside_project(plan, parent)?;
    }
    validate_existing_output(plan.output())?;
    import_source(plan).map(|_| ())
}

/// Validate both destinations required by a manifested build without creating files.
///
/// # Errors
/// Returns the same output errors as execution for either the artifact or its manifest.
pub fn preflight_manifest(plan: &BuildPlan) -> Result<(), BuildError> {
    preflight(plan)?;
    validate_existing_output(&manifest::path_for_artifact(plan.output()))
}

/// Resolve the source argument expected by ibcmd for each native artifact kind.
fn import_source(plan: &BuildPlan) -> Result<PathBuf, BuildError> {
    if matches!(
        plan.artifact_type(),
        ArtifactType::Configuration | ArtifactType::Extension
    ) {
        fs::read_dir(plan.source()).map_err(|source| BuildError::DescriptorDirectory {
            path: plan.source().to_owned(),
            source,
        })?;
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
fn run<const N: usize, F>(
    ibcmd: &Ibcmd,
    stage: BuildStage,
    arguments: [OsString; N],
    pid_file: &Path,
    on_output: &mut F,
) -> Result<std::process::Output, BuildError>
where
    F: FnMut(BuildStage, ProcessStream, &[u8]),
{
    let output = ibcmd
        .run_interruptible(arguments, pid_file, &mut |stream, output| {
            on_output(stage, stream, output);
        })
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
    let root = fs::canonicalize(plan.output_scope_root()).map_err(|source| {
        BuildError::CreateDirectory {
            path: plan.output_scope_root().to_owned(),
            source,
        }
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
    let root = fs::canonicalize(plan.output_scope_root()).map_err(|source| {
        BuildError::CreateDirectory {
            path: plan.output_scope_root().to_owned(),
            source,
        }
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

/// Publish an artifact and manifest together, restoring the previous pair on failure.
fn publish_pair(
    artifact: &Path,
    output: &Path,
    staged_manifest: &Path,
    manifest: &Path,
) -> Result<(), BuildError> {
    let output_backup = optional_backup(output)?;
    let manifest_backup = match optional_backup(manifest) {
        Ok(backup) => backup,
        Err(error) => {
            remove_backup(output_backup.as_deref());
            return Err(error);
        }
    };
    if let Err(error) = remove_existing(output).and_then(|()| remove_existing(manifest)) {
        restore_pair(
            output,
            output_backup.as_deref(),
            manifest,
            manifest_backup.as_deref(),
        )?;
        return Err(error);
    }
    if let Err(source) = fs::hard_link(artifact, output) {
        restore_pair(
            output,
            output_backup.as_deref(),
            manifest,
            manifest_backup.as_deref(),
        )?;
        return Err(BuildError::Publish {
            path: output.to_owned(),
            source,
        });
    }
    if let Err(source) = fs::hard_link(staged_manifest, manifest) {
        restore_pair(
            output,
            output_backup.as_deref(),
            manifest,
            manifest_backup.as_deref(),
        )?;
        return Err(BuildError::Publish {
            path: manifest.to_owned(),
            source,
        });
    }
    let _ = fs::remove_file(artifact);
    let _ = fs::remove_file(staged_manifest);
    remove_backup(output_backup.as_deref());
    remove_backup(manifest_backup.as_deref());
    Ok(())
}

/// Back up a regular destination by hard link while leaving missing paths absent.
fn optional_backup(path: &Path) -> Result<Option<PathBuf>, BuildError> {
    if path.exists() {
        create_backup_link(path).map(Some)
    } else {
        Ok(None)
    }
}

/// Remove one destination that preflight established as regular or absent.
fn remove_existing(path: &Path) -> Result<(), BuildError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(BuildError::Publish {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Remove partial new files and restore every destination that existed before publication.
fn restore_pair(
    output: &Path,
    output_backup: Option<&Path>,
    manifest: &Path,
    manifest_backup: Option<&Path>,
) -> Result<(), BuildError> {
    let _ = fs::remove_file(output);
    let _ = fs::remove_file(manifest);
    restore_optional(output_backup, output)?;
    restore_optional(manifest_backup, manifest)
}

fn restore_optional(backup: Option<&Path>, destination: &Path) -> Result<(), BuildError> {
    let Some(backup) = backup else {
        return Ok(());
    };
    fs::hard_link(backup, destination).map_err(|source| BuildError::Restore {
        path: destination.to_owned(),
        source,
    })?;
    let _ = fs::remove_file(backup);
    Ok(())
}

fn remove_backup(backup: Option<&Path>) {
    if let Some(backup) = backup {
        let _ = fs::remove_file(backup);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Leave both previous destinations untouched when the manifest cannot be backed up.
    fn pair_publication_preserves_previous_results_on_failure() {
        let root = std::env::temp_dir().join(format!(
            "eska-pair-publication-{}-{}",
            std::process::id(),
            WORKSPACE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("test directory");
        let artifact = root.join("staged.cf");
        let staged_manifest = root.join("staged.json");
        let output = root.join("result.cf");
        let manifest = root.join("result.cf.manifest.json");
        fs::write(&artifact, b"new artifact").expect("staged artifact");
        fs::write(&staged_manifest, b"new manifest").expect("staged manifest");
        fs::write(&output, b"old artifact").expect("old artifact");
        fs::create_dir(&manifest).expect("invalid manifest target");

        assert!(publish_pair(&artifact, &output, &staged_manifest, &manifest).is_err());
        assert_eq!(
            fs::read(&output).expect("preserved artifact"),
            b"old artifact"
        );
        assert!(manifest.is_dir());
        fs::remove_dir_all(root).expect("remove test directory");
    }
}
