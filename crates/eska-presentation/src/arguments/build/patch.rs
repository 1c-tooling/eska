//! Shared native command options and help; execution belongs to eska-build.

use crate::localization::Localizer;
use clap::{Args, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct PatchArgs {
    #[arg(long)]
    pub base: Option<String>,
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub ibcmd: Option<PathBuf>,
    #[arg(long)]
    pub platform_arch: Option<String>,
    #[arg(long)]
    pub distrobox: Option<String>,
    #[arg(long)]
    pub platform_version: Option<String>,
    #[arg(long, value_enum, default_value_t = Format::Human)]
    pub format: Format,
    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Format {
    Human,
    Json,
}

/// Localize every new help field without translating argument names or machine values.
pub fn localize(mut command: clap::Command, localizer: &Localizer) -> clap::Command {
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
