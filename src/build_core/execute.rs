//! Native build orchestration; filesystem preflight, staging and publication have separate owners.

use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::{
    ArtifactType, BuildPlan, BuildProvenance, Ibcmd, ManagedInfobase, ManagedInfobaseError,
    ManifestError, ProcessStream, RunError, locate_output_file, manifest,
};
use crate::project_core::source_format::{Error as SourceError, MappedFile, PreparedSource};
use crate::project_core::{Project, ProjectType, SourceFormat};

mod preflight;
mod publication;
mod workspace;

use preflight::{ensure_configured_output_is_inside_project, import_source};
pub use preflight::{preflight, preflight_manifest};
use publication::{publish, publish_pair};
use workspace::{CreatedDirectories, Workspace};

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
    BaseConfigurationRead { path: PathBuf, source: io::Error },
    BaseConfigurationInvalid(PathBuf),
    ManagedInfobase(ManagedInfobaseError),
    Manifest(ManifestError),
    SourceValidation(SourceError),
}

/// Build a native 1C artifact through a reusable managed file infobase.
///
/// The pipeline reuses a compatible file infobase or creates one with an optional base `.cf`,
/// then imports the Designer XML directory with `infobase config import --out`.
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
    execute_streaming_inner(plan, ibcmd, None, None, &mut |stage, stream, line, _| {
        on_output(stage, stream, line);
    })
}

/// Build from an immutable source snapshot and publish its artifact manifest as one pair.
///
/// # Errors
/// Returns a structured failure without replacing either previous result unless both new files
/// can be published.
pub fn execute_streaming_with_manifest<F>(
    plan: &BuildPlan,
    project: &Project,
    provenance: BuildProvenance,
    ibcmd: &Ibcmd,
    mut on_output: F,
) -> Result<BuildResult, BuildError>
where
    F: FnMut(BuildStage, ProcessStream, &[u8]),
{
    execute_streaming_inner(
        plan,
        ibcmd,
        Some((project, provenance)),
        None,
        &mut |stage, stream, line, _| on_output(stage, stream, line),
    )
}

/// Build exclusively from a protected generation and verify the actual copied input.
///
/// The working source and any stale XML directory are never used as fallback inputs.
/// Both artifact and manifest are published only after the verified snapshot has been built.
///
/// # Errors
/// Returns a structured failure for changed prepared bytes, a mismatched artifact profile,
/// source copying, platform execution or result publication.
pub fn execute_prepared_with_manifest<F>(
    plan: &BuildPlan,
    project: &Project,
    prepared: &PreparedSource,
    provenance: BuildProvenance,
    ibcmd: &Ibcmd,
    mut on_output: F,
) -> Result<BuildResult, BuildError>
where
    F: FnMut(BuildStage, ProcessStream, &[u8]),
{
    execute_prepared_with_locations(
        plan,
        project,
        prepared,
        provenance,
        ibcmd,
        |stage, stream, line, _| on_output(stage, stream, line),
    )
}

/// Report raw platform output and an optional file origin from the verified import snapshot.
///
/// Only recognized, unambiguous file addresses inside that snapshot receive an annotation.
/// Raw bytes and process failures remain available even when a message has no recognized address.
/// Metadata origins are composite; a file address alone does not identify a working property.
///
/// # Errors
/// Returns the same prepared-source, platform and publication failures as
/// [`execute_prepared_with_manifest`].
pub fn execute_prepared_with_locations<F>(
    plan: &BuildPlan,
    project: &Project,
    prepared: &PreparedSource,
    provenance: BuildProvenance,
    ibcmd: &Ibcmd,
    mut on_output: F,
) -> Result<BuildResult, BuildError>
where
    F: FnMut(BuildStage, ProcessStream, &[u8], Option<&MappedFile>),
{
    if !matches!(
        plan.artifact_type(),
        ArtifactType::Configuration | ArtifactType::Extension
    ) {
        return Err(BuildError::SourceValidation(SourceError::root(
            "source_profile_unsupported",
        )));
    }
    prepared.validate().map_err(BuildError::SourceValidation)?;
    let root = std::fs::read_to_string(prepared.xml().join("Configuration.xml")).map_err(|_| {
        BuildError::SourceValidation(SourceError::root("source_generation_corrupt"))
    })?;
    let expected = if plan.artifact_type() == ArtifactType::Extension {
        ProjectType::Extension
    } else {
        ProjectType::Configuration
    };
    if project.configuration().project_type() != expected
        || crate::project_core::native::project_type(&root)
            .ok()
            .flatten()
            != Some(expected)
    {
        return Err(BuildError::SourceValidation(SourceError::root(
            "source_profile_unsupported",
        )));
    }
    let native_plan = plan.with_snapshot_source(prepared.xml().to_owned());
    execute_streaming_inner(
        &native_plan,
        ibcmd,
        Some((project, provenance)),
        Some(prepared),
        &mut on_output,
    )
}

