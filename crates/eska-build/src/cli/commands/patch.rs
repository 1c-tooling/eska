//! Separate patch CLI with localized diagnostics and a versioned JSON plan/result.

use eska_presentation::arguments::build::patch::Format;
pub use eska_presentation::arguments::build::patch::{PatchArgs, localize};

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
        platform,
    },
    project::{
        build::{Ibcmd, PlatformVersion},
        discovery,
    },
};
use serde_json::json;
use std::{
    path::{Component, Path},
    process::ExitCode,
};

mod client;

struct PatchHandler<'a> {
    options: &'a PatchArgs,
}

impl PatchHandler<'_> {
    /// Plan the complete committed delta before selecting or invoking platform tools.
    pub fn run(&self, directory: &Path, localizer: &Localizer, vcs: Option<&Path>) -> ExitCode {
        let cancellation = match super::artifact::cancellation_scope(localizer) {
            Ok(scope) => scope,
            Err(error) => return self.fail("component", &error.message, localizer),
        };
        self.run_interruptible(directory, localizer, vcs, cancellation.as_ref())
    }

    /// Hold one sticky cancellation scope from the first plan through native publication.
    fn run_interruptible(
        &self,
        directory: &Path,
        localizer: &Localizer,
        vcs: Option<&Path>,
        cancellation: Option<&eska_tool_runtime::Operation>,
    ) -> ExitCode {
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
        let client =
            match client::Client::new(project.root(), self.options.base.as_deref(), vcs, localizer)
            {
                Ok(client) => client,
                Err(error) => return self.fail(&error.code, &error.detail, localizer),
            };
        let mut plan = match client.plan(localizer) {
            Ok(plan) => plan,
            Err(error) => return self.fail(&error.code, &error.detail, localizer),
        };
        if let Some(version) = &self.options.platform_version {
            match PlatformVersion::parse(version) {
                Ok(version) => plan.platform_version = Some(version),
                Err(_) => return self.fail("platform", version, localizer),
            }
        }
        let output = self.options.output.as_ref().map_or_else(
            || {
                project
                    .root()
                    .join(
                        project
                            .configuration()
                            .build_settings()
                            .artifacts_directory(),
                    )
                    .join(format!("{}.cfe", plan.description.name))
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
        let state = if plan.description.modules.is_empty() {
            "empty"
        } else if self.options.dry_run {
            "planned"
        } else {
            "built"
        };
        if cancellation.is_some_and(eska_tool_runtime::Operation::is_cancelled) {
            return self.fail("tool", &localizer.text("component-cancelled"), localizer);
        }
        self.preview(&plan.description, localizer);
        if !self.options.dry_run
            && !plan.description.modules.is_empty()
            && let Err(code) = self.build(&plan, &client, &output, cancellation, localizer)
        {
            return code;
        }
        if matches!(self.options.format, Format::Json) {
            println!(
                "{}",
                json!({"schema_version": 1, "status": state, "plan": plan.description,
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

    /// Render the same human preview independently from native execution.
    fn preview(&self, plan: &eska_project_core::patch::PatchDescription, localizer: &Localizer) {
        if matches!(self.options.format, Format::Human) {
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
    }

    /// Resolve machine settings only for an accepted, nonempty execution plan.
    fn build(
        &self,
        plan: &client::Plan,
        client: &client::Client,
        output: &Path,
        cancellation: Option<&eska_tool_runtime::Operation>,
        localizer: &Localizer,
    ) -> Result<(), ExitCode> {
        let version = plan
            .platform_version
            .as_ref()
            .ok_or_else(|| self.fail("platform", "", localizer))?;
        let options = platform::tool_options(
            self.options.ibcmd.clone(),
            self.options.platform_arch.clone(),
            self.options.distrobox.clone(),
        )
        .map_err(|e| {
            self.fail(
                "tool",
                &diagnostics::present_global_config_error(&e, localizer),
                localizer,
            )
        })?;
        let options = super::artifact::scoped_options(options, cancellation);
        let tool = Ibcmd::discover(version, &options).map_err(|e| {
            self.fail(
                "tool",
                &diagnostics::present_tool_error(&e, localizer),
                localizer,
            )
        })?;
        let tool = super::artifact::attach_cancellation(tool, cancellation);
        eska_build_runtime::patch::execute(&plan.description, &tool, output, |workspace| {
            if tool.was_interrupted() {
                return Err(eska_project_core::patch::PatchError::new(
                    "tool",
                    localizer.text("component-cancelled"),
                ));
            }
            client.prepare(&plan.fingerprint, workspace, localizer)
        })
        .map_err(|error| self.fail(&error.code, &error.detail, localizer))
    }

    /// Keep JSON error codes independent of locale while presenting human details separately.
    fn fail(&self, code: &str, detail: &str, localizer: &Localizer) -> ExitCode {
        if matches!(self.options.format, Format::Json) {
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

/// Execute parsed patch options using the native and VCS owners.
pub fn run(
    args: &PatchArgs,
    directory: &Path,
    localizer: &Localizer,
    vcs: Option<&Path>,
) -> ExitCode {
    PatchHandler { options: args }.run(directory, localizer, vcs)
}
