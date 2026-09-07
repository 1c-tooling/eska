//! Arguments, prompts, diagnostics and help for `eska new`.

use std::{
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{
    cli::{
        diagnostics,
        interactive::{PROJECT_TYPE_CHOICES, PromptError, Selector, WORKFLOW_CHOICES},
        localization::{LocalizationValue, Localizer},
    },
    project::{
        ProjectName,
        create::{self, CreationError},
        onboarding,
    },
    vcs::workflow::WorkflowPreset,
};
use clap::{ArgAction, Args};

#[derive(Debug, Args)]
pub(in crate::cli) struct NewArgs {
    path: PathBuf,
    #[arg(long = "type")]
    project_type: Option<String>,
    #[arg(long)]
    workflow: Option<String>,
    #[arg(long)]
    no_vcs: bool,
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
}

impl NewArgs {
    pub(super) fn run(&self, base: &Path, localizer: &Localizer) -> ExitCode {
        let workspace = match onboarding::find_workspace(base) {
            Ok(workspace) => workspace,
            Err(error) => {
                eprintln!("{}", diagnostics::present_context_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        if let Some(workspace) = workspace {
            return self.run_workspace(&workspace, localizer);
        }
        self.run_standalone(base, localizer)
    }

    /// Runs the legacy standalone-project creation flow unchanged.
    fn run_standalone(&self, base: &Path, localizer: &Localizer) -> ExitCode {
        let destination = base.join(&self.path);
        let destination = match create::resolve_destination(&destination) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{}", present(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let mut project_type = self.project_type.clone();
        let mut workflow = self.workflow.clone();
        if project_type
            .as_ref()
            .is_some_and(|value| crate::config::parse_project_type(value.clone()).is_err())
        {
            eprintln!("{}", localizer.text("new-type-invalid"));
            return ExitCode::from(2);
        }
        if workflow
            .as_deref()
            .is_some_and(|value| WorkflowPreset::from_name(value).is_none())
        {
            eprintln!("{}", localizer.text("new-workflow-invalid"));
            return ExitCode::from(2);
        }
        if project_type.is_none() || workflow.is_none() {
            if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
                eprintln!("{}", localizer.text("new-options-required"));
                return ExitCode::from(2);
            }
            let result = (|| {
                let mut selector = Selector::start("new-tui-title")?;
                if project_type.is_none() {
                    project_type =
                        Some(selector.choose(localizer, "new-type-menu", &PROJECT_TYPE_CHOICES)?);
                }
                if workflow.is_none() {
                    workflow =
                        Some(selector.choose(localizer, "new-workflow-menu", &WORKFLOW_CHOICES)?);
                }
                selector.finish().map_err(|_| PromptError::Io)?;
                Ok::<_, PromptError>(())
            })();
            if let Err(error) = result {
                eprintln!(
                    "{}",
                    localizer.text(match error {
                        PromptError::Cancelled => "new-cancelled",
                        PromptError::Io => "new-prompt-error",
                    })
                );
                return ExitCode::FAILURE;
            }
        }
        let Some(project_type) =
            project_type.and_then(|value| crate::config::parse_project_type(value).ok())
        else {
            eprintln!("{}", localizer.text("new-type-invalid"));
            return ExitCode::from(2);
        };
        let Some(workflow) = workflow.and_then(|value| WorkflowPreset::from_name(&value)) else {
            eprintln!("{}", localizer.text("new-workflow-invalid"));
            return ExitCode::from(2);
        };
        match create::create(&destination, project_type, workflow, !self.no_vcs) {
            Ok(project) => {
                println!(
                    "{}",
                    localizer.format(
                        "new-created",
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

    /// Creates one conventional `src/<name>` member and enrolls it transactionally.
    fn run_workspace(
        &self,
        workspace: &crate::project::Workspace,
        localizer: &Localizer,
    ) -> ExitCode {
        let Some(name) = workspace_project_name(&self.path) else {
            eprintln!(
                "{}",
                localizer.format(
                    "new-member-name-invalid",
                    &[(
                        "name",
                        LocalizationValue::Text(&self.path.to_string_lossy())
                    )]
                )
            );
            return ExitCode::from(2);
        };
        if self.workflow.is_some() {
            eprintln!("{}", localizer.text("workspace-onboarding-workflow-owned"));
            return ExitCode::from(2);
        }
        let plan = match create::inspect_workspace_member(workspace, name) {
            Ok(plan) => plan,
            Err(error) => {
                eprintln!("{}", present(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let mut project_type = self.project_type.clone();
        if project_type
            .as_ref()
            .is_some_and(|value| crate::config::parse_project_type(value.clone()).is_err())
        {
            eprintln!("{}", localizer.text("new-type-invalid"));
            return ExitCode::from(2);
        }
        if project_type.is_none() {
            if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
                eprintln!("{}", localizer.text("new-member-type-required"));
                return ExitCode::from(2);
            }
            let result = (|| {
                let mut selector = Selector::start("new-tui-title")?;
                project_type =
                    Some(selector.choose(localizer, "new-type-menu", &PROJECT_TYPE_CHOICES)?);
                selector.finish().map_err(|_| PromptError::Io)
            })();
            if let Err(error) = result {
                eprintln!(
                    "{}",
                    localizer.text(match error {
                        PromptError::Cancelled => "new-cancelled",
                        PromptError::Io => "new-prompt-error",
                    })
                );
                return ExitCode::FAILURE;
            }
        }
        let Some(project_type) =
            project_type.and_then(|value| crate::config::parse_project_type(value).ok())
        else {
            eprintln!("{}", localizer.text("new-type-invalid"));
            return ExitCode::from(2);
        };
        match create::create_workspace_member(&plan, project_type) {
            Ok(project) => {
                println!(
                    "{}",
                    localizer.format(
                        "new-member-created",
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

/// Parses the workspace form of `new` as one portable project name.
fn workspace_project_name(path: &Path) -> Option<ProjectName> {
    let mut components = path.components();
    let component = components.next()?;
    if components.next().is_some() {
        return None;
    }
    let std::path::Component::Normal(name) = component else {
        return None;
    };
    ProjectName::parse(name.to_str()?.to_owned()).ok()
}

fn present(error: &CreationError, localizer: &Localizer) -> String {
    let (key, path) = match error {
        CreationError::InvalidDestination { path } => ("new-destination-invalid", path),
        CreationError::AlreadyExists { path } => ("new-destination-exists", path),
        CreationError::Io { path, .. } => ("new-io-error", path),
        CreationError::Template(_) => return localizer.text("new-template-error"),
        CreationError::Git(_) => return localizer.text("new-git-error"),
        CreationError::Validation(error) => {
            return diagnostics::present_project_error(error, localizer);
        }
        CreationError::Workspace(error) => {
            return diagnostics::present_workspace_enrollment_error(error, localizer);
        }
        CreationError::WorkspaceValidation(error) => {
            return diagnostics::present_context_error(error, localizer);
        }
        CreationError::WorkspaceMemberMissing { name } => {
            return localizer.format(
                "workspace-onboarding-member-missing",
                &[("name", LocalizationValue::Text(name.as_str()))],
            );
        }
        CreationError::Rollback { path, original, .. } => {
            return localizer.format(
                "new-rollback-error",
                &[
                    ("path", LocalizationValue::Text(&path.to_string_lossy())),
                    (
                        "reason",
                        LocalizationValue::Text(&present(original, localizer)),
                    ),
                ],
            );
        }
        CreationError::WorkspaceRollback { paths, original } => {
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
        .about(localizer.text("new-about"))
        .override_usage(localizer.text("new-usage"))
        .help_template(format!(
            "{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{positionals}}\n\n{}:\n{{options}}",
            localizer.text("cli-usage"),
            localizer.text("cli-arguments"),
            localizer.text("cli-options")
        ))
        .mut_arg("path", |arg| {
            arg.help(localizer.text("new-path-help"))
                .value_name(localizer.text("cli-project-dir-value"))
        })
        .mut_arg("project_type", |arg| {
            arg.help(localizer.text("new-type-help"))
                .value_name(localizer.text("new-type-value"))
        })
        .mut_arg("workflow", |arg| {
            arg.help(localizer.text("new-workflow-help"))
                .value_name(localizer.text("new-workflow-value"))
        })
        .mut_arg("no_vcs", |arg| arg.help(localizer.text("new-no-vcs-help")))
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
