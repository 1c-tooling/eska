//! Ordinary onboarding policy is selected before companions may claim or publish a destination.

use super::{
    super::{NewArgs, artifact},
    failure,
};
use crate::{
    cli::{
        diagnostics,
        interactive::{PROJECT_TYPE_CHOICES, Selector, WORKFLOW_CHOICES},
        localization::Localizer,
    },
    config::WorkspaceConfig,
    project::{
        ProjectName, ProjectType, Workspace,
        create::{self, WorkspaceCreationPlan},
        onboarding,
    },
    project_core::{
        creation::NewProjectOptions,
        native_version::{ProjectVersion, VersionScheme},
        platform_settings::PlatformVersion,
    },
    tool_cli::OutputFormat,
    vcs::workflow::WorkflowPreset,
};
use artifact::Failure;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) struct Plan {
    pub(super) destination: PathBuf,
    pub(super) name: ProjectName,
    pub(super) member: Option<WorkspaceCreationPlan>,
    pub(super) workspace: Option<Workspace>,
    pub(super) workflow: Option<WorkflowPreset>,
    pub(super) project_type: Option<ProjectType>,
    pub(super) version: Option<ProjectVersion>,
    pub(super) metadata_name: Option<String>,
    pub(super) platform: Option<PlatformVersion>,
    pub(super) scheme: Option<VersionScheme>,
    workspace_manifest: Option<String>,
}

impl Plan {
    /// Root policy must still be the same input when onboarding is finalized.
    pub(super) fn verify_workspace(&self, localizer: &Localizer) -> Result<(), Failure> {
        if let (Some(workspace), Some(expected)) = (&self.workspace, &self.workspace_manifest)
            && fs::read(workspace.root().join(crate::config::FILE_NAME))
                .ok()
                .as_deref()
                != Some(expected.as_bytes())
        {
            return Err(failure("new-policy-changed", localizer));
        }
        Ok(())
    }
}

/// A member inherits root policy while a standalone project selects its own workflow.
pub(super) fn prepare(args: &NewArgs, base: &Path, localizer: &Localizer) -> Result<Plan, Failure> {
    let workspace = onboarding::find_workspace(base).map_err(|error| {
        Failure::new(
            "discovery",
            diagnostics::present_context_error_with_links(&error, localizer, false),
        )
    })?;
    validate_input_options(args, localizer)?;
    let workspace_manifest = workspace
        .as_ref()
        .map(|workspace| {
            fs::read_to_string(workspace.root().join(crate::config::FILE_NAME))
                .map_err(|_| failure("new-policy-changed", localizer))
        })
        .transpose()?;
    let scheme = workspace_manifest
        .as_deref()
        .map(WorkspaceConfig::from_toml)
        .transpose()
        .map_err(|_| failure("new-policy-changed", localizer))?
        .and_then(|config| config.version_scheme());
    let (destination, name, member) = destination(args, base, workspace.as_ref(), localizer)?;
    let workflow = if member.is_some() {
        None
    } else {
        let value = choose(
            args.workflow.as_deref(),
            "new-workflow-menu",
            &WORKFLOW_CHOICES,
            args,
            "init-options-required",
            localizer,
        )?;
        Some(WorkflowPreset::from_name(&value).ok_or_else(|| {
            Failure::new("workflow-invalid", localizer.text("new-workflow-invalid"))
        })?)
    };
    let project_type = if args.from.is_some() {
        None
    } else {
        let value = choose(
            args.project_type.as_deref(),
            "new-type-menu",
            &PROJECT_TYPE_CHOICES,
            args,
            if member.is_some() {
                "new-member-type-required"
            } else {
                "new-options-required"
            },
            localizer,
        )?;
        Some(crate::config::parse_project_type(value).map_err(|_| {
            Failure::new("project-type-invalid", localizer.text("new-type-invalid"))
        })?)
    };
    let version = initial_version(args, scheme, localizer)?;
    let metadata_name = match (project_type, &version) {
        (Some(kind), Some(version)) => {
            let mut options = NewProjectOptions::new(kind, version.clone());
            if let Some(value) = &args.metadata_name {
                options = options
                    .with_metadata_name(value.clone())
                    .map_err(|_| failure("new-metadata-name-invalid", localizer))?;
            }
            Some(options.metadata_name(&name))
        }
        _ => None,
    };
    let platform = args
        .platform
        .platform_version
        .as_deref()
        .map(PlatformVersion::parse)
        .transpose()
        .map_err(|_| {
            Failure::new(
                "platform-version-invalid",
                localizer.text("artifact-version-invalid"),
            )
        })?;
    Ok(Plan {
        destination,
        name,
        member,
        workspace,
        workflow,
        project_type,
        version,
        metadata_name,
        platform,
        scheme,
        workspace_manifest,
    })
}

