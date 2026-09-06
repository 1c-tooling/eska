//! Separate patch CLI with localized diagnostics and a versioned JSON plan/result.

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        build::{Ibcmd, PlatformVersion},
        discovery, patch,
    },
};
use clap::{Args, ValueEnum};
use serde_json::json;
use std::{
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

#[derive(Debug, Args)]
pub(in crate::cli) struct PatchArgs {
    #[arg(long)]
    base: Option<String>,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    ibcmd: Option<PathBuf>,
    #[arg(long)]
    platform_arch: Option<String>,
    #[arg(long)]
    distrobox: Option<String>,
    #[arg(long)]
    platform_version: Option<String>,
    #[arg(long, value_enum, default_value_t = Format::Human)]
    format: Format,
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Format {
    Human,
    Json,
}

impl PatchArgs {
    /// Plan the complete committed delta before selecting or invoking platform tools.
    pub(super) fn run(&self, directory: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover(directory) {
            Ok(project) => project,
            Err(e) => {
                return self.fail(
                    "project",
                    &diagnostics::present_project_error(&e, localizer),
                    localizer,
                );
            }
        };
        let mut plan = match patch::plan(&project, self.base.as_deref()) {
            Ok(plan) => plan,
            Err(e) => return self.fail(e.code, &e.detail, localizer),
        };
        if let Some(version) = &self.platform_version {
            match PlatformVersion::parse(version) {
                Ok(version) => plan.platform_version = Some(version),
                Err(_) => return self.fail("platform", version, localizer),
            }
        }
        let output = self.output.as_ref().map_or_else(
            || {
                project
                    .root()
                    .join(
                        project
                            .configuration()
                            .build_settings()
                            .artifacts_directory(),
                    )
                    .join(format!("{}.cfe", plan.name))
            },
            |path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    project.root().join(path)
                }
            },
        );
        if output.to_str().is_none()
            || output.components().any(|c| c == Component::ParentDir)
            || output.starts_with(project.source())
            || output.extension().is_none_or(|e| e != "cfe")
        {
            return self.fail("output", &output.to_string_lossy(), localizer);
        }
        let state = if plan.modules.is_empty() {
            "empty"
        } else if self.dry_run {
            "planned"
        } else {
            "built"
        };
        if matches!(self.format, Format::Human) {
            println!(
                "{}",
                localizer.format(
                    "patch-preview",
                    &[
                        ("base", LocalizationValue::Text(&plan.merge_base)),
                        ("head", LocalizationValue::Text(&plan.head))
                    ]
                )
            );
            for module in &plan.modules {
                println!("  {}: {}", module.name, module.methods.join(", "));
            }
        }
        if !self.dry_run
            && !plan.modules.is_empty()
            && let Err(code) = self.build(&plan, &output, localizer)
        {
            return code;
        }
        if matches!(self.format, Format::Json) {
            println!(
                "{}",
                json!({"schema_version": 1, "status": state, "plan": plan,
                "output": if state == "empty" { None } else { output.to_str() },
                "validation": if state == "built" { Some("metadata_bsl_applicability") } else { None },
                "runtime_verified": false, "requires_safe_mode_disabled": true})
            );
        } else {
            println!(
                "{}",
                localizer.format(
                    &format!("patch-{state}"),
                    &[("output", LocalizationValue::Text(&output.to_string_lossy()))]
                )
            );
        }
        ExitCode::SUCCESS
    }

    /// Resolve machine settings only for an accepted, nonempty execution plan.
    fn build(
        &self,
        plan: &patch::PatchPlan,
        output: &Path,
        localizer: &Localizer,
    ) -> Result<(), ExitCode> {
        let version = plan
            .platform_version
            .as_ref()
            .ok_or_else(|| self.fail("platform", "", localizer))?;
        let options = super::platform::tool_options(
            self.ibcmd.clone(),
            self.platform_arch.clone(),
            self.distrobox.clone(),
        )
        .map_err(|e| {
            self.fail(
                "tool",
                &super::config::present_error(&e, localizer),
                localizer,
            )
        })?;
        let tool = Ibcmd::discover(version, &options).map_err(|e| {
            self.fail(
                "tool",
                &super::build::present_tool_error(&e, localizer),
                localizer,
            )
        })?;
        patch::execute(plan, &tool, output).map_err(|e| self.fail(e.code, &e.detail, localizer))
    }

    /// Keep JSON error codes independent of locale while presenting human details separately.
    fn fail(&self, code: &str, detail: &str, localizer: &Localizer) -> ExitCode {
        if matches!(self.format, Format::Json) {
            println!(
                "{}",
                json!({"schema_version": 1, "status": "error", "error": {"code": code}})
            );
        }
        eprintln!(
            "{}: {}",
            localizer.text(&format!("patch-error-{code}")),
            detail
        );
        ExitCode::FAILURE
    }
}

/// Localize every new help field without translating argument names or machine values.
pub(super) fn localize(mut command: clap::Command, localizer: &Localizer) -> clap::Command {
    command = command.about(localizer.text("patch-about"));
    for name in [
        "base",
        "output",
        "dry_run",
        "ibcmd",
        "platform_arch",
        "distrobox",
        "platform_version",
        "format",
    ] {
        command = command.mut_arg(name, |arg| {
            arg.help(localizer.text(&format!("patch-help-{}", name.replace('_', "-"))))
        });
    }
    command.mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
