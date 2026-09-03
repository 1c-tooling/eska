use std::{
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{
    creation::{self, CreationError},
    localization::{LocalizationValue, Localizer},
    project::WorkflowPreset,
};
use clap::{ArgAction, Args};

use super::select::{PromptError, Selector, WORKFLOW_CHOICES};

#[derive(Debug, Args)]
pub(super) struct NewArgs {
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
        let destination = base.join(&self.path);
        let destination = match creation::resolve_destination(&destination) {
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
                    project_type = Some(selector.choose(
                        localizer,
                        "new-type-menu",
                        &[
                            ("configuration", "new-type-configuration"),
                            ("extension", "new-type-extension"),
                            ("processing", "new-type-processing"),
                            ("report", "new-type-report"),
                        ],
                    )?);
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
        match creation::create(&destination, project_type, workflow, !self.no_vcs) {
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
}

fn present(error: &CreationError, localizer: &Localizer) -> String {
    let (key, path) = match error {
        CreationError::InvalidDestination { path } => ("new-destination-invalid", path),
        CreationError::AlreadyExists { path } => ("new-destination-exists", path),
        CreationError::Io { path, .. } => ("new-io-error", path),
        CreationError::Template(_) => return localizer.text("new-template-error"),
        CreationError::Git(_) => return localizer.text("new-git-error"),
        CreationError::Validation(error) => {
            return super::project_errors::present(error, localizer);
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