/// Share the verified platform pipeline between ordinary and manifested builds.
fn execute_streaming_inner<F>(
    plan: &BuildPlan,
    ibcmd: &Ibcmd,
    manifest_request: Option<(&Project, BuildProvenance)>,
    prepared: Option<&PreparedSource>,
    on_output: &mut F,
) -> Result<BuildResult, BuildError>
where
    F: FnMut(BuildStage, ProcessStream, &[u8], Option<&MappedFile>),
{
    let started = Instant::now();
    let (manifest_project, manifest_seed) = manifest_request.unzip();
    preflight_input(plan, prepared, manifest_project.is_some())?;
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
    let pid_file = workspace.path.join("ibcmd.pid");
    let artifact = workspace
        .path
        .join(format!("artifact.{}", plan.artifact_type().extension()));
    let snapshot = capture_snapshot(manifest_project, plan, &workspace.path, prepared)?;
    let effective_plan = snapshot.as_ref().map_or_else(
        || plan.clone(),
        |snapshot| plan.with_snapshot_source(snapshot.source().to_owned()),
    );
    let import_source = import_source(&effective_plan)?;
    let effective_base_configuration = prepare_base_configuration(
        plan.base_configuration(),
        manifest_project.is_some(),
        &workspace.path,
    )?;
    let infobase = ManagedInfobase::prepare(plan, effective_base_configuration.as_deref())
        .map_err(BuildError::ManagedInfobase)?;
    let mut observed_output = |stage, stream, line: &[u8]| {
        let location = prepared
            .filter(|_| stage == BuildStage::ImportSources)
            .and_then(|prepared| {
                locate_output_file(prepared.source_map(), effective_plan.source(), line)
            });
        on_output(stage, stream, line, location);
    };
    let create_output = if infobase.needs_creation() {
        let output = create_infobase(
            ibcmd,
            infobase.data(),
            effective_base_configuration.as_deref(),
            &pid_file,
            &mut observed_output,
        )?;
        infobase.mark_ready().map_err(BuildError::ManagedInfobase)?;
        Some(output)
    } else {
        None
    };
    let import_output = run(
        ibcmd,
        BuildStage::ImportSources,
        [
            OsString::from("infobase"),
            OsString::from("config"),
            OsString::from("import"),
            option("--data", infobase.data()),
            option("--out", &artifact),
            import_source.into_os_string(),
        ],
        &pid_file,
        &mut observed_output,
    )?;
    reject_interrupted_import(ibcmd)?;

    validate_artifact(&artifact)?;
    let manifest_path = publish_result(
        plan,
        ibcmd,
        &workspace.path,
        &artifact,
        manifest_seed,
        snapshot.as_ref(),
        effective_base_configuration.as_deref(),
    )?;
    created_directories.keep();
    let mut tool_output = Vec::new();
    if let Some(output) = create_output {
        append_process_output(&mut tool_output, &output.stdout);
        append_process_output(&mut tool_output, &output.stderr);
    }
    append_process_output(&mut tool_output, &import_output.stdout);
    append_process_output(&mut tool_output, &import_output.stderr);
    Ok(BuildResult {
        output: plan.output().to_owned(),
        manifest: manifest_path,
        duration: started.elapsed(),
        tool_output,
    })
}

/// Check proof presence and publication constraints before creating any build resources.
fn preflight_input(
    plan: &BuildPlan,
    prepared: Option<&PreparedSource>,
    manifested: bool,
) -> Result<(), BuildError> {
    if prepared.is_none() && !matches!(plan.source_format(), SourceFormat::DesignerXml) {
        return Err(BuildError::SourceValidation(SourceError::root(
            "source_preparation_required",
        )));
    }
    if manifested {
        preflight_manifest(plan)
    } else {
        preflight(plan)
    }
}

