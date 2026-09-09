//! Localized build command with a locale-independent JSON result contract.

use std::{
    ffi::OsStr,
    fmt::Write as _,
    io::{self, IsTerminal, Write as _},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Sender},
    },
    thread,
    time::Duration,
};

use clap::{Args, ValueEnum};
use crossterm::{
    cursor::MoveToColumn,
    queue,
    terminal::{Clear, ClearType},
};
use gix::bstr::ByteSlice;
use serde::Serialize;

use crate::{
    cli::{
        changes, diagnostics,
        interactive::{PromptError, Selector},
        localization::{LocalizationValue, Localizer},
        platform,
    },
    project::{
        Project, ProjectName,
        build::{
            self, BuildError, BuildPlan, BuildSettingsError, BuildStage, Ibcmd, ManifestError,
            PlanError, PlatformVersion, RunError, ToolError, ToolOptions, ToolSource,
        },
        discovery::{self, DiscoveryContext},
        metadata,
        selection::{SelectedProject, SelectionError, SelectionIntent, select_projects},
    },
};

#[derive(Debug, Args)]
pub(in crate::cli) struct BuildArgs {
    #[arg(long)]
    output: Option<PathBuf>,

    #[arg(long)]
    base_configuration: Option<PathBuf>,

    #[arg(long)]
    ibcmd: Option<PathBuf>,

    #[arg(long)]
    platform_arch: Option<String>,

    #[arg(long)]
    distrobox: Option<String>,

    #[arg(long, conflicts_with = "select_platform")]
    platform_version: Option<String>,

    #[arg(long, conflicts_with = "platform_version")]
    select_platform: bool,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[arg(short = 'p', long)]
    project: Vec<String>,

    #[arg(long)]
    workspace: bool,

    #[command(flatten)]
    mode: BuildModeArgs,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Args)]
struct BuildModeArgs {
    #[arg(long)]
    dry_run: bool,

    #[arg(long, conflicts_with = "dry_run")]
    manifest: bool,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

struct PreparedBuild<'a> {
    name: Option<&'a ProjectName>,
    project: &'a Project,
    plan: BuildPlan,
}

enum BuildExecutionError {
    Build(BuildError),
    Output(io::Error),
}

