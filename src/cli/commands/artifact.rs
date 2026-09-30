//! Shared platform selection and diagnostics for project onboarding and native imports.

use std::{
    io::{self, IsTerminal},
    path::{Path, PathBuf},
};

use clap::Args;

use crate::{
    cli::{
        diagnostics,
        interactive::{PromptError, Selector},
        localization::{LocalizationValue, Localizer},
        path_output, platform,
        process_output::{
            self, decorate_status, diagnostic_styling_enabled, progress::ProgressLine,
            result_styling_enabled,
        },
    },
    project::{
        artifact::{ArtifactError, PreparedArtifact},
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

/// Stream platform diagnostics with the same color and progress policy used by build.
pub(super) fn unpack(
    input: &Path,
    destination: &Path,
    tool: &Ibcmd,
    human: bool,
    localizer: &Localizer,
) -> Result<PreparedArtifact, ArtifactError> {
    let styled = diagnostic_styling_enabled();
    if human {
        let heading = localizer.format(
            "artifact-unpack-started",
            &[("version", LocalizationValue::Text(tool.version().as_str()))],
        );
        process_output::write_diagnostic(
            decorate_status("▶", &heading, styled, "36").as_bytes(),
            false,
            None,
        )
        .map_err(ArtifactError::Output)?;
    }
    let mut progress = (human && io::stderr().is_terminal())
        .then(|| ProgressLine::start(localizer.text("artifact-unpack-progress"), styled));
    let mut output_error = None;
    let result = PreparedArtifact::unpack(input, destination, tool, |_, line| {
        if output_error.is_none()
            && let Err(error) = process_output::write_diagnostic(line, styled, progress.as_ref())
        {
            output_error = Some(error);
        }
    });
    if let Some(error) = progress
        .as_mut()
        .and_then(|progress| progress.finish().err())
        && output_error.is_none()
    {
        output_error = Some(error);
    }
    let prepared = result?;
    if let Some(error) = output_error {
        return Err(ArtifactError::Output(error));
    }
    Ok(prepared)
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

    /// Distinguish cancellation from failure without decorating nested error reasons twice.
    pub fn report(&self) {
        if self.code == "cancelled" {
            report("↩", &self.message, "33");
        } else {
            report("✗", &self.message, "31");
        }
    }
}

/// Give final success messages the same marker and stdout color policy as build.
pub(super) fn success(message: &str) {
    println!(
        "{}",
        decorate_status("✓", message, result_styling_enabled(), "32")
    );
}

/// Decorate CLI diagnostics while native platform severity lines remain unchanged.
pub(super) fn report(marker: &str, message: &str, color: &str) {
    eprintln!(
        "{}",
        decorate_status(marker, message, diagnostic_styling_enabled(), color)
    );
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
        self.resolve_with_cancellation(inherited, interactive, localizer, "artifact-cancelled")
    }

    /// Reuse native creation's platform selector with initialization-specific cancellation.
    pub fn resolve_for_init(&self, localizer: &Localizer) -> Result<Ibcmd, Failure> {
        self.resolve_with_cancellation(None, true, localizer, "init-cancelled")
    }

    /// Keep selection and installed-version verification identical across onboarding commands.
    fn resolve_with_cancellation(
        &self,
        inherited: Option<&PlatformVersion>,
        interactive: bool,
        localizer: &Localizer,
        cancellation_key: &str,
    ) -> Result<Ibcmd, Failure> {
        let options = platform::tool_options(
            self.ibcmd.clone(),
            self.platform_arch.clone(),
            self.distrobox.clone(),
        )
        .map_err(|error| {
            Failure::new(
                "global-config",
                diagnostics::present_global_config_error_with_links(
                    &error,
                    localizer,
                    io::stderr().is_terminal(),
                ),
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
                    diagnostics::present_tool_error_with_links(
                        &error,
                        localizer,
                        io::stderr().is_terminal(),
                    ),
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
                .map_err(|error| platform_prompt_error(error, localizer, cancellation_key))?;
            let selected = selector
                .choose_values(localizer, "artifact-platform-menu", &choices)
                .map_err(|error| platform_prompt_error(error, localizer, cancellation_key))?;
            selector
                .finish()
                .map_err(|_| platform_prompt_error(PromptError::Io, localizer, cancellation_key))?;
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
                diagnostics::present_tool_error_with_links(
                    &error,
                    localizer,
                    io::stderr().is_terminal(),
                ),
            )
        })
    }
}

/// Localize selector cancellation for the command that owns the interaction.
fn platform_prompt_error(
    error: PromptError,
    localizer: &Localizer,
    cancellation_key: &str,
) -> Failure {
    match error {
        PromptError::Cancelled => Failure::new("cancelled", localizer.text(cancellation_key)),
        PromptError::Io => prompt_error(PromptError::Io, localizer),
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
    let hyperlinks = io::stderr().is_terminal();
    let (code, key, path) = match error {
        ArtifactError::Io { path, source } => {
            return Failure::new(
                "io",
                localizer.format(
                    "artifact-io",
                    &[
                        (
                            "path",
                            LocalizationValue::Text(&path_output::render(path, hyperlinks)),
                        ),
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
                    &[(
                        "path",
                        LocalizationValue::Text(&path_output::render(backup, hyperlinks)),
                    )],
                ),
            );
        }
        ArtifactError::TypeMismatch { .. } => {
            return Failure::new("type-mismatch", localizer.text("artifact-type-mismatch"));
        }
        ArtifactError::Run(_) => {
            return Failure::new("platform-run", localizer.text("artifact-platform-run"));
        }
        ArtifactError::Output(_) => {
            return Failure::new("output", localizer.text("artifact-output-error"));
        }
        ArtifactError::Platform { .. } => {
            return Failure::new(
                "platform-failed",
                localizer.text("artifact-platform-failed"),
            );
        }
    };
    Failure::new(
        code,
        localizer.format(
            key,
            &[(
                "path",
                LocalizationValue::Text(&path_output::render(path, hyperlinks)),
            )],
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
