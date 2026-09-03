use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, CommandFactory, FromArgMatches, Parser};

use crate::localization::Localizer;

mod project_errors;

/// Command-line interface for eska.
#[derive(Parser, Debug)]
#[command(name = "eska", disable_help_flag = true, disable_version_flag = true)]
pub struct Cli {
    /// Selected UI locale.
    #[arg(long, global = true)]
    pub lang: Option<String>,

    #[arg(long, default_value = ".")]
    project_dir: PathBuf,

    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,

    #[arg(short = 'V', long, action = ArgAction::Version)]
    version: Option<bool>,
}

impl Cli {
    /// Validates the selected project and presents any failure in the UI locale.
    pub fn run(&self, localizer: &Localizer) -> ExitCode {
        match crate::discovery::discover(&self.project_dir) {
            Ok(_) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{}", project_errors::present(&error, localizer));
                ExitCode::FAILURE
            }
        }
    }

    /// Parses command-line arguments using localized help metadata.
    pub fn parse_localized<I, T>(args: I, localizer: &Localizer) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let matches = Self::localized_command(localizer).get_matches_from(args);
        Self::from_arg_matches(&matches).unwrap_or_else(|error| error.exit())
    }

    fn localized_command(localizer: &Localizer) -> clap::Command {
        let help_template = format!(
            "{{before-help}}{{name}} {{version}}\n{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{options}}{{after-help}}",
            localizer.text("cli-usage"),
            localizer.text("cli-options"),
        );

        Self::command()
            .about(localizer.text("app-about"))
            .version(env!("CARGO_PKG_VERSION"))
            .override_usage(localizer.text("cli-usage-syntax"))
            .help_template(help_template)
            .mut_arg("lang", |arg| {
                arg.help(localizer.text("cli-lang-help"))
                    .value_name(localizer.text("cli-lang-value"))
            })
            .mut_arg("project_dir", |arg| {
                arg.help(localizer.text("cli-project-dir-help"))
                    .value_name(localizer.text("cli-project-dir-value"))
                    .hide_default_value(true)
            })
            .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
            .mut_arg("version", |arg| arg.help(localizer.text("cli-version")))
    }
}
