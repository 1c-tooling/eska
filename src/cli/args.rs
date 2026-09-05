//! Global options, argument parsing and localized top-level help.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use clap::{ArgAction, CommandFactory, FromArgMatches, Parser};

use super::{
    commands::{self, Commands},
    localization::Localizer,
};

/// Command-line interface for eska.
#[derive(Parser, Debug)]
#[command(
    name = "eska",
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Selected UI locale.
    #[arg(long, global = true)]
    pub lang: Option<String>,

    #[arg(long, global = true, default_value = ".")]
    pub(super) project_dir: PathBuf,

    #[command(subcommand)]
    pub(super) command: Option<Commands>,

    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,

    #[arg(short = 'V', long, action = ArgAction::Version)]
    version: Option<bool>,
}

impl Cli {
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
            "{{before-help}}{{name}} {{version}}\n{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{subcommands}}\n{}:\n{{options}}{{after-help}}",
            localizer.text("cli-usage"),
            localizer.text("cli-commands"),
            localizer.text("cli-options"),
        );

        let command = Self::command()
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
            .mut_arg("version", |arg| arg.help(localizer.text("cli-version")));
        commands::localize(command, localizer)
    }
}

/// Extracts only the global `--lang` value needed before clap renders help.
#[must_use]
pub(super) fn bootstrap_lang<T: AsRef<OsStr>>(args: &[T]) -> Option<String> {
    let mut arguments = args.iter().skip(1);
    while let Some(argument) = arguments.next() {
        let argument = argument.as_ref();
        if argument == "--" {
            break;
        }
        if argument == "--lang" {
            return arguments
                .next()
                .and_then(|value| value.as_ref().to_str())
                .map(str::to_owned);
        }
        if let Some(argument) = argument.to_str()
            && let Some(value) = argument.strip_prefix("--lang=")
        {
            return Some(value.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Cli, bootstrap_lang};
    use crate::cli::localization::{Locale, Localizer};

    #[test]
    fn localized_command_registry_contains_only_supported_subcommands() {
        for locale in [Locale::RuRu, Locale::EnUs] {
            let localizer = Localizer::try_new(locale).expect("valid locale");
            let command = Cli::localized_command(&localizer);
            let names: Vec<_> = command
                .get_subcommands()
                .map(clap::Command::get_name)
                .collect();
            assert_eq!(
                names,
                [
                    "clone", "new", "init", "diff", "history", "save", "start", "status"
                ]
            );
            command.debug_assert();
        }
    }

    #[test]
    fn extracts_both_bootstrap_argument_forms() {
        assert_eq!(
            bootstrap_lang(&["eska", "--help", "--lang", "ru"]),
            Some("ru".to_owned())
        );
        assert_eq!(
            bootstrap_lang(&["eska", "--lang=en", "--help"]),
            Some("en".to_owned())
        );
        assert_eq!(bootstrap_lang(&["eska", "--", "--lang", "ru"]), None);
    }
}