impl BuildArgs {
    /// Discover, preflight and sequentially build the selected projects.
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let context = match discovery::discover_context(project_dir) {
            Ok(context) => context,
            Err(error) => {
                let detail = diagnostics::present_context_error(&error, localizer);
                return self.fail(
                    "project-discovery",
                    Some("discovery"),
                    None,
                    &detail,
                    localizer,
                    ExitCode::FAILURE,
                );
            }
        };
        let selection = match select_projects(
            &context,
            &self.project,
            self.workspace,
            SelectionIntent::MultipleMutation,
        ) {
            Ok(selection) => selection,
            Err(error) => return self.fail_selection(&error, localizer),
        };
        if self.output.is_some() && selection.projects().len() != 1 {
            return self.fail(
                "output-requires-single-project",
                Some("selection"),
                None,
                &localizer.text("build-output-single-project"),
                localizer,
                ExitCode::FAILURE,
            );
        }
        let options = match self.tool_options(localizer) {
            Ok(options) => options,
            Err(code) => return code,
        };
        let platform_override = match self.platform_override(&options, localizer) {
            Ok(platform_override) => platform_override,
            Err(code) => return code,
        };
        let workspace_root = workspace_root(&context);
        let base_configuration = self.resolved_base_configuration(&context);
        let mut prepared = Vec::with_capacity(selection.projects().len());
        for selected in selection.projects() {
            let plan = match build_plan(
                *selected,
                workspace_root,
                self.output.as_deref(),
                platform_override.clone(),
                base_configuration.clone(),
            ) {
                Ok(plan) => plan,
                Err(error) => {
                    return self.fail_project(
                        plan_error_code(&error),
                        Some("plan"),
                        selected.name(),
                        &present_plan_error(&error, localizer),
                        localizer,
                    );
                }
            };
            prepared.push(PreparedBuild {
                name: selected.name(),
                project: selected.project(),
                plan,
            });
        }
        if let Err(code) = preflight_group(self.format, &prepared, self.mode.manifest, localizer) {
            return code;
        }
        let tools = match discover_tools(self.format, &prepared, &options, localizer) {
            Ok(tools) => tools,
            Err(code) => return code,
        };
        if self.mode.dry_run {
            return write_build_preview(
                self.format,
                &prepared,
                &tools,
                selection.is_aggregate(),
                localizer,
            );
        }
        if selection.is_aggregate() {
            self.execute_aggregate(&prepared, &tools, localizer)
        } else if let (Some(prepared), Some(ibcmd)) = (prepared.first(), tools.first()) {
            self.execute_single(prepared, ibcmd, localizer)
        } else {
            self.fail(
                "project-selection-invalid",
                Some("selection"),
                None,
                &localizer.text("build-selection-invalid"),
                localizer,
                ExitCode::FAILURE,
            )
        }
    }

    /// Resolve the optional base CF once so every selected plan uses the same host path.
    fn resolved_base_configuration(&self, context: &DiscoveryContext) -> Option<PathBuf> {
        self.base_configuration
            .as_deref()
            .map(|path| resolve_base_configuration(path, context))
    }

    /// Resolve machine-local tool settings through the selected output contract.
    fn tool_options(&self, localizer: &Localizer) -> Result<ToolOptions, ExitCode> {
        platform::tool_options(
            self.ibcmd.clone(),
            self.platform_arch.clone(),
            self.distrobox.clone(),
        )
        .map_err(|error| {
            let detail = diagnostics::present_global_config_error(&error, localizer);
            self.fail(
                "machine-config",
                Some("configuration"),
                None,
                &detail,
                localizer,
                ExitCode::FAILURE,
            )
        })
    }

    /// Execute and present one project with the original single-project JSON contract.
    fn execute_single(
        &self,
        prepared: &PreparedBuild<'_>,
        ibcmd: &Ibcmd,
        localizer: &Localizer,
    ) -> ExitCode {
        match self.execute_plan(prepared, ibcmd, localizer) {
            Ok(result) => write_build_result(self.format, &prepared.plan, &result, localizer),
            Err(BuildExecutionError::Build(error)) => {
                let machine_error = BuildExecutionError::Build(error);
                self.fail_project(
                    execution_error_code(&machine_error),
                    execution_error_stage(&machine_error),
                    prepared.name,
                    &execution_error_message(&machine_error, localizer),
                    localizer,
                )
            }
            Err(BuildExecutionError::Output(error)) => {
                let detail = localizer.format(
                    "build-output-write-error",
                    &[("reason", LocalizationValue::Text(&error.to_string()))],
                );
                self.fail_project("output-write", None, prepared.name, &detail, localizer)
            }
        }
    }

    /// Execute all preflighted members in manifest/selector order and retain every outcome.
    fn execute_aggregate(
        &self,
        prepared: &[PreparedBuild<'_>],
        tools: &[Ibcmd],
        localizer: &Localizer,
    ) -> ExitCode {
        let mut entries = Vec::with_capacity(prepared.len());
        let mut failed = false;
        for (index, (item, ibcmd)) in prepared.iter().zip(tools).enumerate() {
            let Some(name) = item.name else {
                return self.fail(
                    "project-selection-invalid",
                    Some("selection"),
                    None,
                    &localizer.text("build-selection-invalid"),
                    localizer,
                    ExitCode::FAILURE,
                );
            };
            if matches!(self.format, OutputFormat::Human) {
                eprintln!(
                    "{}",
                    localizer.format(
                        "build-member-started",
                        &[("name", LocalizationValue::Text(name.as_str()))],
                    )
                );
            }
            match self.execute_plan(item, ibcmd, localizer) {
                Ok(result) => {
                    if matches!(self.format, OutputFormat::Human) {
                        let _ =
                            write_build_result(OutputFormat::Human, &item.plan, &result, localizer);
                    }
                    entries.push(WorkspaceBuildEntry::success(name, &item.plan, &result));
                }
                Err(error) => {
                    failed = true;
                    let interrupted = execution_was_interrupted(&error);
                    if matches!(self.format, OutputFormat::Human) {
                        let detail = execution_error_message(&error, localizer);
                        let _ = self.fail_project(
                            execution_error_code(&error),
                            execution_error_stage(&error),
                            Some(name),
                            &detail,
                            localizer,
                        );
                    }
                    entries.push(WorkspaceBuildEntry::failure(name, &item.plan, &error));
                    if interrupted {
                        entries.extend(prepared[index + 1..].iter().filter_map(|remaining| {
                            remaining
                                .name
                                .map(|name| WorkspaceBuildEntry::skipped(name, &remaining.plan))
                        }));
                        break;
                    }
                }
            }
        }
        if matches!(self.format, OutputFormat::Json) && !write_workspace_json(entries, localizer) {
            return ExitCode::FAILURE;
        }
        if failed {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        }
    }

    /// Run one already preflighted plan while streaming its diagnostics.
    fn execute_plan(
        &self,
        prepared: &PreparedBuild<'_>,
        ibcmd: &Ibcmd,
        localizer: &Localizer,
    ) -> Result<build::BuildResult, BuildExecutionError> {
        let project = prepared.project;
        let plan = &prepared.plan;
        let human = matches!(self.format, OutputFormat::Human);
        let interactive = human && io::stderr().is_terminal();
        let styled = diagnostic_styling_enabled();
        let configured_version = project.configuration().build_settings().platform_version();
        if human && configured_version != Some(plan.platform_version()) {
            let key = if configured_version.is_some() {
                "build-platform-override"
            } else {
                "build-platform-override-unconfigured"
            };
            let configured = configured_version.map_or("", PlatformVersion::as_str);
            eprintln!(
                "{}",
                localizer.format(
                    key,
                    &[
                        (
                            "version",
                            LocalizationValue::Text(plan.platform_version().as_str())
                        ),
                        ("configured", LocalizationValue::Text(configured)),
                    ],
                )
            );
        }
        if human {
            write_build_started(ibcmd.version().as_str(), localizer, styled)
                .map_err(BuildExecutionError::Output)?;
        }
        let mut progress =
            interactive.then(|| ProgressLine::start(localizer.text("build-progress"), styled));
        let mut output_error = None;
        let mut on_output = |_: BuildStage, _: build::ProcessStream, line: &[u8]| {
            if output_error.is_none()
                && let Err(error) =
                    write_diagnostic(line, project, localizer, styled, progress.as_ref())
            {
                output_error = Some(error);
            }
        };
        let result = if self.mode.manifest {
            let project_name = prepared
                .name
                .map(ProjectName::as_str)
                .or_else(|| project.root().file_name().and_then(std::ffi::OsStr::to_str));
            build::execute_streaming_with_manifest(
                plan,
                project,
                project_name,
                ibcmd,
                &mut on_output,
            )
        } else {
            build::execute_streaming(plan, ibcmd, &mut on_output)
        };
        if let Some(error) = progress
            .as_mut()
            .and_then(|progress| progress.finish().err())
            && output_error.is_none()
        {
            output_error = Some(error);
        }
        let result = result.map_err(BuildExecutionError::Build)?;
        if let Some(error) = output_error {
            return Err(BuildExecutionError::Output(error));
        }
        Ok(result)
    }

    /// Present one selection failure through the human and machine contracts.
    fn fail_selection(&self, error: &SelectionError, localizer: &Localizer) -> ExitCode {
        let message = match error {
            SelectionError::InvalidName(error) => localizer.format(
                "build-project-name-invalid",
                &[("name", LocalizationValue::Text(error.value()))],
            ),
            SelectionError::DuplicateSelector { name } => localizer.format(
                "build-project-duplicate",
                &[("name", LocalizationValue::Text(name.as_str()))],
            ),
            SelectionError::UnknownProject { name } => localizer.format(
                "build-project-unknown",
                &[("name", LocalizationValue::Text(name.as_str()))],
            ),
            SelectionError::WorkspaceSelectorForStandalone => {
                localizer.text("build-selector-standalone")
            }
            SelectionError::ConflictingSelectors => localizer.text("build-selector-conflict"),
            SelectionError::ExplicitProjectRequired | SelectionError::SingleProjectRequired => {
                localizer.text("build-selection-invalid")
            }
        };
        self.fail(
            selection_error_code(error),
            Some("selection"),
            None,
            &message,
            localizer,
            ExitCode::FAILURE,
        )
    }

    /// Add optional project context before presenting a failed project stage.
    fn fail_project(
        &self,
        code: &'static str,
        stage: Option<&'static str>,
        name: Option<&ProjectName>,
        detail: &str,
        localizer: &Localizer,
    ) -> ExitCode {
        let message = name.map_or_else(
            || detail.to_owned(),
            |name| {
                localizer.format(
                    "build-member-error",
                    &[
                        ("name", LocalizationValue::Text(name.as_str())),
                        ("reason", LocalizationValue::Text(detail)),
                    ],
                )
            },
        );
        self.fail(
            code,
            stage,
            name.map(ProjectName::as_str),
            &message,
            localizer,
            ExitCode::FAILURE,
        )
    }

    /// Emit stable JSON on stdout and localized detail on stderr after argument parsing.
    fn fail(
        &self,
        code: &'static str,
        stage: Option<&'static str>,
        project: Option<&str>,
        detail: &str,
        localizer: &Localizer,
        exit_code: ExitCode,
    ) -> ExitCode {
        if matches!(self.format, OutputFormat::Json) {
            write_build_error(code, stage, project, localizer);
        }
        eprintln!("{detail}");
        exit_code
    }

    /// Resolve a one-run explicit or interactive platform override.
    fn platform_override(
        &self,
        options: &ToolOptions,
        localizer: &Localizer,
    ) -> Result<Option<PlatformVersion>, ExitCode> {
        if let Some(value) = &self.platform_version {
            return PlatformVersion::parse(value).map(Some).map_err(|error| {
                self.fail(
                    "platform-version-invalid",
                    Some("platform-selection"),
                    None,
                    &present_platform_version_error(&error, localizer),
                    localizer,
                    ExitCode::from(2),
                )
            });
        }
        if !self.select_platform {
            return Ok(None);
        }
        if matches!(self.format, OutputFormat::Json)
            || !io::stdin().is_terminal()
            || !io::stderr().is_terminal()
        {
            return Err(self.fail(
                "platform-selection-requires-terminal",
                Some("platform-selection"),
                None,
                &localizer.text("build-platform-select-terminal"),
                localizer,
                ExitCode::from(2),
            ));
        }
        let installed = Ibcmd::installed(options).map_err(|error| {
            self.fail(
                tool_error_code(&error),
                Some("tool-discovery"),
                None,
                &diagnostics::present_tool_error(&error, localizer),
                localizer,
                ExitCode::FAILURE,
            )
        })?;
        if installed.is_empty() {
            return Err(self.fail(
                "platform-not-found",
                Some("tool-discovery"),
                None,
                &localizer.text("platform-none"),
                localizer,
                ExitCode::FAILURE,
            ));
        }
        let choices: Vec<_> = installed
            .iter()
            .map(|platform| {
                let version = platform.version().as_str().to_owned();
                (version.clone(), version)
            })
            .collect();
        let selection = (|| {
            let mut selector = Selector::start("build-platform-tui-title")?;
            let value = selector.choose_values(localizer, "build-platform-menu", &choices)?;
            selector.finish().map_err(|_| PromptError::Io)?;
            Ok::<_, PromptError>(value)
        })()
        .map_err(|error| {
            eprintln!(
                "{}",
                localizer.text(match error {
                    PromptError::Cancelled => "build-platform-select-cancelled",
                    PromptError::Io => "build-platform-select-error",
                })
            );
            ExitCode::FAILURE
        })?;
        PlatformVersion::parse(&selection)
            .map(Some)
            .map_err(|error| {
                self.fail(
                    "platform-version-invalid",
                    Some("platform-selection"),
                    None,
                    &present_platform_version_error(&error, localizer),
                    localizer,
                    ExitCode::FAILURE,
                )
            })
    }
}

