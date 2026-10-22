//! Shared VCS argument metadata; repository execution stays in its process owner.

use crate::localization::Localizer;
use clap::{ArgAction, Args};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct CloneArgs {
    pub repository: OsString,
    pub directory: Option<PathBuf>,
    #[arg(long, default_value = "origin")]
    pub remote: String,
    #[arg(short, long, action = ArgAction::Help)]
    pub help: Option<bool>,
}

/// Attach localized arguments without changing the command contract.
#[must_use]
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("clone-about"))
        .override_usage(localizer.text("clone-usage"))
        .help_template(format!(
            "{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{positionals}}\n\n{}:\n{{options}}",
            localizer.text("cli-usage"),
            localizer.text("cli-arguments"),
            localizer.text("cli-options")
        ))
        .mut_arg("repository", |arg| {
            arg.help(localizer.text("clone-repository-help"))
                .value_name(localizer.text("clone-repository-value"))
        })
        .mut_arg("directory", |arg| {
            arg.help(localizer.text("clone-directory-help"))
                .value_name(localizer.text("cli-project-dir-value"))
        })
        .mut_arg("remote", |arg| {
            arg.help(localizer.text("clone-remote-help"))
                .value_name(localizer.text("clone-remote-value"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
