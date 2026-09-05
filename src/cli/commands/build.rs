//! Localized build command with a locale-independent JSON result contract.

use std::{
    ffi::OsStr,
    fmt::Write as _,
    io::{self, Write as _},
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Args, ValueEnum};
use serde::Serialize;

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        build::{
            self, BuildError, BuildPlan, BuildStage, Ibcmd, PlanError, RunError, ToolError,
            ToolOptions, ToolSource,
        },
        discovery,
    },
};

#[derive(Debug, Args)]
pub(in crate::cli) struct BuildArgs {
    #[arg(long)]
    output: Option<PathBuf>,

    #[arg(long)]
    ibcmd: Option<PathBuf>,

    #[arg(long)]
    platform_arch: Option<String>,

    #[arg(long)]
    distrobox: Option<String>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

impl BuildArgs {
    /// Discover the project and exact ibcmd version, then publish one native artifact.
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover(project_dir) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("{}", diagnostics::present_project_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let plan = match BuildPlan::new(&project, self.output.as_deref()) {
            Ok(plan) => plan,
            Err(error) => {
                eprintln!("{}", present_plan_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let options = ToolOptions::new(
            self.ibcmd.clone(),
            self.platform_arch.clone(),
            self.distrobox.clone(),
        );
        let ibcmd = match Ibcmd::discover(plan.platform_version(), &options) {
            Ok(ibcmd) => ibcmd,
            Err(error) => {
                eprintln!("{}", present_tool_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let result = match build::execute(&plan, &ibcmd) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("{}", present_build_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        if let Err(error) = write_tool_output(result.tool_output()) {
            eprintln!(
                "{}",
                localizer.format(
                    "build-output-write-error",
                    &[("reason", LocalizationValue::Text(&error.to_string()))],
                )
            );
            return ExitCode::FAILURE;
        }

        match self.format {
            OutputFormat::Human => println!(
                "{}",
                localizer.format(
                    "build-completed",
                    &[
                        (
                            "artifact",
                            LocalizationValue::Text(&result.output().to_string_lossy()),
                        ),
                        ("version", LocalizationValue::Text(ibcmd.version().as_str()),),
                    ],
                )
            ),
            OutputFormat::Json => {
                let document = BuildDocument::new(&plan, &result);
                let Ok(json) = serde_json::to_string_pretty(&document) else {
                    eprintln!("{}", localizer.text("build-json-error"));
                    return ExitCode::FAILURE;
                };
                println!("{json}");
            }
        }
        ExitCode::SUCCESS
    }
}

/// Forward successful ibcmd diagnostics to stderr without contaminating JSON stdout.
fn write_tool_output(output: &[u8]) -> io::Result<()> {
    if output.is_empty() {
        return Ok(());
    }
    let mut stderr = io::stderr().lock();
    stderr.write_all(output)?;
    if !output.ends_with(b"\n") {
        stderr.write_all(b"\n")?;
    }
    stderr.flush()
}

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("build-about"))
        .override_usage(localizer.text("build-usage"))
        .mut_arg("output", |arg| {
            arg.help(localizer.text("build-output-help"))
                .value_name(localizer.text("build-output-value"))
        })
        .mut_arg("ibcmd", |arg| {
            arg.help(localizer.text("build-ibcmd-help"))
                .value_name(localizer.text("build-ibcmd-value"))
        })
        .mut_arg("platform_arch", |arg| {
            arg.help(localizer.text("build-arch-help"))
                .value_name(localizer.text("build-arch-value"))
        })
        .mut_arg("distrobox", |arg| {
            arg.help(localizer.text("build-distrobox-help"))
                .value_name(localizer.text("build-distrobox-value"))
        })
        .mut_arg("format", |arg| {
            arg.help(localizer.text("build-format-help"))
                .value_name(localizer.text("build-format-value"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}

/// Render output-plan errors without changing their stable domain representation.
fn present_plan_error(error: &PlanError, localizer: &Localizer) -> String {
    let (key, path) = match error {
        PlanError::ProjectNameMissing => return localizer.text("build-project-name-missing"),
        PlanError::InvalidOutput { path } => ("build-output-invalid", path),
        PlanError::UnexpectedExtension { path, expected } => {
            return localizer.format(
                "build-output-extension",
                &[
                    ("path", LocalizationValue::Text(&path.to_string_lossy())),
                    ("extension", LocalizationValue::Text(expected)),
                ],
            );
        }
    };
    localizer.format(
        key,
        &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
    )
}

/// Render discovery and exact-version failures with actionable machine-local settings.
fn present_tool_error(error: &ToolError, localizer: &Localizer) -> String {
    match error {
        ToolError::InvalidArchitecture(value) => localizer.format(
            "build-arch-invalid",
            &[("value", LocalizationValue::Text(value))],
        ),
        ToolError::InvalidContainer(value) => localizer.format(
            "build-distrobox-invalid",
            &[("value", LocalizationValue::Text(value))],
        ),
        ToolError::InvalidExecutable(path) => localizer.format(
            "build-ibcmd-invalid",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        ToolError::NotFound { expected, standard } => localizer.format(
            "build-ibcmd-missing",
            &[
                ("version", LocalizationValue::Text(expected.as_str())),
                ("path", LocalizationValue::Text(&standard.to_string_lossy())),
            ],
        ),
        ToolError::Run(error) => localizer.format(
            "build-ibcmd-run-error",
            &[("reason", LocalizationValue::Text(&error.to_string()))],
        ),
        ToolError::VersionCommandFailed { source, stderr } => localizer.format(
            "build-version-command-error",
            &[
                ("source", LocalizationValue::Text(&tool_source(source))),
                ("reason", LocalizationValue::Text(stderr)),
            ],
        ),
        ToolError::VersionUnreadable(source) => localizer.format(
            "build-version-unreadable",
            &[("source", LocalizationValue::Text(&tool_source(source)))],
        ),
        ToolError::VersionMismatch {
            expected,
            actual,
            source,
        } => localizer.format(
            "build-version-mismatch",
            &[
                ("expected", LocalizationValue::Text(expected.as_str())),
                ("actual", LocalizationValue::Text(actual.as_str())),
                ("source", LocalizationValue::Text(&tool_source(source))),
            ],
        ),
    }
}

/// Render build execution failures and identify the failing stage.
fn present_build_error(error: &BuildError, localizer: &Localizer) -> String {
    match error {
        BuildError::OutputParentMissing(path)
        | BuildError::InvalidExistingOutput(path)
        | BuildError::ArtifactMissing(path)
        | BuildError::ArtifactEmpty(path) => localizer.format(
            match error {
                BuildError::InvalidExistingOutput(_) => "build-output-existing-invalid",
                BuildError::ArtifactMissing(_) => "build-artifact-missing",
                BuildError::ArtifactEmpty(_) => "build-artifact-empty",
                _ => "build-output-invalid",
            },
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::ConfiguredOutputOutsideProject { root, output } => localizer.format(
            "build-output-outside-project",
            &[
                ("root", LocalizationValue::Text(&root.to_string_lossy())),
                ("path", LocalizationValue::Text(&output.to_string_lossy())),
            ],
        ),
        BuildError::CreateDirectory { path, source }
        | BuildError::CreateWorkspace { path, source }
        | BuildError::Publish { path, source }
        | BuildError::Restore { path, source }
        | BuildError::DescriptorDirectory { path, source }
        | BuildError::DescriptorRead { path, source } => localizer.format(
            "build-filesystem-error",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        BuildError::Run { stage, source } => localizer.format(
            match source {
                RunError::Interrupted => "build-interrupted",
                _ => "build-process-error",
            },
            &[
                (
                    "stage",
                    LocalizationValue::Text(&stage_name(*stage, localizer)),
                ),
                ("reason", LocalizationValue::Text(&format!("{source:?}"))),
            ],
        ),
        BuildError::CommandFailed { stage, stderr } => localizer.format(
            "build-command-failed",
            &[
                (
                    "stage",
                    LocalizationValue::Text(&stage_name(*stage, localizer)),
                ),
                ("reason", LocalizationValue::Text(stderr)),
            ],
        ),
        BuildError::DescriptorInvalid { path } => localizer.format(
            "build-descriptor-invalid",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::DescriptorMissing(path) => localizer.format(
            "build-descriptor-missing",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::DescriptorsMultiple(path) => localizer.format(
            "build-descriptors-multiple",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
    }
}

/// Return the selected executable location for diagnostics only.
fn tool_source(source: &ToolSource) -> String {
    match source {
        ToolSource::Explicit(path) | ToolSource::Path(path) | ToolSource::Standard(path) => {
            path.to_string_lossy().into_owned()
        }
        ToolSource::Distrobox { container, path } => {
            format!("{container}:{}", path.to_string_lossy())
        }
    }
}

/// Localize the fixed pipeline stage while retaining enum-based control flow.
fn stage_name(stage: BuildStage, localizer: &Localizer) -> String {
    localizer.text(match stage {
        BuildStage::CreateInfobase => "build-stage-create-infobase",
        BuildStage::ImportSources => "build-stage-import-sources",
    })
}

#[derive(Serialize)]
struct BuildDocument {
    schema_version: u8,
    artifact: ArtifactDocument,
    platform: PlatformDocument,
    duration_ms: u128,
}

#[derive(Serialize)]
struct ArtifactDocument {
    r#type: &'static str,
    path: String,
    path_encoding: &'static str,
}

#[derive(Serialize)]
struct PlatformDocument {
    version: String,
}

impl BuildDocument {
    /// Build schema version 1 without locale-dependent values.
    fn new(plan: &BuildPlan, result: &build::BuildResult) -> Self {
        let (path, path_encoding) = json_path(result.output().as_os_str());
        Self {
            schema_version: 1,
            artifact: ArtifactDocument {
                r#type: plan.artifact_type().as_str(),
                path,
                path_encoding,
            },
            platform: PlatformDocument {
                version: plan.platform_version().as_str().to_owned(),
            },
            duration_ms: result.duration().as_millis(),
        }
    }
}

/// Preserve a UTF-8 path directly and use a reversible platform encoding otherwise.
fn json_path(path: &OsStr) -> (String, &'static str) {
    path.to_str()
        .map_or_else(|| encoded_path(path), |value| (value.to_owned(), "utf-8"))
}

#[cfg(unix)]
/// Percent-encode every raw Unix path byte when it is not valid UTF-8.
fn encoded_path(path: &OsStr) -> (String, &'static str) {
    use std::os::unix::ffi::OsStrExt;
    let mut encoded = String::with_capacity(path.as_bytes().len() * 3);
    for byte in path.as_bytes() {
        write!(encoded, "%{byte:02X}").expect("writing to String cannot fail");
    }
    (encoded, "percent")
}

#[cfg(windows)]
/// Percent-encode every UTF-16 code unit when a Windows path is not Unicode scalar text.
fn encoded_path(path: &OsStr) -> (String, &'static str) {
    use std::os::windows::ffi::OsStrExt;
    let mut encoded = String::new();
    for unit in path.encode_wide() {
        write!(encoded, "%{unit:04X}").expect("writing to String cannot fail");
    }
    (encoded, "utf-16-percent")
}
