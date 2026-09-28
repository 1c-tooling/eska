//! Shared platform selection and diagnostics for native artifact onboarding.

use std::{
    io::{self, IsTerminal},
    path::PathBuf,
};

use clap::Args;

use crate::{
    cli::{
        diagnostics,
        interactive::{PromptError, Selector},
        localization::{LocalizationValue, Localizer},
        platform,
    },
    project::{
        artifact::ArtifactError,
        build::{Ibcmd, PlatformVersion},
    },
};

#[derive(Debug, Default, Args)]
pub(super) struct PlatformArgs {
    #[arg(long)]
    ibcmd: Option<PathBuf>,
    #[arg(long)]
    platform_arch: Option<String>,
    #[arg(long)]
    distrobox: Option<String>,
    #[arg(long, conflicts_with = "select_platform")]
    pub platform_version: Option<String>,
    #[arg(long)]
    pub select_platform: bool,
}

pub(super) struct Failure {
    pub code: &'static str,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl Failure {
    /// Keep stable error codes independent from localized messages.
    pub const fn new(code: &'static str, message: String) -> Self {
        Self {
            code,
            message,
            details: None,
        }
    }

    /// Include a machine-readable preview when an import requires a decision.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl PlatformArgs {
    /// Prevent native-import flags from silently affecting ordinary scaffold creation.
    pub const fn is_set(&self) -> bool {
        self.ibcmd.is_some()
            || self.platform_arch.is_some()
            || self.distrobox.is_some()
            || self.platform_version.is_some()
            || self.select_platform
    }

    /// Resolve an explicit, inherited or interactively selected platform without hidden defaults.
    pub fn resolve(
        &self,
        inherited: Option<&PlatformVersion>,
        interactive: bool,
        localizer: &Localizer,
    ) -> Result<Ibcmd, Failure> {
        let options = platform::tool_options(
            self.ibcmd.clone(),
            self.platform_arch.clone(),
            self.distrobox.clone(),
        )
        .map_err(|error| {
            Failure::new(
                "global-config",
                diagnostics::present_global_config_error(&error, localizer),
            )
        })?;
        let version = if let Some(value) = &self.platform_version {
            value.clone()
        } else if !self.select_platform
            && let Some(version) = inherited
        {
            version.as_str().to_owned()
        } else {
            if !interactive || !terminal() {
                return Err(Failure::new(
                    "platform-required",
                    localizer.text("artifact-platform-required"),
                ));
            }
            let installed = Ibcmd::installed(&options).map_err(|error| {
                Failure::new(
                    "platform-discovery",
                    diagnostics::present_tool_error(&error, localizer),
                )
            })?;
            if installed.is_empty() {
                return Err(Failure::new(
                    "platform-not-found",
                    localizer.text("platform-none"),
                ));
            }
            let choices = installed
                .iter()
                .map(|value| {
                    let version = value.version().as_str().to_owned();
                    (version.clone(), version)
                })
                .collect::<Vec<_>>();
            let mut selector = Selector::start("build-platform-tui-title")
                .map_err(|error| prompt_error(error, localizer))?;
            let selected = selector
                .choose_values(localizer, "build-platform-menu", &choices)
                .map_err(|error| prompt_error(error, localizer))?;
            selector
                .finish()
                .map_err(|_| prompt_error(PromptError::Io, localizer))?;
            selected
        };
        let version = PlatformVersion::parse(&version).map_err(|_| {
            Failure::new(
                "platform-version-invalid",
                localizer.text("artifact-version-invalid"),
            )
        })?;
        Ibcmd::discover(&version, &options).map_err(|error| {
            Failure::new(
                "platform-discovery",
                diagnostics::present_tool_error(&error, localizer),
            )
        })
    }
}

/// Require input and diagnostic output to remain visible during interactive decisions.
pub(super) fn terminal() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

/// Reuse the existing prompt cancellation and terminal-error vocabulary.
pub(super) fn prompt_error(error: PromptError, localizer: &Localizer) -> Failure {
    Failure::new(
        "cancelled",
        localizer.text(match error {
            PromptError::Cancelled => "artifact-cancelled",
            PromptError::Io => "new-prompt-error",
        }),
    )
}

/// Translate structured import failures only at the CLI boundary.
pub(super) fn present(error: &ArtifactError, localizer: &Localizer) -> Failure {
    let (code, key, path) = match error {
        ArtifactError::Io { path, source } => {
            return Failure::new(
                "io",
                localizer.format(
                    "artifact-io",
                    &[
                        ("path", LocalizationValue::Text(&path.to_string_lossy())),
                        ("reason", LocalizationValue::Text(&source.to_string())),
                    ],
                ),
            );
        }
        ArtifactError::UnsupportedFile(path) => {
            ("unsupported-file", "artifact-unsupported-file", path)
        }
        ArtifactError::InvalidDescriptor(path) => {
            ("invalid-descriptor", "artifact-invalid-descriptor", path)
        }
        ArtifactError::MissingDescriptor(path) => {
            ("missing-descriptor", "artifact-missing-descriptor", path)
        }
        ArtifactError::AmbiguousDescriptor(path) => (
            "ambiguous-descriptor",
            "artifact-ambiguous-descriptor",
            path,
        ),
        ArtifactError::UnsafePath(path) => ("unsafe-path", "artifact-unsafe-path", path),
        ArtifactError::SourceChanged(path) => ("source-changed", "artifact-source-changed", path),
        ArtifactError::Config(_) => {
            return Failure::new("project-config", localizer.text("artifact-config-error"));
        }
        ArtifactError::Repository(_) => {
            return Failure::new("repository", localizer.text("artifact-repository-error"));
        }
        ArtifactError::ConfirmationRequired => {
            return Failure::new(
                "confirmation-required",
                localizer.text("artifact-confirmation-required"),
            );
        }
        ArtifactError::Rollback { backup, .. } => {
            return Failure::new(
                "rollback",
                localizer.format(
                    "artifact-rollback-error",
                    &[("path", LocalizationValue::Text(&backup.to_string_lossy()))],
                ),
            );
        }
        ArtifactError::TypeMismatch { .. } => {
            return Failure::new("type-mismatch", localizer.text("artifact-type-mismatch"));
        }
        ArtifactError::Run(_) => {
            return Failure::new("platform-run", localizer.text("artifact-platform-run"));
        }
        ArtifactError::Platform { output, .. } => {
            return Failure::new(
                "platform-failed",
                localizer.format(
                    "artifact-platform-failed",
                    &[("reason", LocalizationValue::Text(output))],
                ),
            );
        }
    };
    Failure::new(
        code,
        localizer.format(
            key,
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
    )
}

/// Keep platform flag help aligned across creation and update commands.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .mut_arg("ibcmd", |arg| arg.help(localizer.text("build-ibcmd-help")))
        .mut_arg("platform_arch", |arg| {
            arg.help(localizer.text("build-arch-help"))
        })
        .mut_arg("distrobox", |arg| {
            arg.help(localizer.text("build-distrobox-help"))
        })
        .mut_arg("platform_version", |arg| {
            arg.help(localizer.text("artifact-platform-version-help"))
        })
        .mut_arg("select_platform", |arg| {
            arg.help(localizer.text("artifact-select-platform-help"))
        })
}
