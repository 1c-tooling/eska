//! Existing directories remain user-owned while source and workspace policy are preflighted read-only.

use std::{
    fs,
    path::{Component, Path},
};

use super::super::super::artifact::Failure;
use super::{super::InitArgs, failure};
use crate::{
    cli::{
        diagnostics,
        interactive::{Selector, WORKFLOW_CHOICES},
        localization::Localizer,
    },
    config::WorkspaceConfig,
    project::{
        ProjectName, Workspace,
        onboarding::{self, WorkspaceMemberEnrollment},
    },
    project_core::{
        platform_settings::PlatformVersion,
        source_format::{
            AcceptedRef, ProjectInitializationPlan, SourcePath, prepare_project_initialization,
        },
    },
    tool_cli::OutputFormat,
    vcs::{git, workflow::WorkflowPreset},
};

pub(super) struct Plan {
    pub(super) source: SourcePath,
    pub(super) name: ProjectName,
    pub(super) core: ProjectInitializationPlan,
    pub(super) workflow: Option<WorkflowPreset>,
    pub(super) platform: Option<PlatformVersion>,
    pub(super) approved: Option<AcceptedRef>,
    pub(super) enrollment: Option<WorkspaceMemberEnrollment>,
    pub(super) git_present: bool,
    workspace: Option<Workspace>,
    workspace_manifest: Option<Vec<u8>>,
}

impl Plan {
    /// Root policy bytes are bound before child publication and every parent-owned mutation.
    pub(super) fn verify_workspace(&self, localizer: &Localizer) -> Result<(), Failure> {
        if let (Some(workspace), Some(expected)) = (&self.workspace, &self.workspace_manifest)
            && fs::read(workspace.root().join(crate::config::FILE_NAME))
                .ok()
                .as_ref()
                != Some(expected)
        {
            return Err(failure("init-eska-policy-changed", localizer));
        }
        Ok(())
    }
}

/// Type/version/source integrity and root-owned policy are checked before any companion writes.
pub(super) fn prepare(
    args: &InitArgs,
    base: &Path,
    localizer: &Localizer,
) -> Result<Plan, Failure> {
    if args.platform.ibcmd.is_some()
        || args.platform.platform_arch.is_some()
        || args.platform.distrobox.is_some()
    {
        return Err(failure("init-eska-runner-options", localizer));
    }
    let root = base
        .join(&args.path)
        .canonicalize()
        .map_err(|_| failure("init-eska-root-invalid", localizer))?;
    let source = source(
        args.source.as_deref().unwrap_or_else(|| Path::new("src")),
        localizer,
    )?;
    let raw_name = args
        .name
        .as_deref()
        .or_else(|| root.file_name().and_then(|name| name.to_str()))
        .ok_or_else(|| failure("init-eska-name-invalid", localizer))?;
    let name = ProjectName::parse(raw_name.into())
        .map_err(|_| failure("init-eska-name-invalid", localizer))?;
    let workspace = onboarding::find_workspace(&root).map_err(|error| {
        Failure::new(
            "discovery",
            diagnostics::present_context_error(&error, localizer),
        )
    })?;
    let workflow = workflow(args, workspace.as_ref(), localizer)?;
    let enrollment = workspace
        .as_ref()
        .map(|workspace| {
            let relative = root
                .strip_prefix(workspace.root())
                .map_err(|_| failure("init-eska-root-invalid", localizer))?;
            onboarding::prepare(workspace, root.clone(), relative.to_owned(), &name).map_err(
                |error| {
                    Failure::new(
                        "init-eska-enrollment-invalid",
                        diagnostics::present_workspace_enrollment_error(&error, localizer),
                    )
                },
            )
        })
        .transpose()?;
    let workspace_manifest = workspace
        .as_ref()
        .map(|workspace| {
            fs::read(workspace.root().join(crate::config::FILE_NAME))
                .map_err(|_| failure("init-eska-policy-changed", localizer))
        })
        .transpose()?;
    let platform = platform(args, workspace.as_ref(), localizer)?;
    let approved = approved(args, localizer)?;
    let core = prepare_project_initialization(
        &root,
        source.clone(),
        name.clone(),
        platform.as_ref(),
        approved.as_ref(),
    )
    .map_err(|error| super::source_failure(&error, localizer))?;
    if let Some(input) = &workspace_manifest {
        let text = std::str::from_utf8(input)
            .map_err(|_| failure("init-eska-policy-changed", localizer))?;
        let config = WorkspaceConfig::from_toml(text)
            .map_err(|_| failure("init-eska-policy-changed", localizer))?;
        if let Some(scheme) = config.version_scheme() {
            core.version()
                .validate_scheme(scheme)
                .map_err(|_| failure("init-eska-version-scheme", localizer))?;
        }
    }
    let git_present = if enrollment.is_none() && !args.no_vcs {
        git::exists(&root).map_err(|_| failure("init-eska-git-invalid", localizer))?
    } else {
        false
    };
    let plan = Plan {
        source,
        name,
        core,
        workflow,
        platform,
        approved,
        enrollment,
        git_present,
        workspace,
        workspace_manifest,
    };
    plan.verify_workspace(localizer)?;
    Ok(plan)
}

