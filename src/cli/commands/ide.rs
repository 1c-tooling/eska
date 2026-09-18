//! CLI entry point for the versioned read-only IDE protocol.

use crate::cli::localization::Localizer;
use clap::Args;

#[derive(Debug, Args)]
pub(in crate::cli) struct IdeArgs {
    #[arg(long, required = true)]
    stdio: bool,
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl IdeArgs {
    /// Standard streams belong exclusively to the protocol after argument parsing.
    pub(super) fn run(&self) -> std::process::ExitCode {
        if self.stdio {
            crate::cli::ide::run()
        } else {
            std::process::ExitCode::from(2)
        }
    }
}

/// Localize CLI help only; protocol values are deliberately locale independent.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("ide-about"))
        .help_template(format!(
            "{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{options}}",
            localizer.text("cli-usage"),
            localizer.text("cli-options")
        ))
        .mut_arg("stdio", |arg| arg.help(localizer.text("ide-stdio-help")))
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