/// Bind the copied snapshot to protected evidence before any infobase is opened or changed.
fn capture_snapshot(
    project: Option<&Project>,
    plan: &BuildPlan,
    workspace: &Path,
    prepared: Option<&PreparedSource>,
) -> Result<Option<manifest::SourceSnapshot>, BuildError> {
    let mut snapshot = project
        .map(|project| manifest::capture_source(project, plan, workspace, prepared.is_some()))
        .transpose()
        .map_err(BuildError::Manifest)?;
    if let Some(prepared) = prepared {
        let snapshot = snapshot.as_mut().ok_or_else(|| {
            BuildError::SourceValidation(SourceError::root("source_snapshot_missing"))
        })?;
        prepared
            .validate_snapshot(snapshot.source())
            .map_err(BuildError::SourceValidation)?;
        prepared.validate().map_err(BuildError::SourceValidation)?;
        snapshot
            .bind_prepared(prepared)
            .map_err(BuildError::SourceValidation)?;
    }
    Ok(snapshot)
}

/// Require an ibcmd result to be a non-empty regular file before publication.
fn validate_artifact(artifact: &Path) -> Result<(), BuildError> {
    let metadata =
        fs::metadata(artifact).map_err(|_| BuildError::ArtifactMissing(artifact.to_owned()))?;
    if !metadata.is_file() {
        return Err(BuildError::ArtifactMissing(artifact.to_owned()));
    }
    if metadata.len() == 0 {
        return Err(BuildError::ArtifactEmpty(artifact.to_owned()));
    }
    Ok(())
}

/// Convert an interrupt observed after import into the stage-specific build error.
fn reject_interrupted_import(ibcmd: &Ibcmd) -> Result<(), BuildError> {
    if ibcmd.was_interrupted() {
        Err(BuildError::Run {
            stage: BuildStage::ImportSources,
            source: RunError::Interrupted,
        })
    } else {
        Ok(())
    }
}

/// Copy the base CF into a manifested build snapshot and otherwise retain its verified path.
fn prepare_base_configuration(
    source: Option<&Path>,
    manifested: bool,
    workspace: &Path,
) -> Result<Option<PathBuf>, BuildError> {
    if !manifested {
        return Ok(source.map(Path::to_path_buf));
    }
    source
        .map(|source| {
            let target = workspace.join("base.cf");
            match fs::copy(source, &target) {
                Ok(_) => Ok(target),
                Err(source) => Err(BuildError::Manifest(ManifestError::SnapshotIo {
                    path: target,
                    source,
                })),
            }
        })
        .transpose()
}

/// Create the isolated infobase and optionally load the external artifact context.
fn create_infobase<F>(
    ibcmd: &Ibcmd,
    data: &Path,
    base_configuration: Option<&Path>,
    pid_file: &Path,
    on_output: &mut F,
) -> Result<std::process::Output, BuildError>
where
    F: FnMut(BuildStage, ProcessStream, &[u8]),
{
    let mut arguments = vec![
        OsString::from("infobase"),
        OsString::from("create"),
        option("--data", data),
    ];
    if let Some(base_configuration) = base_configuration {
        arguments.push(option("--load", base_configuration));
    }
    run(
        ibcmd,
        BuildStage::CreateInfobase,
        arguments,
        pid_file,
        on_output,
    )
}

/// Publish either one ordinary artifact or one checksummed artifact/manifest pair.
fn publish_result(
    plan: &BuildPlan,
    ibcmd: &Ibcmd,
    workspace: &Path,
    artifact: &Path,
    manifest_seed: Option<BuildProvenance>,
    snapshot: Option<&manifest::SourceSnapshot>,
    base_configuration: Option<&Path>,
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
        base_configuration,
    )
    .map_err(BuildError::Manifest)?;
    let destination = manifest::path_for_artifact(plan.output());
    publish_pair(artifact, plan.output(), &staged, &destination)?;
    Ok(Some(destination))
}

/// Run one ibcmd stage and retain only its stable stage plus diagnostic stderr.
fn run<I, S, F>(
    ibcmd: &Ibcmd,
    stage: BuildStage,
    arguments: I,
    pid_file: &Path,
    on_output: &mut F,
) -> Result<std::process::Output, BuildError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
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

/// Create `--name=path` without requiring the path to be UTF-8.
fn option(name: &str, path: &Path) -> OsString {
    let mut argument = OsString::from(name);
    argument.push(OsStr::new("="));
    argument.push(path);
    argument
}