/// Relative OS paths must retain normal portable components before any canonicalization.
fn source(path: &Path, localizer: &Localizer) -> Result<SourcePath, Failure> {
    let parts = path
        .components()
        .map(|part| match part {
            Component::Normal(name) => name
                .to_str()
                .ok_or_else(|| failure("init-eska-source-invalid", localizer)),
            _ => Err(failure("init-eska-source-invalid", localizer)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    SourcePath::parse(&parts.join("/")).map_err(|_| failure("init-eska-source-invalid", localizer))
}

/// The caller supplies existing support authority; root TOML and child output cannot approve it.
fn approved(args: &InitArgs, localizer: &Localizer) -> Result<Option<AcceptedRef>, Failure> {
    let environment = std::env::var("ESKA_ACCEPTED_BASE_SHA256").ok();
    args.accepted_base_sha256
        .as_deref()
        .or(environment.as_deref())
        .map(|value| {
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(failure("init-eska-pin-invalid", localizer));
            }
            Ok(AcceptedRef {
                sha256: value.to_ascii_lowercase(),
            })
        })
        .transpose()
}

/// ESKA attachment does not require a platform unless its selection was explicitly requested.
fn platform(
    args: &InitArgs,
    workspace: Option<&Workspace>,
    localizer: &Localizer,
) -> Result<Option<PlatformVersion>, Failure> {
    if args.platform.select_platform {
        let inherited =
            workspace.and_then(|workspace| workspace.build_settings().platform_version());
        return args
            .platform
            .resolve(
                inherited,
                matches!(args.format, OutputFormat::Human),
                localizer,
            )
            .map(|tool| Some(tool.version().clone()));
    }
    args.platform
        .platform_version
        .as_deref()
        .map(PlatformVersion::parse)
        .transpose()
        .map_err(|_| {
            Failure::new(
                "platform-version-invalid",
                localizer.text("artifact-version-invalid"),
            )
        })
}

/// Members inherit workflow; standalone callers retain the normal explicit/nonterminal policy choice.
fn workflow(
    args: &InitArgs,
    workspace: Option<&Workspace>,
    localizer: &Localizer,
) -> Result<Option<WorkflowPreset>, Failure> {
    if workspace.is_some() {
        return if args.workflow.is_some() {
            Err(Failure::new(
                "workspace-workflow",
                localizer.text("workspace-onboarding-workflow-owned"),
            ))
        } else {
            Ok(None)
        };
    }
    let value = if let Some(value) = &args.workflow {
        value.clone()
    } else {
        if matches!(args.format, OutputFormat::Json) || !super::super::super::artifact::terminal() {
            return Err(failure("init-eska-workflow-required", localizer));
        }
        let mut selector = Selector::start("init-tui-title")
            .map_err(|error| super::super::super::artifact::prompt_error(error, localizer))?;
        let selected = selector
            .choose(localizer, "new-workflow-menu", &WORKFLOW_CHOICES)
            .map_err(|error| super::super::super::artifact::prompt_error(error, localizer))?;
        selector
            .finish()
            .map_err(|_| failure("new-prompt-error", localizer))?;
        selected
    };
    WorkflowPreset::from_name(&value)
        .map(Some)
        .ok_or_else(|| Failure::new("workflow-invalid", localizer.text("new-workflow-invalid")))
}
