use std::{
    io::{self, BufRead, IsTerminal, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{
    creation::{self, CreationError},
    localization::{LocalizationValue, Localizer},
    project::WorkflowPreset,
};
use clap::{ArgAction, Args};

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
            let mut input = io::stdin().lock();
            let mut output = io::stderr().lock();
            let result = (|| {
                if project_type.is_none() {
                    project_type = Some(choose(
                        &mut input,
                        &mut output,
                        localizer,
                        "new-type-menu",
                        &["configuration", "extension", "processing", "report"],
                    )?);
                }
                if workflow.is_none() {
                    workflow = Some(choose(
                        &mut input,
                        &mut output,
                        localizer,
                        "new-workflow-menu",
                        &["trunk", "git-flow", "github-flow", "custom"],
                    )?);
                }
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

#[derive(Debug)]
enum PromptError {
    Cancelled,
    Io,
}

fn choose(
    input: &mut impl BufRead,
    output: &mut impl Write,
    localizer: &Localizer,
    menu: &str,
    choices: &[&str],
) -> Result<String, PromptError> {
    writeln!(output, "{}", localizer.text(menu)).map_err(|_| PromptError::Io)?;
    loop {
        write!(output, "{} ", localizer.text("new-choice-prompt")).map_err(|_| PromptError::Io)?;
        output.flush().map_err(|_| PromptError::Io)?;
        let mut line = String::new();
        if input.read_line(&mut line).map_err(|_| PromptError::Io)? == 0 {
            return Err(PromptError::Cancelled);
        }
        let value = line.trim();
        if let Some(choice) = value
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| choices.get(index))
        {
            return Ok((*choice).to_owned());
        }
        if choices.contains(&value) {
            return Ok(value.to_owned());
        }
        writeln!(output, "{}", localizer.text("new-choice-invalid"))
            .map_err(|_| PromptError::Io)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::localization::Locale;

    #[test]
    fn prompts_accept_numbers_and_names_retry_invalid_input_and_handle_eof() {
        for locale in [Locale::RuRu, Locale::EnUs] {
            let localizer = Localizer::try_new(locale).expect("valid locale");
            for (input, expected) in [("0\n5\nwrong\n2\n", "extension"), ("report\n", "report")] {
                let mut input = io::Cursor::new(input);
                let mut output = Vec::new();
                assert_eq!(
                    choose(
                        &mut input,
                        &mut output,
                        &localizer,
                        "new-type-menu",
                        &["configuration", "extension", "processing", "report"]
                    )
                    .expect("choice"),
                    expected
                );
                assert!(
                    String::from_utf8(output)
                        .expect("UTF-8")
                        .contains(&localizer.text("new-choice-prompt"))
                );
            }
            assert!(matches!(
                choose(
                    &mut io::Cursor::new(""),
                    &mut Vec::new(),
                    &localizer,
                    "new-type-menu",
                    &["configuration"]
                ),
                Err(PromptError::Cancelled)
            ));
        }
    }
}