/// JSON and nonterminal calls require explicit policy; interactive selection uses the existing UI.
fn choose(
    value: Option<&str>,
    menu: &'static str,
    choices: &[(&str, &str)],
    args: &NewArgs,
    missing: &'static str,
    localizer: &Localizer,
) -> Result<String, Failure> {
    if let Some(value) = value {
        return Ok(value.to_owned());
    }
    if matches!(args.format, OutputFormat::Json) || !artifact::terminal() {
        return Err(Failure::new("options-required", localizer.text(missing)));
    }
    let mut selector = Selector::start("new-tui-title")
        .map_err(|error| artifact::prompt_error(error, localizer))?;
    let value = selector
        .choose(localizer, menu, choices)
        .map_err(|error| artifact::prompt_error(error, localizer))?;
    selector
        .finish()
        .map_err(|_| Failure::new("prompt", localizer.text("new-prompt-error")))?;
    Ok(value)
}

/// Derive only an absent destination; the project child retains its exclusive root claim.
fn destination(
    args: &NewArgs,
    base: &Path,
    workspace: Option<&Workspace>,
    localizer: &Localizer,
) -> Result<(PathBuf, ProjectName, Option<WorkspaceCreationPlan>), Failure> {
    let creation_error = |error| Failure::new("creation", super::super::present(&error, localizer));
    let (destination, name, member) = if let Some(workspace) = workspace {
        if args.workflow.is_some() {
            return Err(Failure::new(
                "workspace-workflow",
                localizer.text("workspace-onboarding-workflow-owned"),
            ));
        }
        let name = super::super::workspace_project_name(&args.path)
            .ok_or_else(|| failure("new-eska-name-invalid", localizer))?;
        let member =
            create::inspect_workspace_member(workspace, name.clone()).map_err(creation_error)?;
        (member.destination().to_owned(), name, Some(member))
    } else {
        let destination =
            create::resolve_destination(&base.join(&args.path)).map_err(creation_error)?;
        let name = destination
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| ProjectName::parse(name.to_owned()).ok())
            .ok_or_else(|| failure("new-eska-name-invalid", localizer))?;
        (destination, name, None)
    };
    if destination.to_str().is_none() {
        return Err(failure("new-eska-name-invalid", localizer));
    }

    Ok((destination, name, member))
}

/// Choose only an empty project's initial version; imported native values stay unchanged.
fn initial_version(
    args: &NewArgs,
    scheme: Option<VersionScheme>,
    localizer: &Localizer,
) -> Result<Option<ProjectVersion>, Failure> {
    let version = if args.from.is_some() {
        None
    } else {
        let default = if scheme == Some(VersionScheme::FourPart) {
            "0.1.0.0"
        } else {
            "0.1.0"
        };
        let version = ProjectVersion::parse(args.initial_version.as_deref().unwrap_or(default))
            .map_err(|_| failure("new-initial-version-invalid", localizer))?;
        if let Some(scheme) = scheme {
            version
                .validate_scheme(scheme)
                .map_err(|_| failure("new-version-scheme-mismatch", localizer))?;
        }
        Some(version)
    };
    Ok(version)
}

/// Source-only empty creation and native import expose different, explicit option contracts.
fn validate_input_options(args: &NewArgs, localizer: &Localizer) -> Result<(), Failure> {
    if args.from.is_some() && (args.metadata_name.is_some() || args.initial_version.is_some()) {
        return Err(failure("new-from-source-options-incompatible", localizer));
    }
    if args.from.is_none()
        && (args.platform.ibcmd.is_some()
            || args.platform.platform_arch.is_some()
            || args.platform.distrobox.is_some())
    {
        return Err(failure("new-empty-runner-options-incompatible", localizer));
    }
    Ok(())
}
