//! Arguments, prompts, diagnostics and help for `eska init`.

pub use eska_presentation::arguments::project::init::{InitArgs, localize};

use std::{
    io::{self, IsTerminal},
    path::Path,
    process::ExitCode,
};

use crate::onboarding::{
    cli::{
        diagnostics,
        interactive::{PromptError, Selector, WORKFLOW_CHOICES},
        localization::{LocalizationValue, Localizer},
    },
    project::{
        ProjectName,
        build::BuildSettings,
        init::{self, InitError},
        onboarding,
    },
    vcs::workflow::WorkflowPreset,
};

use super::artifact::{self, Failure};

struct InitHandler<'a> {
    args: &'a InitArgs,
}

impl InitHandler<'_> {
    /// Keep one cancellation owner alive until every file transaction has committed or rolled back.
    pub fn run(&self, base: &Path, localizer: &Localizer) -> ExitCode {
        let scope = match eska_tool_runtime::Operation::new() {
            Ok(scope) => scope,
            Err(error) => {
                eprintln!(
                    "{}",
                    eska_presentation::components::present(&error, localizer)
                );
                return ExitCode::FAILURE;
            }
        };
        self.run_interruptible(base, localizer, &scope)
    }

    /// Resolve options and context under the same operation scope as the final write.
    fn run_interruptible(
        &self,
        base: &Path,
        localizer: &Localizer,
        scope: &eska_tool_runtime::Operation,
    ) -> ExitCode {
        if self
            .args
            .workflow
            .as_deref()
            .is_some_and(|value| WorkflowPreset::from_name(value).is_none())
        {
            eprintln!("{}", localizer.text("new-workflow-invalid"));
            return ExitCode::from(2);
        }
        let plan = match init::inspect(&base.join(&self.args.path), self.args.source.as_deref()) {
            Ok(plan) => plan,
            Err(error) => {
                eprintln!("{}", present(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let workspace = match onboarding::find_workspace(plan.root()) {
            Ok(workspace) => workspace,
            Err(error) => {
                eprintln!("{}", diagnostics::present_context_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        if let Some(workspace) = workspace {
            return self.run_workspace(plan, &workspace, localizer, scope);
        }
        if self.args.application.is_some() {
            eprintln!(
                "{}",
                localizer.text("workspace-onboarding-application-only")
            );
            return ExitCode::from(2);
        }
        self.run_standalone(&plan, localizer, scope)
    }

    /// Select standalone policy and platform before writing any project files.
    fn run_standalone(
        &self,
        plan: &init::InitPlan,
        localizer: &Localizer,
        scope: &eska_tool_runtime::Operation,
    ) -> ExitCode {
        let mut workflow = match self.args.workflow.as_deref() {
            Some(value) => {
                let Some(preset) = WorkflowPreset::from_name(value) else {
                    eprintln!("{}", localizer.text("new-workflow-invalid"));
                    return ExitCode::from(2);
                };
                Some(preset)
            }
            None => None,
        };
        if self.args.name.is_some() {
            eprintln!("{}", localizer.text("init-name-workspace-only"));
            return ExitCode::from(2);
        }
        if workflow.is_none() {
            if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
                eprintln!("{}", localizer.text("init-options-required"));
                return ExitCode::from(2);
            }
            let result = (|| {
                let mut selector = Selector::start("init-tui-title")?;
                let value = selector.choose(localizer, "new-workflow-menu", &WORKFLOW_CHOICES)?;
                selector.finish().map_err(|_| PromptError::Io)?;
                WorkflowPreset::from_name(&value).ok_or(PromptError::Io)
            })();
            match result {
                Ok(preset) => workflow = Some(preset),
                Err(error) => {
                    eprintln!(
                        "{}",
                        localizer.text(match error {
                            PromptError::Cancelled => "init-cancelled",
                            PromptError::Io => "new-prompt-error",
                        })
                    );
                    return ExitCode::FAILURE;
                }
            }
        }
        let Some(workflow) = workflow else {
            return ExitCode::from(2);
        };
        let build = match self.build_settings(localizer, scope) {
            Ok(build) => build,
            Err(error) => return platform_failure(&error),
        };
        match init::apply(plan, workflow, build, !self.args.no_vcs, &|| {
            scope.is_cancelled()
        }) {
            Ok(project) => {
                println!(
                    "{}",
                    localizer.format(
                        "init-created",
                        &[(
                            "path",
                            LocalizationValue::Text(&project.root().to_string_lossy())
                        )]
                    )
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{}", present(&error, localizer));
                ExitCode::FAILURE
            }
        }
    }

    /// Preflight member enrollment before selecting an optional platform override.
    fn run_workspace(
        &self,
        plan: init::InitPlan,
        workspace: &crate::onboarding::project::Workspace,
        localizer: &Localizer,
        scope: &eska_tool_runtime::Operation,
    ) -> ExitCode {
        if self.args.workflow.is_some() {
            eprintln!("{}", localizer.text("workspace-onboarding-workflow-owned"));
            return ExitCode::from(2);
        }
        let inferred = plan
            .root()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let raw_name = self.args.name.as_deref().unwrap_or(inferred);
        let Ok(name) = ProjectName::parse(raw_name.to_owned()) else {
            eprintln!(
                "{}",
                localizer.format(
                    "init-member-name-invalid",
                    &[("name", LocalizationValue::Text(raw_name))]
                )
            );
            return ExitCode::from(2);
        };
        let plan = match init::prepare_workspace_member(plan, workspace, name) {
            Ok(plan) => plan.with_application(self.args.application.as_deref()),
            Err(error) => {
                eprintln!("{}", present(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let plan = if self.args.platform.platform_version.is_some()
            || self.args.platform.select_platform
            || workspace.build_settings().platform_version().is_none()
        {
            match self.build_settings(localizer, scope) {
                Ok(build) => plan.with_build_settings(build),
                Err(error) => return platform_failure(&error),
            }
        } else {
            plan
        };
        match init::apply_workspace_member(&plan, &|| scope.is_cancelled()) {
            Ok(project) => {
                println!(
                    "{}",
                    localizer.format(
                        "init-member-created",
                        &[(
                            "path",
                            LocalizationValue::Text(&project.root().to_string_lossy())
                        )]
                    )
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{}", present(&error, localizer));
                ExitCode::FAILURE
            }
        }
    }

    /// Store the selected version without persisting machine-local runner settings.
    fn build_settings(
        &self,
        localizer: &Localizer,
        scope: &eska_tool_runtime::Operation,
    ) -> Result<BuildSettings, Failure> {
        let tool = artifact::resolve_for_init(&self.args.platform, localizer, scope)?;
        BuildSettings::new(
            tool.as_ref().map_or("", |tool| tool.version().as_str()),
            "build".into(),
        )
        .map_err(|_| {
            Failure::new(
                "platform-version-invalid",
                localizer.text("artifact-version-invalid"),
            )
        })
    }
}

/// Match creation's exit codes while keeping initialization's existing text presentation.
fn platform_failure(error: &Failure) -> ExitCode {
    if !error.message.is_empty() {
        eprintln!("{}", error.message);
    }
    if matches!(
        error.code.as_str(),
        "platform-required" | "platform-version-invalid"
    ) {
        ExitCode::from(2)
    } else {
        ExitCode::FAILURE
    }
}

/// Translate core initialization errors at the CLI boundary.
fn present(error: &InitError, localizer: &Localizer) -> String {
    let (key, path) = match error {
        InitError::Io { path, .. } => ("init-io-error", path),
        InitError::InvalidRoot { path } => ("init-root-invalid", path),
        InitError::ExistingConfig { path } => ("init-config-exists", path),
        InitError::InvalidSource { path } => ("init-source-invalid", path),
        InitError::MissingSource { path } => ("init-source-missing", path),
        InitError::AmbiguousSource { path } => ("init-source-ambiguous", path),
        InitError::MultipleDescriptors { path } => ("init-descriptors-multiple", path),
        InitError::InvalidDescriptor { path } => ("init-descriptor-invalid", path),
        InitError::InvalidXml { path, .. } => ("init-xml-invalid", path),
        InitError::DescriptorTooLarge { path } => ("init-descriptor-large", path),
        InitError::Operation(init::InitFailure::ExistingGit { path, .. }) => {
            ("init-git-invalid", path)
        }
        InitError::Operation(init::InitFailure::Cancelled) => {
            return localizer.text("init-cancelled");
        }
        InitError::ChangedSource { path } => ("init-source-changed", path),
        InitError::Config(_) => return localizer.text("init-source-path-invalid"),
        InitError::Serialize(_) => return localizer.text("init-config-error"),
        InitError::Operation(init::InitFailure::Vcs(error)) => {
            return eska_presentation::components::present(error, localizer);
        }
        InitError::Validation(error) => {
            return diagnostics::present_project_error(error, localizer);
        }
        InitError::Workspace(error) => {
            return diagnostics::present_workspace_enrollment_error(error, localizer);
        }
        InitError::WorkspaceValidation(error) => {
            return diagnostics::present_context_error(error, localizer);
        }
        InitError::WorkspaceMemberMissing { name } => {
            return localizer.format(
                "workspace-onboarding-member-missing",
                &[("name", LocalizationValue::Text(name.as_str()))],
            );
        }
        InitError::Rollback { paths, original } => {
            let paths = paths
                .iter()
                .map(|path| path.to_string_lossy())
                .collect::<Vec<_>>()
                .join(", ");
            return localizer.format(
                "new-rollback-error",
                &[
                    ("path", LocalizationValue::Text(&paths)),
                    (
                        "reason",
                        LocalizationValue::Text(&present(original, localizer)),
                    ),
                ],
            );
        }
    };
    localizer.format(
        key,
        &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
    )
}

/// Execute compatibility initialization through the existing project transaction.
pub fn run(args: &InitArgs, directory: &Path, localizer: &Localizer) -> ExitCode {
    InitHandler { args }.run(directory, localizer)
}
