//! Shared native command options and help; execution belongs to eska-build.

use clap::Args;

use crate::localization::Localizer;
use std::path::PathBuf;

#[derive(Debug, Default, Args)]
pub struct PlatformArgs {
    #[arg(long)]
    pub ibcmd: Option<PathBuf>,
    #[arg(long)]
    pub platform_arch: Option<String>,
    #[arg(long)]
    pub distrobox: Option<String>,
    #[arg(long, conflicts_with = "select_platform")]
    pub platform_version: Option<String>,
    #[arg(long)]
    pub select_platform: bool,
}

/// Keep platform flag help aligned across creation and update commands.
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .mut_arg("ibcmd", |arg| arg.help(localizer.text("build-ibcmd-help")))
        .mut_arg("platform_arch", |arg| {
            arg.help(localizer.text("build-arch-help"))
        })
        .mut_arg("distrobox", |arg| {
            arg.help(localizer.text("build-distrobox-help"))
        })
        .mut_arg("platform_version", |arg| {
            arg.help(localizer.text("artifact-platform-version-help"))
        })
        .mut_arg("select_platform", |arg| {
            arg.help(localizer.text("artifact-select-platform-help"))
        })
}

impl PlatformArgs {
    /// Detect platform flags before native import is selected.
    #[must_use]
    pub const fn is_set(&self) -> bool {
        self.ibcmd.is_some()
            || self.platform_arch.is_some()
            || self.distrobox.is_some()
            || self.platform_version.is_some()
            || self.select_platform
    }
}
