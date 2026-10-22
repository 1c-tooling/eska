//! Global machine configuration initialization and transactional editing.

use eska_presentation::arguments::build::config::ConfigCommand;
pub use eska_presentation::arguments::build::config::{ConfigArgs, localize};

use std::{
    env,
    path::Path,
    process::{Command, ExitCode},
};

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    config::{
        GlobalConfigEditOutcome, GlobalConfigError, GlobalConfigInitOutcome, config_path,
        edit_global_at, init_global_at,
    },
};

struct ConfigHandler<'a> {
    options: &'a ConfigArgs,
}

impl ConfigHandler<'_> {
    /// Create or edit the global machine configuration without project discovery.
    pub fn run(&self, localizer: &Localizer) -> ExitCode {
        let path = match config_path() {
            Ok(path) => path,
            Err(error) => return fail(&error, localizer),
        };
        match &self.options.command {
            ConfigCommand::Init(_) => match init_global_at(&path) {
                Ok(GlobalConfigInitOutcome::Created(path)) => {
                    println!("{}", path_message("config-created", &path, localizer));
                    ExitCode::SUCCESS
                }
                Ok(GlobalConfigInitOutcome::Existing(path)) => {
                    println!("{}", path_message("config-existing", &path, localizer));
                    ExitCode::SUCCESS
                }
                Err(error) => fail(&error, localizer),
            },
            ConfigCommand::Edit(_) => match edit_global_at(&path, open_editor) {
                Ok(GlobalConfigEditOutcome::Unchanged(path)) => {
                    println!("{}", path_message("config-unchanged", &path, localizer));
                    ExitCode::SUCCESS
                }
                Ok(GlobalConfigEditOutcome::Changed { path, backup }) => {
                    println!(
                        "{}",
                        localizer.format(
                            "config-updated",
                            &[
                                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                                ("backup", LocalizationValue::Text(&backup.to_string_lossy()),),
                            ],
                        )
                    );
                    ExitCode::SUCCESS
                }
                Err(error) => fail(&error, localizer),
            },
        }
    }
}

/// Open a blocking editor selected from `ESKA_EDITOR`, `VISUAL`, `EDITOR` or OS defaults.
fn open_editor(path: &Path) -> Result<(), GlobalConfigError> {
    let editor = env::var_os("ESKA_EDITOR")
        .or_else(|| env::var_os("VISUAL"))
        .or_else(|| env::var_os("EDITOR"))
        .unwrap_or_else(default_editor);
    let status = Command::new(editor)
        .arg(path)
        .status()
        .map_err(|source| GlobalConfigError::Editor { source })?;
    if status.success() {
        Ok(())
    } else {
        Err(GlobalConfigError::EditorFailed)
    }
}

#[cfg(windows)]
fn default_editor() -> std::ffi::OsString {
    "notepad.exe".into()
}

#[cfg(not(windows))]
fn default_editor() -> std::ffi::OsString {
    "vi".into()
}

fn path_message(key: &str, path: &Path, localizer: &Localizer) -> String {
    localizer.format(
        key,
        &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
    )
}

fn fail(error: &GlobalConfigError, localizer: &Localizer) -> ExitCode {
    eprintln!(
        "{}",
        diagnostics::present_global_config_error(error, localizer)
    );
    ExitCode::FAILURE
}

/// Execute parsed machine configuration options in the native owner.
pub fn run(args: &ConfigArgs, localizer: &Localizer) -> ExitCode {
    ConfigHandler { options: args }.run(localizer)
}