/// Return the shared output scope only when discovery selected a workspace.
fn workspace_root(context: &DiscoveryContext) -> Option<&Path> {
    match context {
        DiscoveryContext::Standalone(_) => None,
        DiscoveryContext::Workspace { workspace, .. } => Some(workspace.root()),
    }
}

/// Resolve a CLI input from the stable project or workspace root before crossing runners.
fn resolve_base_configuration(path: &Path, context: &DiscoveryContext) -> PathBuf {
    if path.is_absolute() {
        return path.to_owned();
    }
    let root = match context {
        DiscoveryContext::Standalone(project) => project.root(),
        DiscoveryContext::Workspace { workspace, .. } => workspace.root(),
    };
    root.join(path)
}

/// Resolve a standalone or workspace-scoped plan without touching the filesystem.
fn build_plan(
    selected: SelectedProject<'_>,
    workspace_root: Option<&Path>,
    output: Option<&Path>,
    platform_version: Option<PlatformVersion>,
    base_configuration: Option<PathBuf>,
) -> Result<BuildPlan, PlanError> {
    let plan = match (selected.name(), workspace_root) {
        (Some(name), Some(root)) => BuildPlan::for_workspace_member(
            selected.project(),
            root,
            name.as_str(),
            output,
            platform_version,
        ),
        _ => BuildPlan::with_platform_version(selected.project(), output, platform_version),
    }?;
    plan.with_base_configuration(base_configuration)
}

