//! Project onboarding asks the native owner to select a platform or publish into owned storage.

use crate::cli::commands::artifact::{self, Failure, PlatformArgs};
use clap::{Args, Subcommand};
use eska_build_runtime::build::{PlatformVersion, RunError};
use eska_presentation::localization::Localizer;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

#[derive(Debug, Args)]
pub struct BridgeArgs {
    #[command(subcommand)]
    command: Operation,
}

#[derive(Debug, Subcommand)]
enum Operation {
    Doctor(crate::cli::commands::doctor::DoctorArgs),
    SelectPlatform {
        #[command(flatten)]
        platform: PlatformArgs,
        #[arg(long)]
        inherited: Option<String>,
        #[arg(long)]
        init: bool,
    },
    Unpack {
        input: PathBuf,
        destination: PathBuf,
        #[arg(long)]
        stage_near: PathBuf,
        #[command(flatten)]
        platform: PlatformArgs,
    },
}

impl BridgeArgs {
    /// Keep human interaction on inherited streams and emit exactly one private machine reply.
    #[must_use]
    pub fn run(&self, root: &std::path::Path, localizer: &Localizer) -> ExitCode {
        let (operation, init, result) = match &self.command {
            Operation::Doctor(args) => ("doctor", false, Ok(args.inspect(root, localizer))),
            Operation::SelectPlatform {
                platform,
                inherited,
                init,
            } => (
                "select_platform",
                *init,
                select(platform, inherited.as_deref(), *init, localizer),
            ),
            Operation::Unpack {
                input,
                destination,
                stage_near,
                platform,
            } => (
                "unpack",
                false,
                unpack(input, destination, stage_near, platform, localizer),
            ),
        };
        let (data, error, status) = match result {
            Ok(data) => (Some(data), None, ExitCode::SUCCESS),
            Err(error) => {
                if error.code == "cancelled" {
                    // The surrounding operation owns cancellation presentation after cleanup.
                } else if init {
                    eprintln!("{}", error.message);
                } else {
                    error.report();
                }
                (None, Some(error.code), ExitCode::FAILURE)
            }
        };
        let reply = json!({"schema_version":1,"operation":operation,"data":data,"error":error});
        let mut stdout = io::stdout().lock();
        if serde_json::to_writer(&mut stdout, &reply).is_err() || writeln!(stdout).is_err() {
            return ExitCode::FAILURE;
        }
        status
    }
}

/// Verify installed platform identity before the project owner claims a new project directory.
fn select(
    platform: &PlatformArgs,
    inherited: Option<&str>,
    init: bool,
    localizer: &Localizer,
) -> Result<Value, Failure> {
    let inherited = inherited
        .map(PlatformVersion::parse)
        .transpose()
        .map_err(|_| {
            Failure::new(
                "platform-version-invalid",
                localizer.text("artifact-version-invalid"),
            )
        })?;
    let tool = if init {
        platform.resolve_for_init(localizer)
    } else {
        platform.resolve(inherited.as_ref(), true, localizer)
    }?;
    Ok(json!({"platform_version":tool.version().as_str()}))
}

/// Separate native staging location from the caller's exclusively owned transfer namespace.
fn unpack(
    input: &std::path::Path,
    destination: &std::path::Path,
    stage_near: &std::path::Path,
    platform: &PlatformArgs,
    localizer: &Localizer,
) -> Result<Value, Failure> {
    let destination = eska_build_runtime::artifact::new_destination(destination)
        .map_err(|error| artifact::present(&error, localizer))?;
    let tool = platform.resolve(None, true, localizer)?;
    let prepared = artifact::unpack(input, stage_near, &tool, true, localizer)
        .map_err(|error| native_failure(&error, localizer))?;
    let identity = prepared.identity().clone();
    prepared
        .publish_new(&destination, &|| tool.was_interrupted())
        .map_err(|error| native_failure(&error, localizer))?;
    Ok(
        json!({"identity":{"type":identity.project_type.as_str(),"uuid":identity.uuid,"name":identity.name}}),
    )
}

/// Preserve the existing onboarding cancellation code across the private process boundary.
fn native_failure(
    error: &eska_build_runtime::artifact::ArtifactError,
    localizer: &Localizer,
) -> Failure {
    let mut failure = artifact::present(error, localizer);
    if matches!(
        error,
        eska_build_runtime::artifact::ArtifactError::Run(RunError::Interrupted)
    ) {
        failure.code = "cancelled";
    }
    failure
}
