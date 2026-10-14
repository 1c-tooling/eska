//! ESKA builds cross the companion process boundary; the existing XML execution route stays local.

mod tool;

use std::{
    ffi::{OsStr, OsString},
    path::Path,
    process::ExitCode,
};

use crate::project::{
    SourceFormat,
    discovery::DiscoveryContext,
    selection::{ProjectSelection, SelectedProject},
};

use super::{BuildArgs, BuildPlan, Localizer, OutputFormat, PlatformVersion, workspace_root};

/// Resolve the requested format; XML source options fail without discovering a companion.
pub(super) fn dispatch(
    args: &BuildArgs,
    context: &DiscoveryContext,
    selection: &ProjectSelection<'_>,
    localizer: &Localizer,
) -> Option<ExitCode> {
    if selection.projects().iter().any(|selected| {
        matches!(
            selected.project().configuration().source_format(),
            SourceFormat::Eska { .. }
        )
    }) {
        return Some(run(args, context, selection, localizer));
    }
    args.has_source_options().then(|| {
        args.fail(
            "build-source-options-incompatible",
            Some("selection"),
            None,
            &localizer.text("build-source-options-incompatible"),
            localizer,
            ExitCode::FAILURE,
        )
    })
}

/// UI selection and read-only plan checks precede discovery, with no parent publication transaction.
fn run(
    args: &BuildArgs,
    context: &DiscoveryContext,
    selection: &ProjectSelection<'_>,
    localizer: &Localizer,
) -> ExitCode {
    if args.workspace || selection.is_aggregate() || selection.projects().len() != 1 {
        return args.fail(
            "build-eska-scope-unsupported",
            Some("selection"),
            None,
            &localizer.text("build-eska-scope-unsupported"),
            localizer,
            ExitCode::FAILURE,
        );
    }
    if args.mode.dry_run {
        return args.fail(
            "build-eska-dry-run-unsupported",
            Some("selection"),
            None,
            &localizer.text("build-eska-dry-run-unsupported"),
            localizer,
            ExitCode::FAILURE,
        );
    }
    let Some(selected) = selection.projects().first().copied() else {
        return args.fail(
            "project-selection-invalid",
            Some("selection"),
            None,
            &localizer.text("build-selection-invalid"),
            localizer,
            ExitCode::FAILURE,
        );
    };
    let version = match args
        .tool_options(localizer)
        .and_then(|options| args.platform_override(&options, localizer))
    {
        Ok(version) => version,
        Err(exit) => return exit,
    };
    let base = args.resolved_base_configuration(context);
    if let Err(error) = preflight_plan(args, context, selected, version.clone(), base.as_deref()) {
        return args.fail_project(
            super::plan_error_code(&error),
            Some("plan"),
            selected.name(),
            &super::present_plan_error(&error, localizer),
            localizer,
        );
    }
    let outcome = tool::Tool::discover().and_then(|tool| {
        tool.verify(
            localizer.locale().as_str(),
            selected.project().configuration().project_type(),
        )?;
        tool.execute(&arguments(
            args,
            selected,
            version.as_ref(),
            base.as_deref(),
            localizer,
        ))
    });
    match outcome {
        Ok(exit) => exit,
        Err(error) => args.fail_project(
            error.code(),
            Some("tool-delegation"),
            selected.name(),
            &localizer.text(error.message_key()),
            localizer,
        ),
    }
}

/// Validate the common artifact/platform/context plan without preparing or publishing derived XML.
fn preflight_plan(
    args: &BuildArgs,
    context: &DiscoveryContext,
    selected: SelectedProject<'_>,
    version: Option<PlatformVersion>,
    base: Option<&Path>,
) -> Result<(), super::PlanError> {
    let plan = match (selected.name(), workspace_root(context)) {
        (Some(name), Some(root)) => BuildPlan::for_workspace_member(
            selected.project(),
            root,
            name.as_str(),
            args.output.as_deref(),
            version,
        ),
        _ => BuildPlan::with_platform_version(selected.project(), args.output.as_deref(), version),
    }?;
    plan.with_base_configuration(base.map(Path::to_owned))?;
    Ok(())
}

/// Keep native paths as operating-system strings and preserve the member's inherited scope.
fn arguments(
    args: &BuildArgs,
    selected: SelectedProject<'_>,
    version: Option<&PlatformVersion>,
    base: Option<&Path>,
    localizer: &Localizer,
) -> Vec<OsString> {
    let mut values = vec![
        "--lang".into(),
        localizer.locale().as_str().into(),
        "--project-dir".into(),
        selected.project().root().as_os_str().to_owned(),
        "--format".into(),
        match args.format {
            OutputFormat::Human => "human",
            OutputFormat::Json => "json",
        }
        .into(),
        "build".into(),
    ];
    option(
        &mut values,
        "--project",
        selected.name().map(|name| name.as_str().as_ref()),
    );
    option(
        &mut values,
        "--output",
        args.output.as_deref().map(Path::as_os_str),
    );
    option(
        &mut values,
        "--base-configuration",
        base.map(Path::as_os_str),
    );
    option(
        &mut values,
        "--ibcmd",
        args.ibcmd.as_deref().map(Path::as_os_str),
    );
    option(
        &mut values,
        "--platform-arch",
        args.platform_arch.as_deref().map(AsRef::as_ref),
    );
    option(
        &mut values,
        "--distrobox",
        args.distrobox.as_deref().map(AsRef::as_ref),
    );
    option(
        &mut values,
        "--platform-version",
        version.map(|value| value.as_str().as_ref()),
    );
    option(
        &mut values,
        "--accepted-base-sha256",
        args.source
            .accepted_base_sha256
            .as_deref()
            .map(AsRef::as_ref),
    );
    option(
        &mut values,
        "--generations",
        args.source.generations.as_deref().map(Path::as_os_str),
    );
    if args.recreate_infobase {
        values.push("--recreate-infobase".into());
    }
    if args.source.allow_exclusions {
        values.push("--allow-exclusions".into());
    }
    values
}

/// Absent options remain absent so the companion retains its environment and default policy.
fn option(values: &mut Vec<OsString>, key: &str, value: Option<&OsStr>) {
    if let Some(value) = value {
        values.push(key.into());
        values.push(value.to_owned());
    }
}
