//! The shell delegates one explicit analysis profile to its compatible standalone checker.

use std::{
    convert::Infallible,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Args, ValueEnum};

use crate::{
    cli::{
        companion::{self, Capability, Kind, Tool},
        localization::Localizer,
    },
    tool_cli::{self, Failure, OutputFormat},
};

/// Wire names are presentation choices; the shell contains no diagnostic rules or analysis host.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum Profile {
    Syntax,
    Semantic,
}

impl Profile {
    /// Match the standalone checker's capability values without locale-dependent spellings.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::Semantic => "semantic",
        }
    }
}

#[derive(Debug, Args)]
pub(in crate::cli) struct CheckArgs {
    #[arg(long, value_enum)]
    profile: Profile,
    #[arg(short = 'p', long)]
    project: Option<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    timeout_seconds: Option<u64>,
    #[arg(long)]
    max_document_bytes: Option<u64>,
    #[arg(long)]
    max_retained_source_bytes: Option<u64>,
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

impl CheckArgs {
    /// Preserve the child's raw result and numeric status; shell admission failures use exit 3.
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        match self.execute(project_dir, localizer) {
            Ok(never) => match never {},
            Err(error) => {
                let _ =
                    tool_cli::emit_failure("eska", Some("check"), self.format, &error, localizer);
                ExitCode::from(3)
            }
        }
    }

    /// The same resolved binary is admitted before execution; project discovery belongs to the child.
    fn execute(&self, project_dir: &Path, localizer: &Localizer) -> Result<Infallible, Failure> {
        let tool = Tool::discover(Kind::Check).map_err(companion_failure)?;
        tool.verify(
            localizer.locale().as_str(),
            Capability::Check(self.profile.as_str()),
        )
        .map_err(companion_failure)?;
        let mut command = tool.command();
        command
            .args(["--lang", localizer.locale().as_str(), "--project-dir"])
            .arg(project_dir)
            .args(["--profile", self.profile.as_str(), "--format"])
            .arg(match self.format {
                OutputFormat::Human => "human",
                OutputFormat::Json => "json",
            });
        if let Some(project) = &self.project {
            command.args(["--project", project]);
        }
        if let Some(config) = &self.config {
            command.arg("--config").arg(config);
        }
        for (option, value) in [
            ("--timeout-seconds", self.timeout_seconds),
            ("--max-document-bytes", self.max_document_bytes),
            (
                "--max-retained-source-bytes",
                self.max_retained_source_bytes,
            ),
        ] {
            if let Some(value) = value {
                command.arg(option).arg(value.to_string());
            }
        }
        companion::inherited::execute(&mut command)
            .map_err(|_| Failure::new("check_tool_execution_failed"))
    }
}

/// Probe failures never leak probe stdout into the child's machine result.
const fn companion_failure(error: companion::Error) -> Failure {
    Failure::new(match error {
        companion::Error::Missing => "check_tool_missing",
        companion::Error::Unavailable => "check_tool_unavailable",
        companion::Error::ProbeFailed => "check_tool_probe_failed",
        companion::Error::ProbeTimeout => "check_tool_probe_timeout",
        companion::Error::ProbeOversized => "check_tool_probe_oversized",
        companion::Error::Incompatible => "check_tool_incompatible",
    })
}

/// Reuse the standalone tool's option descriptions while retaining the shell command name.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    tool_cli::localize(command, localizer, "tool-check")
}
