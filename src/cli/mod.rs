pub mod commands;

use crate::error::Result;
use clap::{Parser, Subcommand};

/// Подкоманды приложения верхнего уровня
#[derive(Subcommand, Debug, PartialEq)]
pub enum Commands {
    /// Инициализация проекта
    Init,
}

/// Точка входа в приложение.
/// Эта структура представляет полный интерфейс командной строки и синтаксический анализ команд верхнего уровня.
#[derive(Parser, Debug, PartialEq)]
#[command(name = "eska")]
#[command(about = "Утилита для 1С Разработчиков")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    /// Отправляет обработанную CLI-команду соответствующему обработчику.
    /// Возвращает результат в виде строки (выходное сообщение или ошибка).
    pub async fn run(self) -> Result<String> {
        match self.command {
            Commands::Init => commands::init::run().await,
        }
    }

    /// Возвращает ссылку на проанализированную команду.
    ///
    /// Этот метод предоставляет доступ к команде, которая была проанализирована с помощью
    /// аргументов командной строки. Полезно для тестирования и самоанализа.
    pub fn get_command(&self) -> &Commands {
        &self.command
    }
}
