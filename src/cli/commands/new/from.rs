//! Creation from a validated artifact, sharing normal standalone and member publication.

use std::{path::Path, process::ExitCode};

use crate::{
    cli::{
        diagnostics,
        interactive::{Selector, WORKFLOW_CHOICES},
        localization::{LocalizationValue, Localizer},
    },
    config::ProjectConfig,
    project::{artifact::PreparedArtifact, build::BuildSettings, create, onboarding},
    vcs::workflow::WorkflowPreset,
};

use super::{
    super::artifact::{self, Failure},
    NewArgs,
};

/// Report one creation result through the existing localized CLI contract.
pub(super) fn run(args: &NewArgs, base: &Path, localizer: &Localizer) -> ExitCode {
    match execute(args, base, localizer) {
        Ok(project) => {
            println!(
                "{}",
                localizer.format(
                    "artifact-created",
                    &[(
                        "path",
                        LocalizationValue::Text(&project.root().to_string_lossy())
                    )]
                )
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", error.message);
            if matches!(
                error.code,
                "platform-required"
                    | "platform-version-invalid"
                    | "workflow-required"
                    | "workflow-invalid"
                    | "project-name"
                    | "workspace-workflow"
            ) {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

/// Select policy and platform before unpacking, then publish one complete project.
fn execute(
    args: &NewArgs,
    base: &Path,
    localizer: &Localizer,
) -> Result<crate::project::Project, Failure> {
    let workspace = onboarding::find_workspace(base).map_err(|error| {
        Failure::new(
            "discovery",
            diagnostics::present_context_error(&error, localizer),
        )
    })?;
    let creation_error = |error| Failure::new("creation", super::present(&error, localizer));
    let (destination, member) = if let Some(workspace) = &workspace {
        if args.workflow.is_some() {
            return Err(Failure::new(
                "workspace-workflow",
                localizer.text("workspace-onboarding-workflow-owned"),
            ));
        }
        let name = super::workspace_project_name(&args.path).ok_or_else(|| {
            Failure::new(
                "project-name",
                localizer.text("artifact-member-name-invalid"),
            )
        })?;
        let plan = create::inspect_workspace_member(workspace, name).map_err(creation_error)?;
        (plan.destination().to_owned(), Some(plan))
    } else {
        (
            create::resolve_destination(&base.join(&args.path)).map_err(creation_error)?,
            None,
        )
    };
    let workflow = if workspace.is_some() {
        None
    } else {
        Some(workflow(args, localizer)?)
    };
    let inherited = workspace
        .as_ref()
        .and_then(|workspace| workspace.build_settings().platform_version());
    let tool = args.platform.resolve(inherited, true, localizer)?;
    let input = args
        .from
        .as_ref()
        .ok_or_else(|| Failure::new("input-required", localizer.text("artifact-from-required")))?;
    let prepared = PreparedArtifact::unpack(&base.join(input), &destination, &tool, |_, line| {
        eprintln!("{}", String::from_utf8_lossy(line));
    })
    .map_err(|error| artifact::present(&error, localizer))?;
    let mut config = ProjectConfig::new(prepared.identity().project_type);
    if let Some(workflow) = workflow {
        config = config.with_workflow(workflow);
    }
    if workspace.is_none()
        || args.platform.platform_version.is_some()
        || args.platform.select_platform
        || inherited.is_none()
    {
        let settings =
            BuildSettings::new(tool.version().as_str(), "build".into()).map_err(|_| {
                Failure::new(
                    "platform-version-invalid",
                    localizer.text("artifact-version-invalid"),
                )
            })?;
        config = config.with_build_settings(settings);
    }
    member.map_or_else(
        || {
            create::create_imported(&destination, &config, !args.no_vcs, &prepared)
                .map_err(creation_error)
        },
        |plan| create::create_imported_member(&plan, &config, &prepared).map_err(creation_error),
    )
}

/// Ask only for the workflow that cannot be derived from a native artifact.
fn workflow(args: &NewArgs, localizer: &Localizer) -> Result<WorkflowPreset, Failure> {
    let value = if let Some(value) = &args.workflow {
        value.clone()
    } else {
        if !artifact::terminal() {
            return Err(Failure::new(
                "workflow-required",
                localizer.text("init-options-required"),
            ));
        }
        let mut selector = Selector::start("new-tui-title")
            .map_err(|error| artifact::prompt_error(error, localizer))?;
        let value = selector
            .choose(localizer, "new-workflow-menu", &WORKFLOW_CHOICES)
            .map_err(|error| artifact::prompt_error(error, localizer))?;
        selector
            .finish()
            .map_err(|_| Failure::new("prompt", localizer.text("new-prompt-error")))?;
        value
    };
    WorkflowPreset::from_name(&value)
        .ok_or_else(|| Failure::new("workflow-invalid", localizer.text("new-workflow-invalid")))
}
