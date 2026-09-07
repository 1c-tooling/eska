//! Shared resolution of command-line and machine-local 1C platform settings.

use crate::{
    config::{GlobalConfigError, RunnerKind, load_global},
    project::build::{RunnerPreference, ToolOptions},
};
use std::path::PathBuf;

/// Merge command-line runner values with the machine-local global config.
pub(super) fn tool_options(
    ibcmd: Option<PathBuf>,
    platform_arch: Option<String>,
    distrobox: Option<String>,
) -> Result<ToolOptions, GlobalConfigError> {
    let (_, config) = load_global()?;
    Ok(
        ToolOptions::new(ibcmd, platform_arch, distrobox).with_machine_defaults(
            runner_preference(config.build.runner),
            config.build.platform_arch,
            config.build.container,
        ),
    )
}

const fn runner_preference(runner: RunnerKind) -> RunnerPreference {
    match runner {
        RunnerKind::Auto => RunnerPreference::Auto,
        RunnerKind::Host => RunnerPreference::Host,
        RunnerKind::Distrobox => RunnerPreference::Distrobox,
    }
}
