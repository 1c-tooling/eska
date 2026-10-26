//! Arguments, prompts, diagnostics and help for `eska new`.

pub use eska_presentation::arguments::project::new::{NewArgs, localize};

use std::{
    io::{self, IsTerminal},
    path::Path,
    process::ExitCode,
};

use crate::onboarding::{
    cli::{
        diagnostics,
        interactive::{PROJECT_TYPE_CHOICES, PromptError, Selector, WORKFLOW_CHOICES},
        localization::{LocalizationValue, Localizer},
        path_output,
    },
    project::{
        ProjectName,
        create::{self, CreationError},
        onboarding,
    },
    vcs::workflow::WorkflowPreset,
};

use super::artifact;

mod from;

struct NewHandler<'a> {
    args: &'a NewArgs,
}

impl NewHandler<'_> {
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
        if self.args.from.is_some() {
            return from::run(self.args, base, localizer, scope);
        }
        if self.args.platform.is_set() {
            artifact::report("✗", &localizer.text("artifact-from-required"), "31");
            return ExitCode::from(2);
        }
        let workspace = match onboarding::find_workspace(base) {
            Ok(workspace) => workspace,
            Err(error) => {
                artifact::report(
                    "✗",
                    &diagnostics::present_context_error_with_links(
                        &error,
                        localizer,
                        io::stderr().is_terminal(),
                    ),
                    "31",
                );
                return ExitCode::FAILURE;
            }
        };
        if let Some(workspace) = workspace {
            return self.run_workspace(&workspace, localizer, scope);
        }
        if self.args.application.is_some() {
            artifact::report(
                "✗",
                &localizer.text("workspace-onboarding-application-only"),
                "31",
            );
            return ExitCode::from(2);
        }
        self.run_standalone(base, localizer, scope)
    }

    /// Preserve standalone creation policy while styling its human result.
    fn run_standalone(
        &self,
        base: &Path,
        localizer: &Localizer,
        scope: &eska_tool_runtime::Operation,
    ) -> ExitCode {
        let destination = base.join(&self.args.path);
        let destination = match create::resolve_destination(&destination) {
            Ok(path) => path,
            Err(error) => {
                artifact::report("✗", &present(&error, localizer), "31");
                return ExitCode::FAILURE;
            }
        };
        let mut project_type = self.args.project_type.clone();
        let mut workflow = self.args.workflow.clone();
        if project_type.as_ref().is_some_and(|value| {
            crate::onboarding::config::parse_project_type(value.clone()).is_err()
        }) {
            artifact::report("✗", &localizer.text("new-type-invalid"), "31");
            return ExitCode::from(2);
        }
        if workflow
            .as_deref()
            .is_some_and(|value| WorkflowPreset::from_name(value).is_none())
        {
            artifact::report("✗", &localizer.text("new-workflow-invalid"), "31");
            return ExitCode::from(2);
        }
        if project_type.is_none() || workflow.is_none() {
            if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
                artifact::report("✗", &localizer.text("new-options-required"), "31");
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
                artifact::report(
                    "✗",
                    &localizer.text(match error {
                        PromptError::Cancelled => "new-cancelled",
                        PromptError::Io => "new-prompt-error",
                    }),
                    "31",
                );
                return ExitCode::FAILURE;
            }
        }
        let Some(project_type) = project_type
            .and_then(|value| crate::onboarding::config::parse_project_type(value).ok())
        else {
            artifact::report("✗", &localizer.text("new-type-invalid"), "31");
            return ExitCode::from(2);
        };
        let Some(workflow) = workflow.and_then(|value| WorkflowPreset::from_name(&value)) else {
            artifact::report("✗", &localizer.text("new-workflow-invalid"), "31");
            return ExitCode::from(2);
        };
        match create::create(
            &destination,
            project_type,
            workflow,
            !self.args.no_vcs,
            &|| scope.is_cancelled(),
        ) {
            Ok(project) => {
                artifact::success(&localizer.format(
                    "new-created",
                    &[(
                        "path",
                        LocalizationValue::Text(&path_output::render(
                            project.root(),
                            io::stdout().is_terminal(),
                        )),
                    )],
                ));
                ExitCode::SUCCESS
            }
            Err(error) => {
                artifact::report("✗", &present(&error, localizer), "31");
                ExitCode::FAILURE
            }
        }
    }

    /// Creates one conventional `src/<name>` member and enrolls it transactionally.
    fn run_workspace(
        &self,
        workspace: &crate::onboarding::project::Workspace,
        localizer: &Localizer,
        scope: &eska_tool_runtime::Operation,
    ) -> ExitCode {
        let Some(name) = workspace_project_name(&self.args.path) else {
            artifact::report(
                "✗",
                &localizer.format(
                    "new-member-name-invalid",
                    &[(
                        "name",
                        LocalizationValue::Text(&self.args.path.to_string_lossy()),
                    )],
                ),
                "31",
            );
            return ExitCode::from(2);
        };
        if self.args.workflow.is_some() {
            artifact::report(
                "✗",
                &localizer.text("workspace-onboarding-workflow-owned"),
                "31",
            );
            return ExitCode::from(2);
        }
        let plan = match create::inspect_workspace_member(workspace, name) {
            Ok(plan) => plan.with_application(self.args.application.as_deref()),
            Err(error) => {
                artifact::report("✗", &present(&error, localizer), "31");
                return ExitCode::FAILURE;
            }
        };
        let mut project_type = self.args.project_type.clone();
        if project_type.as_ref().is_some_and(|value| {
            crate::onboarding::config::parse_project_type(value.clone()).is_err()
        }) {
            artifact::report("✗", &localizer.text("new-type-invalid"), "31");
            return ExitCode::from(2);
        }
        if project_type.is_none() {
            if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
                artifact::report("✗", &localizer.text("new-member-type-required"), "31");
                return ExitCode::from(2);
            }
            let result = (|| {
                let mut selector = Selector::start("new-tui-title")?;
                project_type =
                    Some(selector.choose(localizer, "new-type-menu", &PROJECT_TYPE_CHOICES)?);
                selector.finish().map_err(|_| PromptError::Io)
            })();
            if let Err(error) = result {
                artifact::report(
                    "✗",
                    &localizer.text(match error {
                        PromptError::Cancelled => "new-cancelled",
                        PromptError::Io => "new-prompt-error",
                    }),
                    "31",
                );
                return ExitCode::FAILURE;
            }
        }
        let Some(project_type) = project_type
            .and_then(|value| crate::onboarding::config::parse_project_type(value).ok())
        else {
            artifact::report("✗", &localizer.text("new-type-invalid"), "31");
            return ExitCode::from(2);
        };
        match create::create_workspace_member(&plan, project_type, &|| scope.is_cancelled()) {
            Ok(project) => {
                artifact::success(&localizer.format(
                    "new-member-created",
                    &[(
                        "path",
                        LocalizationValue::Text(&path_output::render(
                            project.root(),
                            io::stdout().is_terminal(),
                        )),
                    )],
                ));
                ExitCode::SUCCESS
            }
            Err(error) => {
                artifact::report("✗", &present(&error, localizer), "31");
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

/// Render nested creation errors without decorating their reasons more than once.
fn present(error: &CreationError, localizer: &Localizer) -> String {
    let (key, path) = match error {
        CreationError::Operation(create::CreationFailure::Native(error)) => {
            return error.message.clone();
        }
        CreationError::Operation(create::CreationFailure::Cancelled) => {
            return localizer.text("new-cancelled");
        }
        CreationError::InvalidDestination { path } => ("new-destination-invalid", path),
        CreationError::AlreadyExists { path } => ("new-destination-exists", path),
        CreationError::Io { path, .. } => ("new-io-error", path),
        CreationError::Template(_) => return localizer.text("new-template-error"),
        CreationError::Operation(create::CreationFailure::Vcs(error)) => {
            return eska_presentation::components::present(error, localizer);
        }
        CreationError::Validation(error) => {
            return diagnostics::present_project_error_with_links(
                error,
                localizer,
                io::stderr().is_terminal(),
            );
        }
        CreationError::Workspace(error) => {
            return diagnostics::present_workspace_enrollment_error_with_links(
                error,
                localizer,
                io::stderr().is_terminal(),
            );
        }
        CreationError::WorkspaceValidation(error) => {
            return diagnostics::present_context_error_with_links(
                error,
                localizer,
                io::stderr().is_terminal(),
            );
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
                    (
                        "path",
                        LocalizationValue::Text(&path_output::render(
                            path,
                            io::stderr().is_terminal(),
                        )),
                    ),
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
                .map(|path| path_output::render(path, io::stderr().is_terminal()))
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
        &[(
            "path",
            LocalizationValue::Text(&path_output::render(path, io::stderr().is_terminal())),
        )],
    )
}

/// Execute compatibility creation with the same transaction and component boundaries.
pub fn run(args: &NewArgs, directory: &Path, localizer: &Localizer) -> ExitCode {
    NewHandler { args }.run(directory, localizer)
}
