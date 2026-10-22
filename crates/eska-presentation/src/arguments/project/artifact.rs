//! Shared project command options and help; metadata and transactions stay with their owner.

use crate::localization::Localizer;
use clap::Args;
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

/// Localized public flags stay identical when parsing moves to the component.
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
    /// Detect platform flags before artifact input or initialization is selected.
    #[must_use]
    pub const fn is_set(&self) -> bool {
        self.ibcmd.is_some()
            || self.platform_arch.is_some()
            || self.distrobox.is_some()
            || self.platform_version.is_some()
            || self.select_platform
    }
}
