pub mod commands;

use crate::error::Result;
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand};

const HELP_TEMPLATE: &str = "\
{before-help}{name} {version}
{about-with-newline}
ИСПОЛЬЗОВАНИЕ:
    {usage}

КОМАНДЫ:
{subcommands}

ОПЦИИ:
{options}
{after-help}
";

#[derive(Args, Debug, PartialEq)]
#[command(disable_help_flag = true)]
#[command(disable_version_flag = true)]
#[command(help_template = HELP_TEMPLATE)]
pub struct InitArgs {
    /// Показать справку
    #[arg(long, short, action = ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Commands {
    /// Инициализация проекта
    Init(InitArgs),

    /// Показать справку
    Help,
}

#[derive(Parser, Debug, PartialEq)]
#[command(name = "eska")]
#[command(about = "Утилита для 1С Разработчиков")]
#[command(version)]
#[command(disable_help_flag = true)]
#[command(disable_version_flag = true)]
#[command(disable_help_subcommand = true)]
#[command(help_template = HELP_TEMPLATE)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Показать справку
    #[arg(long, short, action = ArgAction::Help)]
    help: Option<bool>,

    /// Показать версию
    #[arg(long, short = 'V', action = ArgAction::Version)]
    version: Option<bool>,
}

impl Cli {
    pub async fn run(self) -> Result<String> {
        match self.command {
            Commands::Init(_args) => commands::init::run().await,

            Commands::Help => {
                Self::command().print_help()?;
                Ok(String::new())
            }
        }
    }

    #[must_use]
    pub const fn get_command(&self) -> &Commands {
        &self.command
    }
}
