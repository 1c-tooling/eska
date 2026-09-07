//! Arguments, prompts, diagnostics and help for `eska init`.

use std::{
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{
    cli::{
        diagnostics,
        interactive::{PromptError, Selector, WORKFLOW_CHOICES},
        localization::{LocalizationValue, Localizer},
    },
    project::{
        ProjectName,
        init::{self, InitError},
        onboarding,
    },
    vcs::workflow::WorkflowPreset,
};
use clap::{ArgAction, Args};

#[derive(Debug, Args)]
pub(in crate::cli) struct InitArgs {
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    source: Option<PathBuf>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    workflow: Option<String>,
    #[arg(long)]
    no_vcs: bool,
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
}

impl InitArgs {
    pub(super) fn run(&self, base: &Path, localizer: &Localizer) -> ExitCode {
        if self
            .workflow
            .as_deref()
            .is_some_and(|value| WorkflowPreset::from_name(value).is_none())
        {
            eprintln!("{}", localizer.text("new-workflow-invalid"));
            return ExitCode::from(2);
        }
        let plan = match init::inspect(&base.join(&self.path), self.source.as_deref()) {
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
            return self.run_workspace(plan, &workspace, localizer);
        }
        self.run_standalone(&plan, localizer)
    }

    /// Completes the legacy standalone initialization flow.
    fn run_standalone(&self, plan: &init::InitPlan, localizer: &Localizer) -> ExitCode {
        let mut workflow = match self.workflow.as_deref() {
            Some(value) => {
                let Some(preset) = WorkflowPreset::from_name(value) else {
                    eprintln!("{}", localizer.text("new-workflow-invalid"));
                    return ExitCode::from(2);
                };
                Some(preset)
            }
            None => None,
        };
        if self.name.is_some() {
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
        match init::apply(plan, workflow, !self.no_vcs) {
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

    /// Attaches one copied export to its containing workspace without a prompt.
    fn run_workspace(
        &self,
        plan: init::InitPlan,
        workspace: &crate::project::Workspace,
        localizer: &Localizer,
    ) -> ExitCode {
        if self.workflow.is_some() {
            eprintln!("{}", localizer.text("workspace-onboarding-workflow-owned"));
            return ExitCode::from(2);
        }
        let inferred = plan
            .root()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let raw_name = self.name.as_deref().unwrap_or(inferred);
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
            Ok(plan) => plan,
            Err(error) => {
                eprintln!("{}", present(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        match init::apply_workspace_member(&plan) {
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
}

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
        InitError::ExistingGit { path, .. } => ("init-git-invalid", path),
        InitError::ChangedSource { path } => ("init-source-changed", path),
        InitError::Config(_) => return localizer.text("init-source-path-invalid"),
        InitError::Serialize(_) => return localizer.text("init-config-error"),
        InitError::Git(_) => return localizer.text("new-git-error"),
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

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("init-about"))
        .override_usage(localizer.text("init-usage"))
        .help_template(format!(
            "{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{positionals}}\n\n{}:\n{{options}}",
            localizer.text("cli-usage"),
            localizer.text("cli-arguments"),
            localizer.text("cli-options")
        ))
        .mut_arg("path", |arg| {
            arg.help(localizer.text("init-path-help"))
                .value_name(localizer.text("cli-project-dir-value"))
                .hide_default_value(true)
        })
        .mut_arg("source", |arg| {
            arg.help(localizer.text("init-source-help"))
                .value_name(localizer.text("cli-project-dir-value"))
        })
        .mut_arg("name", |arg| {
            arg.help(localizer.text("init-name-help"))
                .value_name(localizer.text("init-name-value"))
        })
        .mut_arg("workflow", |arg| {
            arg.help(localizer.text("new-workflow-help"))
                .value_name(localizer.text("new-workflow-value"))
        })
        .mut_arg("no_vcs", |arg| arg.help(localizer.text("new-no-vcs-help")))
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
