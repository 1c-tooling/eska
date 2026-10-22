//! Build arguments, project selection and preflighted command execution.

use eska_presentation::arguments::build::OutputFormat;
pub use eska_presentation::arguments::build::{BuildArgs, localize};

use std::{
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{
    cli::{
        diagnostics,
        interactive::{PromptError, Selector},
        localization::{LocalizationValue, Localizer},
        platform,
    },
    project::{
        Project, ProjectName,
        build::{self, BuildPlan, BuildStage, Ibcmd, PlanError, PlatformVersion, ToolOptions},
        discovery::{self, DiscoveryContext},
        selection::{SelectedProject, SelectionError, SelectionIntent, select_projects},
    },
};

mod errors;
mod json;
mod output;

use crate::cli::process_output::{diagnostic_styling_enabled, progress::ProgressLine};

use errors::{
    BuildExecutionError, execution_error_code, execution_error_message, execution_error_stage,
    execution_was_interrupted, plan_error_code, present_plan_error, present_platform_version_error,
    selection_error_code, tool_error_code,
};
use json::{WorkspaceBuildEntry, write_build_error, write_workspace_json};
use output::{write_build_preview, write_build_result, write_build_started, write_diagnostic};

struct PreparedBuild<'a> {
    name: Option<&'a ProjectName>,
    project: &'a Project,
    plan: BuildPlan,
}

struct BuildHandler<'a> {
    options: &'a BuildArgs,
}

