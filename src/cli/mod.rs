pub mod commands;

use crate::error::Result;
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand};

const APP_TEMPLATE: &str = "\
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

const COMMAND_TEMPLATE: &str = "\
{before-help}{name} {version}
{about-with-newline}
ИСПОЛЬЗОВАНИЕ:
    {usage}

АРГУМЕНТЫ:
{positionals}

ОПЦИИ:
{options}
{after-help}
";

#[derive(Args, Debug, PartialEq, Eq)]
#[command(disable_help_flag = true)]
#[command(disable_version_flag = true)]
#[command(help_template = COMMAND_TEMPLATE)]
pub struct FmtArgs {
    /// Путь к файлу или каталогу для форматирования
    #[arg(default_value = ".")]
    pub path: String,

    /// Проверить стиль без изменения файлов (режим CI)
    #[arg(long)]
    pub check: bool,

    /// Показать справку
    #[arg(long, short, action = ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum Commands {
    /// Форматирование исходного кода
    ///
    /// Приводит исходные тексты модулей 1С к каноническому стилю.
    /// Поддерживает файлы *.bsl и *.os. Если указан каталог,
    /// обработка выполняется рекурсивно.
    Fmt(FmtArgs),

    /// Показать справку
    Help,
}

#[derive(Parser, Debug, PartialEq, Eq)]
#[command(name = "eska")]
#[command(about = "Утилита для 1С Разработчиков")]
#[command(version)]
#[command(disable_help_flag = true)]
#[command(disable_version_flag = true)]
#[command(disable_help_subcommand = true)]
#[command(help_template = APP_TEMPLATE)]
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
            Commands::Fmt(_args) => commands::fmt::run().await,

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