fn preflight_group(
    format: OutputFormat,
    prepared: &[PreparedBuild<'_>],
    manifest: bool,
    localizer: &Localizer,
) -> Result<(), ExitCode> {
    let plans: Vec<_> = prepared.iter().map(|item| &item.plan).collect();
    if let Err(error) = build::validate_unique_outputs(&plans) {
        write_failure(
            format,
            plan_error_code(&error),
            Some("preflight"),
            None,
            &present_plan_error(&error, localizer),
            localizer,
        );
        return Err(ExitCode::FAILURE);
    }
    for item in prepared {
        let result = if manifest {
            build::preflight_manifest(&item.plan)
        } else {
            build::preflight(&item.plan)
        };
        if let Err(error) = result {
            let machine_error = BuildExecutionError::Build(error);
            return Err(write_project_failure(
                format,
                execution_error_code(&machine_error),
                Some("preflight"),
                item.name,
                &execution_error_message(&machine_error, localizer),
                localizer,
            ));
        }
    }
    Ok(())
}

fn discover_tools(
    format: OutputFormat,
    prepared: &[PreparedBuild<'_>],
    options: &ToolOptions,
    localizer: &Localizer,
) -> Result<Vec<Ibcmd>, ExitCode> {
    let mut tools = Vec::with_capacity(prepared.len());
    for item in prepared {
        match Ibcmd::discover(item.plan.platform_version(), options) {
            Ok(ibcmd) => tools.push(ibcmd),
            Err(error) => {
                return Err(write_project_failure(
                    format,
                    tool_error_code(&error),
                    Some("tool-discovery"),
                    item.name,
                    &diagnostics::present_tool_error(&error, localizer),
                    localizer,
                ));
            }
        }
    }
    Ok(tools)
}

/// Present a fully preflighted plan without starting any build stage.
fn write_build_preview(
    format: OutputFormat,
    prepared: &[PreparedBuild<'_>],
    tools: &[Ibcmd],
    aggregate: bool,
    localizer: &Localizer,
) -> ExitCode {
    match format {
        OutputFormat::Human => write_human_build_preview(prepared, tools, aggregate, localizer),
        OutputFormat::Json => {
            let document = BuildPlanDocument::new(prepared, tools, aggregate);
            let Ok(json) = serde_json::to_string_pretty(&document) else {
                eprintln!("{}", localizer.text("build-json-error"));
                return ExitCode::FAILURE;
            };
            println!("{json}");
        }
    }
    ExitCode::SUCCESS
}

fn write_human_build_preview(
    prepared: &[PreparedBuild<'_>],
    tools: &[Ibcmd],
    aggregate: bool,
    localizer: &Localizer,
) {
    println!("{}", localizer.text("build-preview-title"));
    println!(
        "{}",
        localizer.text(if aggregate {
            "build-preview-scope-workspace"
        } else {
            "build-preview-scope-project"
        })
    );
    println!(
        "{}",
        localizer.format(
            "build-preview-projects",
            &[(
                "projects",
                LocalizationValue::Number(i64::try_from(prepared.len()).unwrap_or(i64::MAX)),
            )],
        )
    );
    for (index, (item, tool)) in prepared.iter().zip(tools).enumerate() {
        let position = i64::try_from(index + 1).unwrap_or(i64::MAX);
        let name = item.name.map_or_else(
            || project_display_name(item.project),
            |name| name.as_str().to_owned(),
        );
        println!(
            "{}",
            localizer.format(
                "build-preview-project",
                &[
                    ("position", LocalizationValue::Number(position)),
                    ("name", LocalizationValue::Text(&name)),
                ],
            )
        );
        write_build_preview_field(
            "build-preview-root",
            &display_path(item.plan.project_root()),
            localizer,
        );
        write_build_preview_field(
            "build-preview-source",
            &display_path(item.plan.source()),
            localizer,
        );
        if let Some(path) = item.plan.base_configuration() {
            write_build_preview_field(
                "build-preview-base-configuration",
                &display_path(path),
                localizer,
            );
        }
        write_build_preview_field(
            "build-preview-artifact-type",
            &localizer.text(artifact_type_key(item.plan.artifact_type())),
            localizer,
        );
        write_build_preview_field(
            "build-preview-artifact-path",
            &display_path(item.plan.output()),
            localizer,
        );
        write_build_preview_field(
            "build-preview-replaces-existing",
            &localizer.text(if item.plan.output().is_file() {
                "build-preview-yes"
            } else {
                "build-preview-no"
            }),
            localizer,
        );
        write_build_preview_field(
            "build-preview-required-platform",
            item.plan.platform_version().as_str(),
            localizer,
        );
        write_build_preview_field(
            "build-preview-found-platform",
            tool.version().as_str(),
            localizer,
        );
        write_build_preview_field(
            "build-preview-runner",
            &localizer.text(runner_key(tool.source())),
            localizer,
        );
    }
}

fn write_build_preview_field(key: &str, value: &str, localizer: &Localizer) {
    println!(
        "{}",
        localizer.format(key, &[("value", LocalizationValue::Text(value))])
    );
}

fn project_display_name(project: &Project) -> String {
    project
        .root()
        .file_name()
        .unwrap_or_else(|| project.root().as_os_str())
        .to_string_lossy()
        .into_owned()
}

const fn artifact_type_key(artifact_type: build::ArtifactType) -> &'static str {
    match artifact_type {
        build::ArtifactType::Configuration => "build-preview-type-configuration",
        build::ArtifactType::Extension => "build-preview-type-extension",
        build::ArtifactType::Processing => "build-preview-type-processing",
        build::ArtifactType::Report => "build-preview-type-report",
    }
}

const fn runner_key(source: &ToolSource) -> &'static str {
    match source {
        ToolSource::Explicit(_) | ToolSource::Path(_) | ToolSource::Standard(_) => {
            "build-preview-runner-host"
        }
        ToolSource::Distrobox { .. } => "build-preview-runner-distrobox",
    }
}

/// Present a project-scoped failure outside the command implementation.
fn write_project_failure(
    format: OutputFormat,
    code: &'static str,
    stage: Option<&'static str>,
    name: Option<&ProjectName>,
    detail: &str,
    localizer: &Localizer,
) -> ExitCode {
    let message = name.map_or_else(
        || detail.to_owned(),
        |name| {
            localizer.format(
                "build-member-error",
                &[
                    ("name", LocalizationValue::Text(name.as_str())),
                    ("reason", LocalizationValue::Text(detail)),
                ],
            )
        },
    );
    write_failure(
        format,
        code,
        stage,
        name.map(ProjectName::as_str),
        &message,
        localizer,
    );
    ExitCode::FAILURE
}

/// Emit a machine error when requested and always retain the localized stderr diagnostic.
fn write_failure(
    format: OutputFormat,
    code: &'static str,
    stage: Option<&'static str>,
    project: Option<&str>,
    detail: &str,
    localizer: &Localizer,
) {
    if matches!(format, OutputFormat::Json) {
        write_build_error(code, stage, project, localizer);
    }
    eprintln!("{detail}");
}

fn execution_error_message(error: &BuildExecutionError, localizer: &Localizer) -> String {
    match error {
        BuildExecutionError::Build(error) => present_streamed_build_error(error, localizer),
        BuildExecutionError::Output(error) => localizer.format(
            "build-output-write-error",
            &[("reason", LocalizationValue::Text(&error.to_string()))],
        ),
    }
}

/// Render and flush one ibcmd line as soon as it reaches the CLI layer.
fn write_diagnostic(
    line: &[u8],
    project: &Project,
    localizer: &Localizer,
    styled: bool,
    progress: Option<&ProgressLine>,
) -> io::Result<()> {
    let Ok(line) = std::str::from_utf8(line) else {
        return write_diagnostic_bytes(line, progress);
    };
    let line = humanize_source_paths(line, project, localizer);
    let line = style_diagnostic_level(&line, styled);
    write_diagnostic_bytes(line.as_bytes(), progress)
}

/// Write one diagnostic while keeping an interactive progress line at the bottom.
fn write_diagnostic_bytes(line: &[u8], progress: Option<&ProgressLine>) -> io::Result<()> {
    if let Some(progress) = progress {
        return progress.write_diagnostic(line);
    }
    let mut stderr = io::stderr().lock();
    stderr.write_all(line)?;
    if !line.ends_with(b"\n") {
        stderr.write_all(b"\n")?;
    }
    stderr.flush()
}

/// Replace absolute source files with their existing Configurator-style ownership.
fn humanize_source_paths(message: &str, project: &Project, localizer: &Localizer) -> String {
    let source = project.source();
    let source_text = source.to_string_lossy();
    let mut rendered = String::with_capacity(message.len());
    let mut remaining = message;
    while let Some(position) = remaining.find(source_text.as_ref()) {
        rendered.push_str(&remaining[..position]);
        let after_source = &remaining[position + source_text.len()..];
        let Some((relative, consumed)) = existing_source_path(after_source, source) else {
            rendered.push_str(&source_text);
            remaining = after_source;
            continue;
        };
        let normalized = relative.replace('\\', "/");
        let display = metadata::from_path(
            project.configuration().project_type(),
            normalized.as_bytes().as_bstr(),
        )
        .map_or_else(
            || normalized.clone(),
            |path| {
                let owner = changes::render_metadata_path(&path, localizer);
                diagnostic_path_tail(&normalized)
                    .map_or_else(|| owner.clone(), |tail| format!("{owner} · {tail}"))
            },
        );
        rendered.push_str(&display);
        remaining = &after_source[consumed..];
    }
    rendered.push_str(remaining);
    rendered
}

/// Retain the concrete help payload below its concise logical metadata owner.
fn diagnostic_path_tail(relative: &str) -> Option<&str> {
    relative
        .find("/Ext/Help/")
        .map(|position| &relative[position + 1..])
}

/// Find the longest existing path immediately below the configured source directory.
fn existing_source_path<'a>(suffix: &'a str, source: &Path) -> Option<(&'a str, usize)> {
    let separator_length = if suffix.starts_with('/') || suffix.starts_with('\\') {
        1
    } else {
        return None;
    };
    let path = &suffix[separator_length..];
    path.char_indices()
        .map(|(index, _)| index)
        .chain([path.len()])
        .rev()
        .find_map(|end| {
            let relative = &path[..end];
            (!relative.is_empty() && source.join(relative).exists())
                .then_some((relative, separator_length + end))
        })
}

/// Highlight a recognized ibcmd severity prefix while keeping its message unchanged.
fn style_diagnostic_level(message: &str, styled: bool) -> String {
    if !styled {
        return message.to_owned();
    }
    for (level, color) in [
        ("[TRACE]", "90"),
        ("[DEBUG]", "34"),
        ("[INFO]", "36"),
        ("[WARN]", "33"),
        ("[ERROR]", "31"),
        ("[FATAL]", "35"),
    ] {
        if let Some(rest) = message.strip_prefix(level) {
            return format!("\x1b[1;{color}m{level}\x1b[0m{rest}");
        }
    }
    message.to_owned()
}

/// Enable diagnostic colors only for an interactive stderr that permits color.
fn diagnostic_styling_enabled() -> bool {
    io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Enable success styling only when its stdout destination is interactive.
fn result_styling_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Print the stable build heading before the first tool stage starts.
fn write_build_started(version: &str, localizer: &Localizer, styled: bool) -> io::Result<()> {
    let message = localizer.format(
        "build-started",
        &[("version", LocalizationValue::Text(version))],
    );
    let message = decorate_status("▶", &message, styled, "36");
    let mut stderr = io::stderr().lock();
    writeln!(stderr, "{message}")?;
    stderr.flush()
}

/// Add a stable marker and optionally color only that marker.
fn decorate_status(marker: &str, message: &str, styled: bool, color: &str) -> String {
    if styled {
        format!("\x1b[1;{color}m{marker}\x1b[0m {message}")
    } else {
        format!("{marker} {message}")
    }
}

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

struct ProgressState {
    output: Mutex<()>,
    frame: AtomicUsize,
    message: String,
    styled: bool,
}

struct ProgressLine {
    state: Arc<ProgressState>,
    stop: Sender<()>,
    worker: Option<thread::JoinHandle<io::Result<()>>>,
    active: bool,
}

impl ProgressLine {
    /// Start a terminal-owned spinner that remains below streamed diagnostics.
    fn start(message: String, styled: bool) -> Self {
        let state = Arc::new(ProgressState {
            output: Mutex::new(()),
            frame: AtomicUsize::new(0),
            message,
            styled,
        });
        let (stop, receiver) = mpsc::channel();
        let worker_state = Arc::clone(&state);
        let worker = thread::spawn(move || {
            loop {
                draw_progress(&worker_state)?;
                match receiver.recv_timeout(Duration::from_millis(80)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        worker_state.frame.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
        Self {
            state,
            stop,
            worker: Some(worker),
            active: true,
        }
    }

    /// Clear the spinner, write one complete diagnostic line, then restore it.
    fn write_diagnostic(&self, line: &[u8]) -> io::Result<()> {
        let _guard = lock_progress_output(&self.state)?;
        let mut stderr = io::stderr().lock();
        clear_progress(&mut stderr)?;
        stderr.write_all(line)?;
        if !line.ends_with(b"\n") {
            stderr.write_all(b"\n")?;
        }
        draw_progress_locked(&mut stderr, &self.state)?;
        stderr.flush()
    }

    /// Stop animation and remove its final line before result presentation.
    fn finish(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let _ = self.stop.send(());
        let worker_result = self
            .worker
            .take()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| io::Error::other("build progress thread panicked"))?
            })
            .transpose();
        let clear_result = {
            let _guard = lock_progress_output(&self.state)?;
            let mut stderr = io::stderr().lock();
            clear_progress(&mut stderr)?;
            stderr.flush()
        };
        worker_result.and(clear_result)
    }
}

impl Drop for ProgressLine {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

/// Lock the multi-write terminal sequence shared with the animation thread.
fn lock_progress_output(state: &ProgressState) -> io::Result<std::sync::MutexGuard<'_, ()>> {
    state
        .output
        .lock()
        .map_err(|_| io::Error::other("build progress output lock was poisoned"))
}

/// Draw the current animation frame while acquiring the shared terminal lock.
fn draw_progress(state: &ProgressState) -> io::Result<()> {
    let _guard = lock_progress_output(state)?;
    let mut stderr = io::stderr().lock();
    draw_progress_locked(&mut stderr, state)?;
    stderr.flush()
}

/// Draw the current animation frame as the terminal's last line.
fn draw_progress_locked(stderr: &mut impl io::Write, state: &ProgressState) -> io::Result<()> {
    let frame = state.frame.load(Ordering::Relaxed) % SPINNER_FRAMES.len();
    let message = decorate_status(SPINNER_FRAMES[frame], &state.message, state.styled, "36");
    queue!(
        stderr,
        MoveToColumn(0),
        Clear(ClearType::CurrentLine),
        crossterm::style::Print(message)
    )
}

/// Remove the transient progress line without affecting completed diagnostics.
fn clear_progress(stderr: &mut impl io::Write) -> io::Result<()> {
    queue!(stderr, MoveToColumn(0), Clear(ClearType::CurrentLine))
}

/// Present the successful build without changing the JSON result contract.
fn write_build_result(
    format: OutputFormat,
    plan: &BuildPlan,
    result: &build::BuildResult,
    localizer: &Localizer,
) -> ExitCode {
    match format {
        OutputFormat::Human => {
            let interactive = io::stdout().is_terminal();
            println!(
                "{}",
                render_build_success(
                    result.output(),
                    localizer,
                    result_styling_enabled(),
                    interactive,
                )
            );
            if let Some(manifest) = result.manifest() {
                println!(
                    "{} {}",
                    localizer.text("build-manifest-completed-label"),
                    render_artifact_link(manifest, interactive),
                );
            }
        }
        OutputFormat::Json => {
            let document = BuildDocument::new(plan, result);
            let Ok(json) = serde_json::to_string_pretty(&document) else {
                eprintln!("{}", localizer.text("build-json-error"));
                return ExitCode::FAILURE;
            };
            println!("{json}");
        }
    }
    ExitCode::SUCCESS
}

/// Render a prominent label and link the visible artifact path to its directory.
fn render_build_success(
    artifact: &Path,
    localizer: &Localizer,
    styled: bool,
    hyperlink: bool,
) -> String {
    let label = localizer.text("build-completed-label");
    let label = if styled {
        format!("\x1b[1m{label}\x1b[0m")
    } else {
        label
    };
    let artifact = render_artifact_link(artifact, hyperlink);
    decorate_status("✓", &format!("{label} {artifact}"), styled, "32")
}

/// Link the artifact label to its parent directory in an interactive terminal.
fn render_artifact_link(artifact: &Path, hyperlink: bool) -> String {
    let label = display_path(artifact);
    let Some(parent) = artifact.parent().filter(|_| hyperlink) else {
        return label;
    };
    let target = file_uri(parent);
    format!("\x1b]8;;{target}\x1b\\{label}\x1b]8;;\x1b\\")
}

/// Escape control characters without making a normal filesystem path less readable.
fn display_path(path: &Path) -> String {
    let mut display = String::new();
    for character in path.to_string_lossy().chars() {
        if character.is_control() {
            display.extend(character.escape_default());
        } else {
            display.push(character);
        }
    }
    display
}

/// Encode an absolute local directory as a safe file URI for an OSC 8 target.
fn file_uri(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let prefix = if normalized.starts_with("//") {
        "file:"
    } else if normalized.starts_with('/') {
        "file://"
    } else {
        "file:///"
    };
    format!("{prefix}{}", percent_encode_uri_path(normalized.as_bytes()))
}

/// Percent-encode bytes that are not safe inside a hierarchical file URI path.
fn percent_encode_uri_path(path: &[u8]) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            encoded.push(*byte as char);
        } else {
            write!(encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("build-about"))
        .override_usage(localizer.text("build-usage"))
        .mut_arg("output", |arg| {
            arg.help(localizer.text("build-output-help"))
                .value_name(localizer.text("build-output-value"))
        })
        .mut_arg("base_configuration", |arg| {
            arg.help(localizer.text("build-base-configuration-help"))
                .value_name(localizer.text("build-base-configuration-value"))
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
        .mut_arg("platform_version", |arg| {
            arg.help(localizer.text("build-platform-version-help"))
                .value_name(localizer.text("build-platform-version-value"))
        })
        .mut_arg("select_platform", |arg| {
            arg.help(localizer.text("build-select-platform-help"))
        })
        .mut_arg("format", |arg| {
            arg.help(localizer.text("build-format-help"))
                .value_name(localizer.text("build-format-value"))
        })
        .mut_arg("project", |arg| {
            arg.help(localizer.text("build-project-help"))
                .value_name(localizer.text("build-project-value"))
        })
        .mut_arg("workspace", |arg| {
            arg.help(localizer.text("build-workspace-help"))
        })
        .mut_arg("dry_run", |arg| {
            arg.help(localizer.text("build-dry-run-help"))
        })
        .mut_arg("manifest", |arg| {
            arg.help(localizer.text("build-manifest-help"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}

/// Render output-plan errors without changing their stable domain representation.
fn present_plan_error(error: &PlanError, localizer: &Localizer) -> String {
    let (key, path) = match error {
        PlanError::ProjectNameMissing => return localizer.text("build-project-name-missing"),
        PlanError::PlatformVersionMissing => {
            return localizer.text("build-platform-version-missing");
        }
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
        PlanError::OutputCollision { path } => ("build-output-collision", path),
        PlanError::BaseConfigurationUnsupported => {
            return localizer.text("build-base-configuration-unsupported");
        }
    };
    localizer.format(
        key,
        &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
    )
}

fn present_platform_version_error(error: &BuildSettingsError, localizer: &Localizer) -> String {
    match error {
        BuildSettingsError::InvalidPlatformVersion { value } => localizer.format(
            "build-platform-version-invalid",
            &[("value", LocalizationValue::Text(value))],
        ),
        BuildSettingsError::InvalidArtifactsDirectory { .. } => {
            localizer.text("build-platform-version-error")
        }
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
        BuildError::BaseConfigurationInvalid(path) => localizer.format(
            "build-base-configuration-invalid",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::BaseConfigurationRead { path, source } => localizer.format(
            "build-base-configuration-read",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        BuildError::Manifest(error) => present_manifest_error(error, localizer),
    }
}

/// Render source-snapshot and manifest failures without adding localized text to core.
fn present_manifest_error(error: &ManifestError, localizer: &Localizer) -> String {
    match error {
        ManifestError::SnapshotIo { path, source } => localizer.format(
            "build-manifest-filesystem-error",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        ManifestError::SnapshotEntryUnsupported(path) => localizer.format(
            "build-manifest-entry-unsupported",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        ManifestError::SnapshotProject(_) => localizer.text("build-manifest-snapshot-invalid"),
        ManifestError::ArtifactRead { path, source } | ManifestError::Write { path, source } => {
            localizer.format(
                "build-manifest-filesystem-error",
                &[
                    ("path", LocalizationValue::Text(&path.to_string_lossy())),
                    ("reason", LocalizationValue::Text(&source.to_string())),
                ],
            )
        }
        ManifestError::Serialize(_) => localizer.text("build-manifest-serialize-error"),
    }
}

/// Avoid repeating process output that the streaming renderer has already emitted.
fn present_streamed_build_error(error: &BuildError, localizer: &Localizer) -> String {
    if let BuildError::CommandFailed { stage, .. } = error {
        localizer.format(
            "build-command-failed-streamed",
            &[(
                "stage",
                LocalizationValue::Text(&stage_name(*stage, localizer)),
            )],
        )
    } else {
        present_build_error(error, localizer)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decorate_status, diagnostic_path_tail, render_build_success, style_diagnostic_level,
    };
    use crate::cli::localization::{Locale, Localizer};
    use std::path::Path;

    #[test]
    /// Color only the recognized diagnostic prefix and preserve the message bytes.
    fn styles_known_diagnostic_levels() {
        for (level, color) in [
            ("TRACE", "90"),
            ("DEBUG", "34"),
            ("INFO", "36"),
            ("WARN", "33"),
            ("ERROR", "31"),
            ("FATAL", "35"),
        ] {
            let message = format!("[{level}] diagnostic\n");
            assert_eq!(
                style_diagnostic_level(&message, true),
                format!("\x1b[1;{color}m[{level}]\x1b[0m diagnostic\n")
            );
            assert_eq!(style_diagnostic_level(&message, false), message);
        }
    }

    #[test]
    /// Leave unknown prefixes unchanged even when terminal styling is enabled.
    fn preserves_unknown_diagnostic_prefixes() {
        assert_eq!(
            style_diagnostic_level("plain diagnostic\n", true),
            "plain diagnostic\n"
        );
    }

    #[test]
    /// Preserve the concrete help file without restoring its absolute source path.
    fn keeps_help_payload_tail_after_logical_owner() {
        assert_eq!(
            diagnostic_path_tail("DataProcessors/Files/Forms/AttachedFile/Ext/Help/ru.html"),
            Some("Ext/Help/ru.html")
        );
        assert_eq!(
            diagnostic_path_tail("DataProcessors/Files/Ext/ObjectModule.bsl"),
            None
        );
    }

    #[test]
    /// Keep status markers in redirected output and color only the interactive marker.
    fn decorates_build_status_without_changing_its_message() {
        assert_eq!(
            decorate_status("✓", "Built artifact", false, "32"),
            "✓ Built artifact"
        );
        assert_eq!(
            decorate_status("✓", "Built artifact", true, "32"),
            "\x1b[1;32m✓\x1b[0m Built artifact"
        );
    }

    #[test]
    /// Link the visible artifact to its directory and bold only the result label.
    fn build_result_links_to_artifact_directory() {
        let localizer = Localizer::try_new(Locale::RuRu).expect("locale");
        let artifact = Path::new("/tmp/build dir/demo.cf");
        assert_eq!(
            render_build_success(artifact, &localizer, true, true),
            concat!(
                "\x1b[1;32m✓\x1b[0m \x1b[1mСобран\x1b[0m ",
                "\x1b]8;;file:///tmp/build%20dir\x1b\\",
                "/tmp/build dir/demo.cf",
                "\x1b]8;;\x1b\\"
            )
        );
        assert_eq!(
            render_build_success(artifact, &localizer, false, false),
            "✓ Собран /tmp/build dir/demo.cf"
        );
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
struct BuildPlanDocument {
    schema_version: u8,
    kind: &'static str,
    scope: &'static str,
    projects: Vec<BuildPlanEntryDocument>,
}

#[derive(Serialize)]
struct BuildPlanEntryDocument {
    name: Option<String>,
    root: BuildPlanPathDocument,
    source: BuildPlanPathDocument,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_configuration: Option<BuildPlanPathDocument>,
    artifact: BuildPlanArtifactDocument,
    platform: BuildPlanPlatformDocument,
}

#[derive(Serialize)]
struct BuildPlanPathDocument {
    path: String,
    path_encoding: &'static str,
}

#[derive(Serialize)]
struct BuildPlanArtifactDocument {
    r#type: &'static str,
    path: String,
    path_encoding: &'static str,
    replaces_existing: bool,
}

#[derive(Serialize)]
struct BuildPlanPlatformDocument {
    required_version: String,
    found_version: String,
    runner: &'static str,
}

#[derive(Serialize)]
struct WorkspaceBuildDocument {
    schema_version: u8,
    projects: Vec<WorkspaceBuildEntry>,
}

#[derive(Serialize)]
struct WorkspaceBuildEntry {
    name: String,
    status: &'static str,
    artifact: Option<ArtifactDocument>,
    platform: PlatformDocument,
    duration_ms: Option<u128>,
    error: Option<BuildFailureDocument>,
}

#[derive(Serialize)]
struct BuildFailureDocument {
    code: &'static str,
    stage: Option<&'static str>,
}

#[derive(Serialize)]
struct BuildErrorDetailDocument {
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
}

#[derive(Serialize)]
struct BuildErrorDocument {
    schema_version: u8,
    status: &'static str,
    error: BuildErrorDetailDocument,
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

impl BuildPlanDocument {
    /// Build a locale-independent dry-run document in execution order.
    fn new(prepared: &[PreparedBuild<'_>], tools: &[Ibcmd], aggregate: bool) -> Self {
        let projects = prepared
            .iter()
            .zip(tools)
            .map(|(item, tool)| BuildPlanEntryDocument::new(item, tool))
            .collect();
        Self {
            schema_version: 1,
            kind: "build-plan",
            scope: if aggregate { "workspace" } else { "project" },
            projects,
        }
    }
}

impl BuildPlanEntryDocument {
    /// Serialize one preflighted plan and discovered runner without localized values.
    fn new(prepared: &PreparedBuild<'_>, tool: &Ibcmd) -> Self {
        let (artifact_path, artifact_path_encoding) = json_path(prepared.plan.output().as_os_str());
        Self {
            name: prepared.name.map(|name| name.as_str().to_owned()),
            root: BuildPlanPathDocument::new(prepared.plan.project_root()),
            source: BuildPlanPathDocument::new(prepared.plan.source()),
            base_configuration: prepared
                .plan
                .base_configuration()
                .map(BuildPlanPathDocument::new),
            artifact: BuildPlanArtifactDocument {
                r#type: prepared.plan.artifact_type().as_str(),
                path: artifact_path,
                path_encoding: artifact_path_encoding,
                replaces_existing: prepared.plan.output().is_file(),
            },
            platform: BuildPlanPlatformDocument {
                required_version: prepared.plan.platform_version().as_str().to_owned(),
                found_version: tool.version().as_str().to_owned(),
                runner: tool.runner_kind(),
            },
        }
    }
}

impl BuildPlanPathDocument {
    /// Preserve one plan path with the existing reversible build encoding.
    fn new(path: &Path) -> Self {
        let (path, path_encoding) = json_path(path.as_os_str());
        Self {
            path,
            path_encoding,
        }
    }
}

impl WorkspaceBuildEntry {
    /// Serialize one successful member without locale-dependent values.
    fn success(name: &ProjectName, plan: &BuildPlan, result: &build::BuildResult) -> Self {
        let (path, path_encoding) = json_path(result.output().as_os_str());
        Self {
            name: name.as_str().to_owned(),
            status: "success",
            artifact: Some(ArtifactDocument {
                r#type: plan.artifact_type().as_str(),
                path,
                path_encoding,
            }),
            platform: PlatformDocument {
                version: plan.platform_version().as_str().to_owned(),
            },
            duration_ms: Some(result.duration().as_millis()),
            error: None,
        }
    }

    /// Serialize one failed member through stable non-localized error codes.
    fn failure(name: &ProjectName, plan: &BuildPlan, error: &BuildExecutionError) -> Self {
        Self {
            name: name.as_str().to_owned(),
            status: "failed",
            artifact: None,
            platform: PlatformDocument {
                version: plan.platform_version().as_str().to_owned(),
            },
            duration_ms: None,
            error: Some(BuildFailureDocument {
                code: execution_error_code(error),
                stage: execution_error_stage(error),
            }),
        }
    }

    /// Mark a member that was not started because the user interrupted the group.
    fn skipped(name: &ProjectName, plan: &BuildPlan) -> Self {
        Self {
            name: name.as_str().to_owned(),
            status: "skipped",
            artifact: None,
            platform: PlatformDocument {
                version: plan.platform_version().as_str().to_owned(),
            },
            duration_ms: None,
            error: Some(BuildFailureDocument {
                code: "not-run-after-interrupt",
                stage: None,
            }),
        }
    }
}

/// Serialize one post-parse failure through the stable top-level error envelope.
fn write_build_error(
    code: &'static str,
    stage: Option<&'static str>,
    project: Option<&str>,
    localizer: &Localizer,
) {
    let document = BuildErrorDocument {
        schema_version: 1,
        status: "error",
        error: BuildErrorDetailDocument {
            code,
            stage,
            project: project.map(str::to_owned),
        },
    };
    match serde_json::to_string_pretty(&document) {
        Ok(json) => println!("{json}"),
        Err(_) => eprintln!("{}", localizer.text("build-json-error")),
    }
}

/// Map selector failures to stable machine-facing codes.
const fn selection_error_code(error: &SelectionError) -> &'static str {
    match error {
        SelectionError::InvalidName(_) => "project-name-invalid",
        SelectionError::DuplicateSelector { .. } => "project-selector-duplicate",
        SelectionError::UnknownProject { .. } => "project-not-found",
        SelectionError::WorkspaceSelectorForStandalone => "workspace-selector-for-standalone",
        SelectionError::ConflictingSelectors => "project-selectors-conflict",
        SelectionError::ExplicitProjectRequired => "project-selector-required",
        SelectionError::SingleProjectRequired => "single-project-required",
    }
}

/// Map immutable plan failures to stable machine-facing codes.
const fn plan_error_code(error: &PlanError) -> &'static str {
    match error {
        PlanError::ProjectNameMissing => "project-name-missing",
        PlanError::PlatformVersionMissing => "platform-version-missing",
        PlanError::InvalidOutput { .. } => "output-invalid",
        PlanError::UnexpectedExtension { .. } => "output-extension-invalid",
        PlanError::OutputCollision { .. } => "output-collision",
        PlanError::BaseConfigurationUnsupported => "base-configuration-unsupported",
    }
}

/// Map tool discovery failures to stable machine-facing codes.
const fn tool_error_code(error: &ToolError) -> &'static str {
    match error {
        ToolError::InvalidArchitecture(_) => "platform-architecture-invalid",
        ToolError::InvalidContainer(_) => "distrobox-container-invalid",
        ToolError::InvalidExecutable(_) => "ibcmd-executable-invalid",
        ToolError::DistroboxContainerRequired => "distrobox-container-required",
        ToolError::Scan { .. } | ToolError::ScanCommandFailed { .. } => "platform-scan-failed",
        ToolError::NotFound { .. } => "ibcmd-not-found",
        ToolError::Run(_) => "ibcmd-run-failed",
        ToolError::VersionCommandFailed { .. } => "ibcmd-version-command-failed",
        ToolError::VersionUnreadable(_) => "ibcmd-version-unreadable",
        ToolError::VersionMismatch { .. } => "ibcmd-version-mismatch",
    }
}

fn write_workspace_json(entries: Vec<WorkspaceBuildEntry>, localizer: &Localizer) -> bool {
    let document = WorkspaceBuildDocument {
        schema_version: 1,
        projects: entries,
    };
    let Ok(json) = serde_json::to_string_pretty(&document) else {
        eprintln!("{}", localizer.text("build-json-error"));
        return false;
    };
    println!("{json}");
    true
}

const fn execution_error_code(error: &BuildExecutionError) -> &'static str {
    let BuildExecutionError::Build(error) = error else {
        return "output-write";
    };
    match error {
        BuildError::OutputParentMissing(_) => "output-parent-missing",
        BuildError::ConfiguredOutputOutsideProject { .. } => "output-outside-scope",
        BuildError::InvalidExistingOutput(_) => "output-existing-invalid",
        BuildError::CreateDirectory { .. } => "create-directory",
        BuildError::CreateWorkspace { .. } => "create-workspace",
        BuildError::Run {
            source: RunError::Interrupted,
            ..
        } => "interrupted",
        BuildError::Run { .. } => "process-error",
        BuildError::CommandFailed { .. } => "command-failed",
        BuildError::ArtifactMissing(_) => "artifact-missing",
        BuildError::ArtifactEmpty(_) => "artifact-empty",
        BuildError::Publish { .. } => "publish",
        BuildError::Restore { .. } => "restore",
        BuildError::DescriptorDirectory { .. } => "descriptor-directory",
        BuildError::DescriptorRead { .. } => "descriptor-read",
        BuildError::DescriptorInvalid { .. } => "descriptor-invalid",
        BuildError::DescriptorMissing(_) => "descriptor-missing",
        BuildError::DescriptorsMultiple(_) => "descriptors-multiple",
        BuildError::BaseConfigurationRead { .. } => "base-configuration-read",
        BuildError::BaseConfigurationInvalid(_) => "base-configuration-invalid",
        BuildError::Manifest(ManifestError::SnapshotIo { .. }) => "snapshot-io",
        BuildError::Manifest(ManifestError::SnapshotEntryUnsupported(_)) => {
            "snapshot-entry-unsupported"
        }
        BuildError::Manifest(ManifestError::SnapshotProject(_)) => "snapshot-invalid",
        BuildError::Manifest(ManifestError::ArtifactRead { .. }) => "artifact-checksum",
        BuildError::Manifest(ManifestError::Write { .. }) => "manifest-write",
        BuildError::Manifest(ManifestError::Serialize(_)) => "manifest-serialize",
    }
}

const fn execution_error_stage(error: &BuildExecutionError) -> Option<&'static str> {
    match error {
        BuildExecutionError::Build(
            BuildError::Run { stage, .. } | BuildError::CommandFailed { stage, .. },
        ) => Some(match stage {
            BuildStage::CreateInfobase => "create-infobase",
            BuildStage::ImportSources => "import-sources",
        }),
        BuildExecutionError::Build(BuildError::Manifest(
            ManifestError::ArtifactRead { .. }
            | ManifestError::Write { .. }
            | ManifestError::Serialize(_),
        )) => Some("manifest"),
        BuildExecutionError::Build(BuildError::Manifest(_)) => Some("snapshot"),
        BuildExecutionError::Build(_) | BuildExecutionError::Output(_) => None,
    }
}

const fn execution_was_interrupted(error: &BuildExecutionError) -> bool {
    matches!(
        error,
        BuildExecutionError::Build(BuildError::Run {
            source: RunError::Interrupted,
            ..
        })
    )
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
