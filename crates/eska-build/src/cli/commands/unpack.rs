//! Standalone native export never replaces an existing project or creates Git metadata.

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Args, ValueEnum};
use eska_build_runtime::{
    artifact::{self, ArtifactError},
    build::RunError,
};
use eska_presentation::{
    encoding::json_path,
    localization::{LocalizationValue, Localizer},
};
use serde_json::{Value, json};

use super::artifact::{self as output, Failure, PlatformArgs};

#[derive(Debug, Args)]
pub struct UnpackArgs {
    input: PathBuf,
    destination: PathBuf,
    #[command(flatten)]
    platform: PlatformArgs,
    #[arg(long,value_enum,default_value_t=Format::Human)]
    format: Format,
    #[arg(short,long,action=clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum Format {
    #[default]
    Human,
    Json,
}

impl UnpackArgs {
    /// Resolve output before starting a platform and publish only an exclusively new directory.
    pub fn run(&self, base: &Path, localizer: &Localizer) -> ExitCode {
        match self.execute(base, localizer) {
            Ok(document) => {
                if matches!(self.format, Format::Json) {
                    println!("{document}");
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                if matches!(self.format, Format::Json) {
                    println!(
                        "{}",
                        json!({"schema_version":1,"kind":"unpack","status":"error","error":{"code":error.code,"data":error.details}})
                    );
                }
                eprintln!("{}", error.message);
                if error.code == "cancelled" {
                    ExitCode::from(130)
                } else {
                    ExitCode::FAILURE
                }
            }
        }
    }

    /// Metadata identity is read from the exported XML, independently of the binary filename.
    fn execute(&self, base: &Path, localizer: &Localizer) -> Result<Value, Failure> {
        let input = absolute_from(base, &self.input);
        let destination = artifact::new_destination(&absolute_from(base, &self.destination))
            .map_err(|error| failure(&error, localizer))?;
        let human = matches!(self.format, Format::Human);
        let tool = self.platform.resolve(None, human, localizer)?;
        let prepared = output::unpack(&input, &destination, &tool, human, localizer)
            .map_err(|error| failure(&error, localizer))?;
        let identity = prepared.identity().clone();
        let path = prepared
            .publish_new(&destination, &|| tool.was_interrupted())
            .map_err(|error| failure(&error, localizer))?;
        if human {
            output::success(&localizer.format(
                "unpack-complete",
                &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
            ));
        }
        let (value, encoding) = json_path(path.as_os_str());
        Ok(
            json!({"schema_version":1,"kind":"unpack","status":"unpacked","destination":{"value":value,"encoding":encoding},"identity":{"project_type":identity.project_type.as_str(),"name":identity.name,"uuid":identity.uuid},"platform_version":tool.version().as_str()}),
        )
    }
}

/// Relative native artifact paths use the explicitly selected base without project discovery.
fn absolute_from(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.into()
    } else {
        base.join(path)
    }
}

/// Distinguish an interrupted owned platform from an ordinary native execution error.
fn failure(error: &ArtifactError, localizer: &Localizer) -> Failure {
    let mut failure = output::present(error, localizer);
    if matches!(error, ArtifactError::Run(RunError::Interrupted)) {
        failure.code = "cancelled";
    }
    failure
}

/// Extend the shared platform help with the standalone export's explicit new output boundary.
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    output::localize(command, localizer)
        .about(localizer.text("unpack-about"))
        .help_template(format!(
            "{{about-with-newline}}\n{}: {{usage}}\n\n{{positionals}}\n{}:\n{{options}}",
            localizer.text("cli-usage"),
            localizer.text("cli-options")
        ))
        .mut_arg("input", |arg| arg.help(localizer.text("unpack-input-help")))
        .mut_arg("destination", |arg| {
            arg.help(localizer.text("unpack-destination-help"))
        })
        .mut_arg("format", |arg| {
            arg.help(localizer.text("version-format-help"))
        })
}