impl BuildHandler<'_> {
    /// Discover, preflight and sequentially build the selected projects.
    pub fn run(
        &self,
        project_dir: &Path,
        localizer: &Localizer,
        provenance: &dyn Fn(&Path) -> Result<build::GitProvenance, eska_tool_runtime::Error>,
    ) -> ExitCode {
        let cancellation = match super::artifact::cancellation_scope(localizer) {
            Ok(cancellation) => cancellation,
            Err(error) => {
                return self.fail(
                    error.code,
                    Some("tool-discovery"),
                    None,
                    &error.message,
                    localizer,
                    ExitCode::FAILURE,
                );
            }
        };
        let guarded_provenance = |root: &Path| {
            if cancellation
                .as_ref()
                .is_some_and(eska_tool_runtime::Operation::is_cancelled)
            {
                return Err(eska_tool_runtime::Error::Cancelled);
            }
            provenance(root)
        };
        self.run_interruptible(
            project_dir,
            localizer,
            &guarded_provenance,
            cancellation.as_ref(),
        )
    }

    /// Retain one caller cancellation scope through all preflighted members and native operations.
    fn run_interruptible(
        &self,
        project_dir: &Path,
        localizer: &Localizer,
        provenance: &dyn Fn(&Path) -> Result<build::GitProvenance, eska_tool_runtime::Error>,
        cancellation: Option<&eska_tool_runtime::Operation>,
    ) -> ExitCode {
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
            &self.options.project,
            self.options.workspace,
            SelectionIntent::MultipleMutation,
        ) {
            Ok(selection) => selection,
            Err(error) => return self.fail_selection(&error, localizer),
        };
        if let Err(code) = self.validate_output_scope(selection.projects().len(), localizer) {
            return code;
        }
        let options = match self.tool_options(localizer, cancellation) {
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
                self.options.output.as_deref(),
                platform_override.clone(),
                base_configuration.clone(),
                self.options.recreate_infobase,
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
        if let Err(code) = preflight_group(
            self.options.format,
            &prepared,
            self.options.mode.manifest,
            localizer,
        ) {
            return code;
        }
        let tools = match self.operation_tools(&prepared, &options, cancellation, localizer) {
            Ok(tools) => tools,
            Err(code) => return code,
        };
        if self.options.mode.dry_run {
            return write_build_preview(
                self.options.format,
                &prepared,
                &tools,
                selection.is_aggregate(),
                localizer,
            );
        }
        if selection.is_aggregate() {
            self.execute_aggregate(&prepared, &tools, localizer, provenance)
        } else if let (Some(prepared), Some(ibcmd)) = (prepared.first(), tools.first()) {
            self.execute_single(prepared, ibcmd, localizer, provenance)
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

    /// Reject a shared output before planning multiple selected projects.
    fn validate_output_scope(&self, count: usize, localizer: &Localizer) -> Result<(), ExitCode> {
        if self.options.output.is_some() && count != 1 {
            return Err(self.fail(
                "output-requires-single-project",
                Some("selection"),
                None,
                &localizer.text("build-output-single-project"),
                localizer,
                ExitCode::FAILURE,
            ));
        }
        Ok(())
    }

    /// Bind each native runner to the caller's scope before preview or execution.
    fn operation_tools(
        &self,
        prepared: &[PreparedBuild<'_>],
        options: &ToolOptions,
        cancellation: Option<&eska_tool_runtime::Operation>,
        localizer: &Localizer,
    ) -> Result<Vec<Ibcmd>, ExitCode> {
        let tools = discover_tools(self.options.format, prepared, options, localizer)?;
        if cancellation.is_some_and(eska_tool_runtime::Operation::is_cancelled) {
            return Err(self.fail(
                "interrupted",
                Some("tool-discovery"),
                None,
                &localizer.text("component-cancelled"),
                localizer,
                ExitCode::FAILURE,
            ));
        }
        Ok(tools
            .into_iter()
            .map(|tool| super::artifact::attach_cancellation(tool, cancellation))
            .collect())
    }

    /// Resolve the optional base CF once so every selected plan uses the same host path.
    fn resolved_base_configuration(&self, context: &DiscoveryContext) -> Option<PathBuf> {
        self.options
            .base_configuration
            .as_deref()
            .map(|path| resolve_base_configuration(path, context))
    }

    /// Resolve machine-local tool settings through the selected output contract.
    fn tool_options(
        &self,
        localizer: &Localizer,
        scope: Option<&eska_tool_runtime::Operation>,
    ) -> Result<ToolOptions, ExitCode> {
        platform::tool_options(
            self.options.ibcmd.clone(),
            self.options.platform_arch.clone(),
            self.options.distrobox.clone(),
        )
        .map(|options| super::artifact::scoped_options(options, scope))
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
        provenance: &dyn Fn(&Path) -> Result<build::GitProvenance, eska_tool_runtime::Error>,
    ) -> ExitCode {
        match self.execute_plan(prepared, ibcmd, localizer, provenance) {
            Ok(result) => {
                write_build_result(self.options.format, &prepared.plan, &result, localizer)
            }
            Err(error) => self.fail_project(
                execution_error_code(&error),
                execution_error_stage(&error),
                prepared.name,
                &execution_error_message(&error, localizer),
                localizer,
            ),
        }
    }

    /// Execute all preflighted members in manifest/selector order and retain every outcome.
    fn execute_aggregate(
        &self,
        prepared: &[PreparedBuild<'_>],
        tools: &[Ibcmd],
        localizer: &Localizer,
        provenance: &dyn Fn(&Path) -> Result<build::GitProvenance, eska_tool_runtime::Error>,
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
            if matches!(self.options.format, OutputFormat::Human) {
                eprintln!(
                    "{}",
                    localizer.format(
                        "build-member-started",
                        &[("name", LocalizationValue::Text(name.as_str()))],
                    )
                );
            }
            match self.execute_plan(item, ibcmd, localizer, provenance) {
                Ok(result) => {
                    if matches!(self.options.format, OutputFormat::Human) {
                        let _ =
                            write_build_result(OutputFormat::Human, &item.plan, &result, localizer);
                    }
                    entries.push(WorkspaceBuildEntry::success(name, &item.plan, &result));
                }
                Err(error) => {
                    failed = true;
                    let interrupted = execution_was_interrupted(&error);
                    if matches!(self.options.format, OutputFormat::Human) {
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
        if matches!(self.options.format, OutputFormat::Json)
            && !write_workspace_json(entries, localizer)
        {
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
        provenance: &dyn Fn(&Path) -> Result<build::GitProvenance, eska_tool_runtime::Error>,
    ) -> Result<build::BuildResult, BuildExecutionError> {
        let project = prepared.project;
        let plan = &prepared.plan;
        let human = matches!(self.options.format, OutputFormat::Human);
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
        let result = if self.options.mode.manifest {
            let project_name = prepared
                .name
                .map(ProjectName::as_str)
                .or_else(|| project.root().file_name().and_then(std::ffi::OsStr::to_str));
            build::execute_streaming_with_manifest(
                plan,
                project,
                project_name,
                provenance(project.root()).map_err(BuildExecutionError::Vcs)?,
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
        write_project_failure(self.options.format, code, stage, name, detail, localizer)
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
        write_failure(self.options.format, code, stage, project, detail, localizer);
        exit_code
    }

    /// Resolve a one-run explicit or interactive platform override.
    fn platform_override(
        &self,
        options: &ToolOptions,
        localizer: &Localizer,
    ) -> Result<Option<PlatformVersion>, ExitCode> {
        if let Some(value) = &self.options.platform_version {
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
        if !self.options.select_platform {
            return Ok(None);
        }
        if matches!(self.options.format, OutputFormat::Json)
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
    recreate_infobase: bool,
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
        .map(|plan| plan.with_recreate_infobase(recreate_infobase))
}

/// Validate every selected output before invoking a platform tool.
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

/// Resolve a matching platform for every prepared build before execution.
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

/// Execute parsed build options in their native owner.
pub fn run(
    args: &BuildArgs,
    directory: &Path,
    localizer: &Localizer,
    provenance: &dyn Fn(&Path) -> Result<build::GitProvenance, eska_tool_runtime::Error>,
) -> ExitCode {
    BuildHandler { options: args }.run(directory, localizer, provenance)
}
